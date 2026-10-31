//! Event loop (design prompt §20). Connects the terminal, input and the loader threads.
//!
//! No busy loop: wait up to `TICK` for input, then handle listing results and signals that arrived.
//! No tokio (no async runtime in Phase 1).

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event};

use crate::app::{App, Effect, ListingResult, Mode, ViewerResult};
use crate::archive::{self, Action as ArchiveAction, Member as ArchiveMember};
use crate::command;
use crate::fs::spawn_listing;
use crate::license;
use crate::message::Message;
use crate::pack::{self, PackState};
use crate::settings;
use crate::supporter::SupporterInfo;
use crate::terminal::{ExitSignal, TerminalGuard};
use crate::ui;
use crate::ui_viewer::ImageSlot;
use crate::viewer::image::{ProtocolSetting, Resolved, font_size, resolve_protocol};
use crate::viewer::registry::Registry;
use crate::viewer::{self, Document, open_document_cancellable};
use ratatui_image::FontSize;
use ratatui_image::picker::{Picker, ProtocolType};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app_ops::SimpleKind;
use crate::ops::{Answer, JobEvent, simple, worker};

/// Answer of the info loader: request id, details of the cursor item, summary of the marks.
type InfoAnswer = (
    u64,
    Option<crate::info::Info>,
    Option<crate::info::MarkSummary>,
);

/// Answer of the preview loader: request id and the preview (R-200-9).
type PreviewAnswer = (
    u64,
    Result<crate::preview::PreviewDoc, crate::preview::PreviewError>,
);

const TICK: Duration = Duration::from_millis(100);

fn current_minute() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() / 60)
}

pub fn run(start_dir: PathBuf) -> io::Result<()> {
    let config_dir = settings::config_dir();
    let loaded_settings = settings::load_settings(config_dir.as_deref());
    let (delete_mode, delete_warning) = settings::delete_mode(loaded_settings.ops.delete.as_ref());
    let catalog = settings::load_catalog(&loaded_settings, config_dir.as_deref());
    let image_setting = ProtocolSetting::parse(loaded_settings.viewer.image_protocol.as_deref());
    let app_major: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap_or(0);
    let keys = license::build_keys();
    let license = license::load(config_dir.as_deref(), &keys, app_major);
    let packs = pack::load_packs(
        config_dir.as_deref(),
        &license,
        &keys,
        &loaded_settings.packs.disabled,
    );
    let timeout = viewer::external::timeout_from_setting(loaded_settings.providers.timeout_secs);
    let exit_signal = ExitSignal::register()?;
    let mut guard = TerminalGuard::new()?;
    let (tx, rx) = mpsc::channel::<(u64, ListingResult)>();
    let (view_tx, view_rx) = mpsc::channel::<(u64, ViewerResult)>();
    let (archive_view_tx, archive_view_rx) =
        mpsc::channel::<(u64, io::Result<Vec<ArchiveMember>>)>();
    let (archive_job_tx, archive_job_rx) = mpsc::channel::<(u64, io::Result<()>)>();
    let mut registry = Registry::default();
    for pack in &packs {
        if pack.state == PackState::Loaded {
            for spec in &pack.providers {
                registry.register(Box::new(viewer::external::ExternalProvider::new(
                    spec.clone(),
                    timeout,
                )));
            }
        }
    }
    let registry = Arc::new(registry);
    let mut images = ImageSlot::default();
    let mut picker: Option<Option<Picker>> = None;
    let (job_tx, job_rx) = mpsc::channel::<(u64, JobEvent)>();
    let (places_tx, places_rx) = mpsc::channel::<(u64, Vec<crate::ops::place::Place>)>();
    let (info_tx, info_rx) = mpsc::channel::<InfoAnswer>();
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewAnswer>();
    let (opened_tx, opened_rx) = mpsc::channel::<Result<(), String>>();
    // The preview load in flight and its cancel flag (one at a time, see app_preview.rs).
    let mut preview_load: Option<(u64, Arc<AtomicBool>)> = None;
    let mut channels = Channels {
        listing: tx.clone(),
        view: view_tx,
        archive_view: archive_view_tx,
        archive_jobs: archive_job_tx,
        archive_controls: HashMap::new(),
        registry,
        jobs: job_tx,
        places: places_tx,
        controls: HashMap::new(),
        view_controls: HashMap::new(),
        editor: loaded_settings.editor,
        opened: opened_tx,
        open_apps: crate::open::apps_from_settings(&loaded_settings.open.by_extension),
        press_enter: catalog.text("status.press_enter", &[]),
    };

    let (mut app, first) = App::new(start_dir);
    app.delete_mode = delete_mode;
    app.open_extensions = channels.open_apps.keys().cloned().collect();
    app.supporter = SupporterInfo {
        license,
        packs,
        dev_key: license::accepts_dev_key(),
    };
    app.show_frame = loaded_settings.ui.frame.unwrap_or(true);
    app.show_preview = loaded_settings.ui.preview.unwrap_or(true);
    app.preview_images = loaded_settings.ui.preview_images.unwrap_or(true);
    app.vi_keys = loaded_settings.viewer.vi_keys.unwrap_or(false);
    if let Some(value) = delete_warning {
        app.set_status(Message::DeleteSettingUnknown { value });
    }
    apply(&mut app, first, &mut channels, &mut guard)?;

    let mut dirty = true;
    let mut drawn_minute = current_minute();
    loop {
        if exit_signal.raised() {
            cancel_all_views(&channels);
            return Ok(());
        }
        // The clock in the title band (R-200-12).
        let minute = current_minute();
        if minute != drawn_minute {
            drawn_minute = minute;
            dirty = true;
        }
        if dirty {
            // The pane's picture follows the cursor item. Sixel / Kitty / iTerm2 pictures are not
            // always erased by text drawn over them, so one leaving the pane clears the terminal.
            if prepare_pane_image(&app, &mut images, &mut picker, image_setting) {
                guard.terminal.clear()?;
            }
            guard
                .terminal
                .draw(|frame| ui::render(frame, &mut app, &catalog, &mut images))?;
            dirty = false;
            // This frame may have hidden the pane (narrow terminal, another screen).
            if prepare_pane_image(&app, &mut images, &mut picker, image_setting) {
                guard.terminal.clear()?;
                dirty = true;
            }
        }
        while let Ok((id, result)) = rx.try_recv() {
            app.on_listing(id, result);
            dirty = true;
        }
        while let Ok((id, places)) = places_rx.try_recv() {
            app.on_places(id, places);
            dirty = true;
        }
        while let Ok((id, event)) = job_rx.try_recv() {
            if matches!(event, JobEvent::Finished(_)) {
                channels.controls.remove(&id);
            }
            let effect = app.on_job_event(id, event);
            apply(&mut app, effect, &mut channels, &mut guard)?;
            dirty = true;
        }
        while let Ok((id, result)) = view_rx.try_recv() {
            channels.view_controls.remove(&id);
            app.on_viewer(id, result);
            prepare_image(&app, &mut images, &mut picker, image_setting);
            dirty = true;
        }
        while let Ok(result) = opened_rx.try_recv() {
            app.on_open_finished(result);
            dirty = true;
        }
        while let Ok((id, result)) = archive_view_rx.try_recv() {
            app.on_archive_loaded(id, result);
            dirty = true;
        }
        while let Ok((id, result)) = archive_job_rx.try_recv() {
            channels.archive_controls.remove(&id);
            let effect = app.on_archive_finished(id, result);
            apply(&mut app, effect, &mut channels, &mut guard)?;
            dirty = true;
        }
        while let Ok((id, info, marks)) = info_rx.try_recv() {
            app.on_info(id, info, marks);
            dirty = true;
        }
        if let Some(request) = app.want_info() {
            // Read on a thread: a slow or disconnected drive must not freeze the UI.
            let tx = info_tx.clone();
            std::thread::spawn(move || {
                let info = request
                    .path
                    .as_deref()
                    .and_then(|p| crate::info::read_info(p).ok());
                let marks =
                    (!request.marked.is_empty()).then(|| crate::info::summarize(&request.marked));
                let _ = tx.send((request.id, info, marks));
            });
        }
        while let Ok((id, result)) = preview_rx.try_recv() {
            if preview_load.as_ref().is_some_and(|(load, _)| *load == id) {
                preview_load = None;
            }
            app.on_preview(id, result);
            dirty = true;
        }
        if let Some(request) = app.want_preview(std::time::Instant::now()) {
            // A given-up load keeps its (already set) flag; only the newest is tracked.
            let cancel = Arc::new(AtomicBool::new(false));
            preview_load = Some((request.id, Arc::clone(&cancel)));
            let tx = preview_tx.clone();
            std::thread::spawn(move || {
                let result = crate::preview::load_preview_with(
                    &request.path,
                    request.show_hidden,
                    request.thumbnails,
                    &cancel,
                );
                let _ = tx.send((request.id, result));
            });
        }
        if let (Some(stale), Some((load, cancel))) = (app.stale_preview(), &preview_load)
            && stale == *load
        {
            cancel.store(true, Ordering::Relaxed);
        }
        cancel_stale_views(&app, &mut channels);
        if dirty {
            continue;
        }
        if !event::poll(TICK)? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                let effect = if app.wants_text() {
                    let Some(text_key) = command::text_key(key) else {
                        continue;
                    };
                    app.handle_text(text_key)
                } else {
                    let cmd = if app.mode == Mode::Viewer {
                        command::viewer_key(key, app.vi_keys)
                    } else {
                        command::from_key(key)
                    };
                    let Some(cmd) = cmd else {
                        continue;
                    };
                    app.handle(cmd)
                };
                if effect == Effect::Quit {
                    cancel_all_views(&channels);
                    return Ok(());
                }
                if effect == Effect::CycleImageProtocol {
                    let name = cycle_protocol(&mut images, &mut picker, image_setting);
                    // The old method's picture may stay on screen otherwise.
                    guard.terminal.clear()?;
                    if matches!(app.mode, Mode::Normal | Mode::PreviewBrowse) {
                        app.set_status(Message::ImageMode {
                            name: name.to_string(),
                        });
                    }
                }
                apply(&mut app, effect, &mut channels, &mut guard)?;
                cancel_stale_views(&app, &mut channels);
                prepare_image(&app, &mut images, &mut picker, image_setting);
                dirty = true;
            }
            Event::Resize(_, _) => dirty = true,
            _ => {}
        }
    }
}

/// Senders and per-job controls owned by the event loop.
struct Channels {
    listing: mpsc::Sender<(u64, ListingResult)>,
    view: mpsc::Sender<(u64, ViewerResult)>,
    archive_view: mpsc::Sender<(u64, io::Result<Vec<ArchiveMember>>)>,
    archive_jobs: mpsc::Sender<(u64, io::Result<()>)>,
    archive_controls: HashMap<u64, Arc<AtomicBool>>,
    registry: Arc<Registry>,
    jobs: mpsc::Sender<(u64, JobEvent)>,
    places: mpsc::Sender<(u64, Vec<crate::ops::place::Place>)>,
    /// Running jobs: answer channel and cancel flag (P3-6, P3-11).
    controls: HashMap<u64, (mpsc::Sender<Answer>, Arc<AtomicBool>)>,
    /// Viewer opens in flight, cancelled when their id is no longer pending.
    view_controls: HashMap<u64, Arc<AtomicBool>>,
    editor: settings::EditorSettings,
    /// Outside applications started on a thread report here.
    opened: mpsc::Sender<Result<(), String>>,
    open_apps: HashMap<String, crate::open::OpenApp>,
    press_enter: String,
}

fn cancel_stale_views(app: &App, channels: &mut Channels) {
    let pending = app.pending_view_id();
    channels.view_controls.retain(|id, flag| {
        if Some(*id) == pending {
            true
        } else {
            flag.store(true, Ordering::Relaxed);
            false
        }
    });
}

fn cancel_all_views(channels: &Channels) {
    for flag in channels.view_controls.values() {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Executes an Effect other than Quit. Effects that finish immediately (Rename, mkdir, editor)
/// feed their result back into `app` and apply the follow-up effect.
fn apply(
    app: &mut App,
    effect: Effect,
    channels: &mut Channels,
    guard: &mut TerminalGuard,
) -> io::Result<()> {
    match effect {
        Effect::OpenArchive { id, path } => {
            let tx = channels.archive_view.clone();
            std::thread::spawn(move || {
                let _ = tx.send((id, archive::list(&path)));
            });
        }
        Effect::StartArchive { id, action } => {
            let tx = channels.archive_jobs.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            channels.archive_controls.insert(id, Arc::clone(&cancel));
            std::thread::spawn(move || {
                let result = match action {
                    ArchiveAction::Create {
                        destination,
                        sources,
                    } => archive::create(&destination, &sources, &cancel),
                    ArchiveAction::Extract {
                        source,
                        destination,
                    } => archive::extract(&source, &destination, &cancel),
                };
                let _ = tx.send((id, result));
            });
        }
        Effect::CancelArchive { id } => {
            if let Some(cancel) = channels.archive_controls.get(&id) {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        Effect::StartJob(job) => {
            let (answer_tx, answer_rx) = mpsc::channel::<Answer>();
            let cancel = Arc::new(AtomicBool::new(false));
            channels
                .controls
                .insert(job.id, (answer_tx, Arc::clone(&cancel)));
            let events = channels.jobs.clone();
            std::thread::spawn(move || {
                // Forward worker events tagged with the job id.
                let (tx, rx) = mpsc::channel::<JobEvent>();
                let id = job.id;
                let forward = std::thread::spawn(move || {
                    for event in rx {
                        if events.send((id, event)).is_err() {
                            break;
                        }
                    }
                });
                worker::run(&job, &tx, &answer_rx, &cancel, &worker::Hooks::default());
                drop(tx);
                let _ = forward.join();
            });
        }
        Effect::AnswerConflict { job_id, answer } => {
            if let Some((tx, _)) = channels.controls.get(&job_id) {
                let _ = tx.send(answer);
            }
        }
        Effect::ListPlaces { id } => {
            // Built on a thread: probing a disconnected network drive can be slow.
            let tx = channels.places.clone();
            std::thread::spawn(move || {
                let _ = tx.send((id, crate::ops::place::list_places_real()));
            });
        }
        Effect::CancelJob { job_id } => {
            if let Some((_, cancel)) = channels.controls.get(&job_id) {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        Effect::Rename { dir, old, new } => {
            let result = simple::rename(&dir, &old, &new);
            let next = app.on_simple_result(SimpleKind::Rename { old }, result);
            apply(app, next, channels, guard)?;
        }
        Effect::MakeDir { dir, name } => {
            let result = simple::make_dir(&dir, &name);
            let next = app.on_simple_result(SimpleKind::Mkdir, result);
            apply(app, next, channels, guard)?;
        }
        Effect::OpenEditor { path } => {
            let result = run_editor(&path, &channels.editor, guard);
            let next = app.on_editor_finished(result);
            apply(app, next, channels, guard)?;
        }
        Effect::OpenExternal { path } => match crate::open::classify(&path) {
            crate::open::Launch::Document => open_with_os(path, channels),
            crate::open::Launch::RunDirectly | crate::open::Launch::RunThroughOs => {
                app.ask_run(path);
            }
        },
        Effect::RunExecutable { path } => match crate::open::classify(&path) {
            crate::open::Launch::RunDirectly => {
                let result = run_program(&path, &channels.press_enter, guard);
                app.on_open_finished(result);
            }
            _ => open_with_os(path, channels),
        },
        Effect::OpenWithApp { path } => {
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase());
            let found = extension.and_then(|e| channels.open_apps.get(&e).cloned());
            if let Some(open_app) = found {
                if open_app.wait {
                    let result = run_app_waiting(&open_app, &path, guard);
                    app.on_open_finished(result);
                } else {
                    let tx = channels.opened.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::open::run_app_detached(&open_app, &path));
                    });
                }
            }
        }
        Effect::PrefillCommand { path } => {
            let name = path.file_name().unwrap_or(path.as_os_str());
            let is_program = crate::shell_cmd::runnable_in_shell(
                &name.to_string_lossy(),
                crate::open::classify(&path),
                app.shell_kind,
            );
            app.on_command_classified(path, is_program);
        }
        Effect::RunShell { dir, line } => {
            let result = run_shell(&dir, &line, app.shell_kind, &channels.press_enter, guard);
            let next = app.on_command_finished(result);
            apply(app, next, channels, guard)?;
        }
        Effect::Load { id, dir, .. } => spawn_listing(id, dir, channels.listing.clone()),
        Effect::OpenViewer {
            id,
            path,
            kind,
            encoding,
        } => {
            let tx = channels.view.clone();
            let registry = Arc::clone(&channels.registry);
            let flag = Arc::new(AtomicBool::new(false));
            channels.view_controls.insert(id, Arc::clone(&flag));
            std::thread::spawn(move || {
                let result = open_document_cancellable(&registry, &path, kind, encoding, &flag);
                let _ = tx.send((id, result));
            });
        }
        Effect::Suspend => {
            guard.suspend()?;
            app.set_status(Message::Resumed);
        }
        // Handled by the loop, which owns the image state.
        Effect::None | Effect::Quit | Effect::CycleImageProtocol => {}
    }
    Ok(())
}

/// Resolves the start directory from the optional `fds [path]` argument (R-111-1).
///
/// - No argument: `cwd`.
/// - Relative paths are resolved against `cwd`, so Parent can walk up from them.
/// - Err (a message for stderr) if the result is not an existing directory.
pub fn resolve_start_dir(arg: Option<OsString>, cwd: &Path) -> Result<PathBuf, String> {
    let dir = match arg {
        Some(arg) => cwd.join(arg),
        None => cwd.to_path_buf(),
    };
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", dir.display()));
    }
    std::path::absolute(&dir).map_err(|e| format!("cannot resolve {}: {e}", dir.display()))
}

/// Keeps `images` in sync with the open viewer (P2-11).
///
/// The protocol is resolved once, from config and environment variables only (ADR 0002): the
/// terminal is never queried, so the UI never blocks and no thread reads stdin behind our back.
fn prepare_image(
    app: &App,
    images: &mut ImageSlot,
    picker: &mut Option<Option<Picker>>,
    setting: ProtocolSetting,
) {
    let image = app.viewer.as_ref().and_then(|v| match &v.doc {
        Document::Image(doc) => doc
            .image
            .as_ref()
            .map(|img| (format!("{}|{:?}", v.path.display(), v.kind), img.clone())),
        _ => None,
    });
    let Some((owner, img)) = image else {
        // Only the viewer's part: the preview pane's picture is kept (`prepare_pane_image`).
        images.protocol = None;
        images.owner = None;
        images.protocol_name = None;
        return;
    };
    if images.owner.as_deref() == Some(owner.as_str()) {
        return;
    }
    images.owner = Some(owner);
    match picker.get_or_insert_with(|| make_picker(setting)) {
        Some(p) => {
            images.protocol_name = Some(protocol_name(p.protocol_type()));
            images.protocol = Some(p.new_resize_protocol(img));
        }
        None => {
            images.protocol = None;
            images.protocol_name = None;
        }
    }
}

/// Builds the picker for the resolved protocol. None means "metadata only".
fn make_picker(setting: ProtocolSetting) -> Option<Picker> {
    match resolve_protocol(setting, &|key| std::env::var(key).ok()) {
        Resolved::MetadataOnly => None,
        Resolved::Protocol(ProtocolSetting::Halfblocks) => Some(Picker::halfblocks()),
        Resolved::Protocol(protocol) => {
            let (w, h) = ratatui::crossterm::terminal::window_size()
                .map(|s| font_size(s.columns, s.rows, s.width, s.height))
                .unwrap_or_else(|_| font_size(0, 0, 0, 0));
            // Deprecated upstream in favour of `from_query_stdio`, which we must not use (ADR 0002).
            #[allow(deprecated)]
            let mut p = Picker::from_fontsize(FontSize::new(w, h));
            if let Some(t) = protocol_type(protocol) {
                p.set_protocol_type(t);
            }
            Some(p)
        }
    }
}

fn protocol_type(setting: ProtocolSetting) -> Option<ProtocolType> {
    match setting {
        ProtocolSetting::Kitty => Some(ProtocolType::Kitty),
        ProtocolSetting::Sixel => Some(ProtocolType::Sixel),
        ProtocolSetting::Iterm2 => Some(ProtocolType::Iterm2),
        ProtocolSetting::Halfblocks => Some(ProtocolType::Halfblocks),
        _ => None,
    }
}

fn protocol_name(protocol: ProtocolType) -> &'static str {
    match protocol {
        ProtocolType::Halfblocks => "halfblocks",
        ProtocolType::Sixel => "sixel",
        ProtocolType::Kitty => "kitty",
        ProtocolType::Iterm2 => "iterm2",
    }
}

/// Switches to the next protocol (halfblocks -> sixel -> kitty -> iterm2 -> halfblocks) and forces
/// the current image to be re-encoded. With `image_protocol = "none"` it starts from half blocks.
/// Switches to the next image protocol and returns its name.
fn cycle_protocol(
    images: &mut ImageSlot,
    picker: &mut Option<Option<Picker>>,
    setting: ProtocolSetting,
) -> &'static str {
    let slot = picker.get_or_insert_with(|| make_picker(setting));
    let p = slot.get_or_insert_with(Picker::halfblocks);
    let next = p.protocol_type().next();
    p.set_protocol_type(next);
    // Drop the old protocol state (viewer and pane); `prepare_image` and `prepare_pane_image`
    // rebuild it for the new protocol.
    *images = ImageSlot::default();
    protocol_name(next)
}

/// Builds or drops the preview pane's picture (`ui_preview::prepare_preview_image`). The picker
/// is made only when a picture is to be shown. True: clear the terminal before the next frame.
fn prepare_pane_image(
    app: &App,
    images: &mut ImageSlot,
    picker: &mut Option<Option<Picker>>,
    setting: ProtocolSetting,
) -> bool {
    use crate::app_preview::PreviewView;
    use crate::preview::PreviewDoc;
    let picture = matches!(
        app.preview_view(),
        PreviewView::Ready(PreviewDoc::Picture { .. })
    );
    let picker = if picture {
        picker.get_or_insert_with(|| make_picker(setting)).as_ref()
    } else {
        picker.as_ref().and_then(Option::as_ref)
    };
    crate::ui_preview::prepare_preview_image(app, images, picker)
}

/// Runs the external editor on `path` with the terminal restored (P3-15, R-1200-4).
/// Never uses a shell: the command is split by `editor_cmd::split_command`.
fn run_editor(
    path: &Path,
    settings: &settings::EditorSettings,
    guard: &mut TerminalGuard,
) -> Result<(), String> {
    use crate::editor_cmd::{EditorSources, choose};
    let visual = std::env::var("VISUAL").ok();
    let editor = std::env::var("EDITOR").ok();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let sources = EditorSources {
        config_default: settings.command.as_deref(),
        config_by_extension: Some(&settings.by_extension),
        visual: visual.as_deref(),
        editor: editor.as_deref(),
        windows: cfg!(windows),
        edit_on_path: cfg!(windows) && on_path("edit.exe"),
    };
    let argv = choose(&sources, extension.as_deref());
    let Some((program, args)) = argv.split_first() else {
        return Err("no editor".to_string());
    };
    let status = guard
        .run_external(|| {
            std::process::Command::new(program)
                .args(args)
                .arg(path)
                .status()
        })
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program}: {status}"))
    }
}

fn open_with_os(path: PathBuf, channels: &Channels) {
    let tx = channels.opened.clone();
    std::thread::spawn(move || {
        let _ = tx.send(crate::open::open_with_os(&path));
    });
}

/// Runs a program the user said y to, in the terminal and in its own folder (R-110-3). Waits for
/// Enter afterwards, or its output would vanish as soon as the list comes back.
fn run_program(path: &Path, press_enter: &str, guard: &mut TerminalGuard) -> Result<(), String> {
    let mut command = std::process::Command::new(path);
    if let Some(dir) = path.parent() {
        command.current_dir(dir);
    }
    guard
        .run_external(|| {
            let status = command.status()?;
            wait_for_enter(status, press_enter);
            Ok(())
        })
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Runs a typed line through the shell, in the terminal and in the listed folder (R-412). A failing
/// command is not an error: its status is on screen. Err: the shell could not start.
fn run_shell(
    dir: &Path,
    line: &str,
    kind: crate::shell_cmd::ShellKind,
    press_enter: &str,
    guard: &mut TerminalGuard,
) -> Result<(), String> {
    let mut command = crate::shell_cmd::shell_command(line, kind);
    command.current_dir(dir);
    guard
        .run_external(|| {
            println!("> {}", crate::text::sanitize_for_display(line));
            let status = command.status()?;
            wait_for_enter(status, press_enter);
            Ok(())
        })
        .map_err(|e| format!("{}: {e}", dir.display()))
}

/// Shows how the program ended and waits for Enter, or its output would vanish as soon as the list
/// comes back. A read that fails also just means going back.
fn wait_for_enter(status: std::process::ExitStatus, press_enter: &str) {
    println!("\n[{status}] {press_enter}");
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}

/// Gives a `[open.by_extension]` application the terminal until it ends (R-110-4).
fn run_app_waiting(
    open_app: &crate::open::OpenApp,
    path: &Path,
    guard: &mut TerminalGuard,
) -> Result<(), String> {
    let Some((program, args)) = open_app.argv.split_first() else {
        return Err("no command".to_string());
    };
    let status = guard
        .run_external(|| {
            std::process::Command::new(program)
                .args(args)
                .arg(path)
                .status()
        })
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program}: {status}"))
    }
}

/// Whether `file` exists in one of the PATH directories.
fn on_path(file: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(file).is_file()))
        .unwrap_or(false)
}
