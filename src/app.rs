//! App state (design prompt §8, §15). Applies Commands and returns required I/O as Effects.
//!
//! Touches neither the filesystem nor the terminal, so it is unit-testable as is.

use std::path::PathBuf;

pub use crate::app_ops::{ConflictInfo, InputPurpose, InputState, JobState, PendingOp};
use crate::command::Command;
use crate::entry::{EntryKind, FileEntry, LinkTarget};
use crate::message::Message;
use crate::ops::{Answer, Job, OpRecord};
use crate::pane::{CursorFocus, Pane};
use crate::viewer::view::ViewerState;
use crate::viewer::{Opened, ViewError, ViewerKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// The cursor is in the read-only folder list of the preview pane (`App::peek`, R-200-13).
    PreviewBrowse,
    /// An archive's entries are being read on a loader thread.
    ArchiveLoading,
    /// Read-only hierarchical browser for a ZIP/tar archive.
    ArchiveBrowse,
    /// Archive creation or extraction is running on a worker thread.
    ArchiveRunning,
    Help,
    /// A file is being opened for the viewer. q / Esc / F3 cancel (P2-13).
    ViewerLoading,
    Viewer,
    /// Help opened from the viewer; closing it returns to the viewer.
    ViewerHelp,
    /// Copy / Move: choosing the destination by navigating the list (`App::pending_op`).
    SelectDestination,
    /// Delete: waiting for y / n (`App::delete_targets`).
    ConfirmDelete,
    /// Esc in the list: waiting for y / n before quitting (R-200-14).
    ConfirmQuit,
    /// `o` on a program: only y runs it (`App::run_target`, R-110-3).
    ConfirmRun,
    /// Drives / home / mount points popup (`App::places`).
    Places,
    /// A Copy / Move / Delete job is running (`App::job`).
    Running,
    /// The running job asks what to do with an existing destination (`App::conflict`).
    Conflict,
    /// One-line input: Rename / mkdir / search (`App::input`).
    Input,
    /// Failures of the last job (`App::failures`). Any key closes.
    ErrorList,
}

/// I/O requested from the caller (the event loop).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    OpenArchive {
        id: u64,
        path: PathBuf,
    },
    StartArchive {
        id: u64,
        action: crate::archive::Action,
    },
    CancelArchive {
        id: u64,
    },
    /// Read `dir` and pass the result to `App::on_listing` with the same id.
    Load {
        id: u64,
        dir: PathBuf,
        focus: CursorFocus,
    },
    /// Open `path` in the viewer (with `kind`, or the best one) and pass the result to
    /// `App::on_viewer` with the same id. Runs on a loader thread. `encoding` is the encoding
    /// forced with `E` (None: detect); see `viewer::open_document_with`.
    OpenViewer {
        id: u64,
        path: PathBuf,
        kind: Option<ViewerKind>,
        encoding: Option<&'static encoding_rs::Encoding>,
    },
    /// `o`: the event loop looks at the file, then opens it with the OS or calls `App::ask_run`.
    OpenExternal {
        path: PathBuf,
    },
    /// The user said y to running `path`.
    RunExecutable {
        path: PathBuf,
    },
    /// Enter on a file whose extension has an application in `[open.by_extension]`.
    OpenWithApp {
        path: PathBuf,
    },
    /// `x` on a file with no marks: the event loop looks at the file, then calls
    /// `App::on_command_classified` (R-412).
    PrefillCommand {
        path: PathBuf,
    },
    /// Run `line` through the shell in `dir` with the terminal handed over (R-412); the result goes
    /// to `App::on_command_finished`.
    RunShell {
        dir: PathBuf,
        line: String,
    },
    /// Switch the image viewer to the next image protocol (terminal specific, so done by the
    /// event loop).
    CycleImageProtocol,
    /// List drives, home and mount points on a loader thread; the result goes to
    /// `App::on_places` with the same id.
    ListPlaces {
        id: u64,
    },
    /// Start a Copy / Move / Delete job on a worker thread. Events come back through
    /// `App::on_job_event` with `job.id`.
    StartJob(Job),
    /// Answer the running job's conflict question.
    AnswerConflict {
        job_id: u64,
        answer: Answer,
    },
    /// Ask the running job to stop.
    CancelJob {
        job_id: u64,
    },
    /// Rename `dir/old` to `dir/new`; result to `App::on_simple_result`.
    Rename {
        dir: PathBuf,
        old: String,
        new: String,
    },
    /// Create `dir/name`; result to `App::on_simple_result`.
    MakeDir {
        dir: PathBuf,
        name: String,
    },
    /// Run the external editor on `path` (terminal restored meanwhile); result to
    /// `App::on_editor_finished`.
    OpenEditor {
        path: PathBuf,
    },
    Quit,
    Suspend,
}

/// Viewer open result.
pub type ViewerResult = Result<Opened, ViewError>;

/// Listing result. Err holds the OS error text (the reason).
pub type ListingResult = Result<Vec<FileEntry>, String>;

#[derive(Debug)]
pub struct App {
    pub pane: Pane,
    pub archive: Option<crate::app_archive::ArchiveView>,
    pub(crate) pending_archive: Option<(u64, PathBuf)>,
    pub archive_job: Option<crate::app_archive::ArchiveJob>,
    pub mode: Mode,
    pub status: Option<Message>,
    /// Load in flight. Results with any other id are stale and dropped.
    pending: Option<(u64, PathBuf, CursorFocus)>,
    /// Viewer open in flight: (id, display name, preserve text position on encoding switch).
    pending_view: Option<(u64, String, bool)>,
    /// The open viewer (Mode::Viewer / ViewerHelp).
    pub viewer: Option<ViewerState>,
    pub(crate) next_id: u64,
    /// Mode to return to when Help (F1) closes.
    pub(crate) help_return: Mode,
    /// Copy / Move waiting for a destination (Mode::SelectDestination).
    pub pending_op: Option<PendingOp>,
    /// Targets shown in the delete confirmation (Mode::ConfirmDelete).
    pub delete_targets: Vec<PathBuf>,
    /// The running job, if any (Mode::Running / Conflict).
    pub job: Option<JobState>,
    /// The question shown in Mode::Conflict.
    pub conflict: Option<ConflictInfo>,
    /// One-line input state (Mode::Input).
    pub input: Option<InputState>,
    /// Failures of the last finished job (shown in Mode::ErrorList).
    pub failures: Vec<(PathBuf, String)>,
    /// Destination selection: the cursor is on the "copy/move here" row above the list.
    pub dest_on_here: bool,
    /// The shell that typed commands run in (R-412). A field so tests can check both.
    pub shell_kind: crate::shell_cmd::ShellKind,
    /// Commands typed in this session, oldest first, as typed (`{files}` not expanded). Kept in
    /// memory only: a line may hold a password.
    pub command_history: Vec<String>,
    /// Draw a frame around the screen (setting `[ui] frame`, default on).
    pub show_frame: bool,
    /// What F8 does (setting `[ops] delete`, default permanent; M1-a). After `y` in the
    /// confirmation: Permanent -> `Op::Delete`, Trash -> `Op::Trash`. The confirmation text says
    /// which one will happen.
    pub delete_mode: crate::ops::DeleteMode,
    /// Supporter license and packs, shown on the Help screen (Phase 4). Set once by `run`.
    pub supporter: crate::supporter::SupporterInfo,
    /// Show the info panel (details of the item under the cursor); toggled with `i`.
    pub show_info: bool,
    /// Details of `info.path`, once loaded. Only valid for that path (the UI checks).
    pub info: Option<crate::info::Info>,
    /// Summary of the marked items, when something is marked.
    pub info_marks: Option<crate::info::MarkSummary>,
    /// What was last requested (cursor path, marked paths), so the same target is not asked twice.
    pub(crate) info_key: Option<(Option<PathBuf>, Vec<PathBuf>)>,
    pub(crate) pending_info: Option<u64>,
    /// Show the preview pane right of the list (setting `[ui] preview`, default on; toggled with
    /// `P`). See `app_preview.rs`.
    pub show_preview: bool,
    /// The last preview answer and the path it belongs to. Only valid for that path.
    pub(crate) preview: Option<(
        PathBuf,
        Result<crate::preview::PreviewDoc, crate::preview::PreviewError>,
    )>,
    /// Target of the last preview request: (cursor path, show_hidden).
    pub(crate) preview_key: Option<(Option<PathBuf>, bool)>,
    /// Preview load in flight: (id, path, started).
    pub(crate) pending_preview: Option<(u64, PathBuf, std::time::Instant)>,
    /// Whether the last frame drew the preview pane (set by `ui::render`; true until the first
    /// frame). Nothing is loaded for a pane that is not on screen (narrow terminal, other screens).
    pub preview_shown: bool,
    /// The folder list in the preview pane while `Mode::PreviewBrowse`, and while a file opened
    /// from it is in the viewer or help is open (R-200-13).
    pub peek: Option<crate::app_preview::PeekView>,
    /// Whether the last frame had room to split the pane (set by `ui::render`).
    pub peek_fits: bool,
    /// The program waiting for y (Mode::ConfirmRun).
    pub run_target: Option<PathBuf>,
    /// Lowercase extensions with an application in `[open.by_extension]`.
    pub open_extensions: std::collections::HashSet<String>,
    /// vi-style keys in the viewer (setting `[viewer] vi_keys`, default off): the event loop maps
    /// keys with `command::viewer_key` and the viewer help lists `command::viewer_key_help`.
    pub vi_keys: bool,
    /// Draw pictures in the preview pane (setting `[ui] preview_images`, default on; R-200-10).
    /// Off: images show their format and size only, as before.
    pub preview_images: bool,
    /// Places popup (Mode::Places): the entries, the cursor, the mode to go back to and the
    /// request in flight.
    pub places: Vec<crate::ops::place::Place>,
    pub places_cursor: usize,
    pub(crate) places_return: Mode,
    pub(crate) pending_places: Option<u64>,
    /// Finished jobs, newest last, at most `ops::HISTORY_LIMIT` (P3-16).
    pub history: Vec<OpRecord>,
}

impl App {
    /// Returns the initial App and the first load request (R-111-1).
    pub fn new(start_dir: PathBuf) -> (Self, Effect) {
        let mut app = Self {
            pane: Pane::new(start_dir.clone()),
            archive: None,
            pending_archive: None,
            archive_job: None,
            mode: Mode::Normal,
            status: None,
            pending: None,
            pending_view: None,
            viewer: None,
            next_id: 0,
            help_return: Mode::Normal,
            pending_op: None,
            delete_targets: Vec::new(),
            job: None,
            conflict: None,
            input: None,
            failures: Vec::new(),
            dest_on_here: false,
            show_frame: true,
            delete_mode: crate::ops::DeleteMode::Permanent,
            supporter: crate::supporter::SupporterInfo::default(),
            show_info: true,
            info: None,
            info_marks: None,
            shell_kind: crate::shell_cmd::ShellKind::current(),
            command_history: Vec::new(),
            info_key: None,
            pending_info: None,
            show_preview: true,
            preview: None,
            preview_key: None,
            pending_preview: None,
            preview_shown: true,
            peek: None,
            peek_fits: true,
            run_target: None,
            open_extensions: std::collections::HashSet::new(),
            vi_keys: false,
            preview_images: true,
            places: Vec::new(),
            places_cursor: 0,
            places_return: Mode::Normal,
            pending_places: None,
            history: Vec::new(),
        };
        let effect = app.request_load(start_dir, CursorFocus::First);
        (app, effect)
    }

    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Requests a load. If the same directory is already loading, nothing new is requested
    /// (mashing Enter on a slow filesystem must not spawn a thread per key press).
    pub(crate) fn request_load(&mut self, dir: PathBuf, focus: CursorFocus) -> Effect {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, pending_dir, _)| *pending_dir == dir)
        {
            return Effect::None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.pending = Some((id, dir.clone(), focus.clone()));
        Effect::Load { id, dir, focus }
    }

    /// Handles one Command.
    ///
    /// While Help is shown, any Command just closes Help (the Command itself is not executed).
    pub fn handle(&mut self, cmd: Command) -> Effect {
        if cmd == Command::ForceQuit {
            return Effect::Quit;
        }
        match self.mode {
            Mode::Help => {
                self.mode = self.help_return;
                self.help_return = Mode::Normal;
                return Effect::None;
            }
            Mode::ViewerHelp => {
                self.mode = Mode::Viewer;
                return Effect::None;
            }
            Mode::ViewerLoading => return self.handle_loading(cmd),
            Mode::Viewer => return self.handle_viewer(cmd),
            Mode::PreviewBrowse => return self.handle_peek(cmd),
            Mode::ArchiveLoading => return self.handle_archive_loading(cmd),
            Mode::ArchiveBrowse => return self.handle_archive_browse(cmd),
            Mode::ArchiveRunning => return self.handle_archive_running(cmd),
            Mode::SelectDestination => return self.handle_select_destination(cmd),
            Mode::Places => return self.handle_places(cmd),
            Mode::Running => return self.handle_running(cmd),
            Mode::ErrorList => {
                self.mode = Mode::Normal;
                self.failures.clear();
                return Effect::None;
            }
            // Text modes are driven by `handle_text`; a Command reaching them is ignored.
            Mode::ConfirmDelete
            | Mode::ConfirmQuit
            | Mode::ConfirmRun
            | Mode::Conflict
            | Mode::Input => {
                return Effect::None;
            }
            Mode::Normal => {}
        }
        self.status = None;
        match cmd {
            Command::CursorUp => self.pane.cursor_up(),
            Command::CursorDown => self.pane.cursor_down(),
            Command::PageUp => self.pane.page_up(),
            Command::PageDown => self.pane.page_down(),
            Command::Home => self.pane.cursor_home(),
            Command::End => self.pane.cursor_end(),
            Command::Enter => return self.enter(),
            Command::OpenExternal => {
                let entry = self.pane.cursor_entry().cloned();
                return self.open_external(entry);
            }
            // Left is Parent in the list (like h); in the viewer it scrolls.
            Command::ParentDirectory | Command::ScrollLeft => return self.parent(),
            Command::ToggleMark => {
                self.pane.toggle_mark();
            }
            Command::Help => self.mode = Mode::Help,
            Command::Refresh => {
                let dir = self.pane.current_dir.clone();
                return self.request_load(dir, CursorFocus::Keep);
            }
            Command::ToggleHidden => {
                self.pane.toggle_hidden();
                self.status = Some(if self.pane.show_hidden {
                    Message::HiddenShown
                } else {
                    Message::HiddenHidden
                });
            }
            Command::ToggleInfo => {
                self.show_info = !self.show_info;
                self.info_key = None;
            }
            Command::TogglePreview => {
                self.show_preview = !self.show_preview;
                self.preview_key = None;
            }
            Command::View => {
                if let Some(path) = self.archive_cursor_path() {
                    return self.open_archive(path);
                }
                return self.view_cursor_entry();
            }
            Command::Edit => return self.edit_cursor_entry(),
            Command::Copy => self.begin_select(crate::ops::Op::Copy),
            Command::Move => self.begin_select(crate::ops::Op::Move),
            Command::Delete => self.begin_delete(),
            Command::Rename => self.begin_rename(),
            Command::MakeDir => self.begin_input(InputPurpose::Mkdir),
            Command::ArchiveCreate => self.begin_archive_create(),
            Command::ArchiveExtract => self.begin_archive_extract(),
            Command::Search => self.begin_search(Mode::Normal),
            Command::Places => return self.request_places(),
            Command::RunCommand => self.begin_input(InputPurpose::Command {
                history_pos: None,
                draft: String::new(),
            }),
            Command::RunCommandOnTarget => return self.begin_command_on_target(),
            Command::Quit | Command::Back => self.mode = Mode::ConfirmQuit,
            Command::ForceQuit => return Effect::Quit,
            Command::Suspend => return Effect::Suspend,
            // p switches how the pane's picture is drawn (R-200-10); without a picture there is
            // nothing to show the change on.
            Command::NextImageProtocol => return self.image_protocol_effect(),
            Command::NextViewer => self.enter_peek(),
            Command::ToggleWrap | Command::ScrollRight | Command::NextEncoding => {}
        }
        Effect::None
    }

    pub(crate) fn image_protocol_effect(&self) -> Effect {
        if matches!(
            self.preview_view(),
            crate::app_preview::PreviewView::Ready(crate::preview::PreviewDoc::Picture { .. })
        ) {
            Effect::CycleImageProtocol
        } else {
            Effect::None
        }
    }

    /// The list screen to come back to from the viewer: the pane's list if it was opened there.
    pub(crate) fn list_mode(&self) -> Mode {
        if self.peek.is_some() {
            Mode::PreviewBrowse
        } else {
            Mode::Normal
        }
    }

    /// Commands while a viewer is loading: only cancel and suspend do anything.
    fn handle_loading(&mut self, cmd: Command) -> Effect {
        match cmd {
            Command::Quit | Command::Back | Command::View => {
                self.pending_view = None;
                self.mode = if self.viewer.is_some() {
                    Mode::Viewer
                } else {
                    self.list_mode()
                };
                Effect::None
            }
            Command::Suspend => Effect::Suspend,
            _ => Effect::None,
        }
    }

    /// Commands inside the viewer (P2-3, P2-9, P2-12).
    fn handle_viewer(&mut self, cmd: Command) -> Effect {
        if self.viewer.is_none() {
            self.mode = self.list_mode();
            return Effect::None;
        }
        let Some(viewer) = self.viewer.as_mut() else {
            return Effect::None;
        };
        // A message (e.g. a viewer fallback) is shown until the next key, then the viewer's
        // own status (encoding, line) comes back.
        self.status = None;
        match cmd {
            Command::CursorUp => viewer.scroll_up(1),
            Command::CursorDown => viewer.scroll_down(1),
            Command::PageUp => viewer.page_up(),
            Command::PageDown => viewer.page_down(),
            Command::Home => viewer.home(),
            Command::End => viewer.end(),
            Command::ScrollLeft => viewer.scroll_left(),
            Command::ScrollRight => viewer.scroll_right(),
            Command::ToggleWrap => viewer.toggle_wrap(),
            Command::NextImageProtocol => {
                if matches!(viewer.doc, crate::viewer::Document::Image(_)) {
                    return Effect::CycleImageProtocol;
                }
            }
            Command::Help => self.mode = Mode::ViewerHelp,
            Command::NextViewer => {
                if let Some(kind) = viewer.next_kind() {
                    let path = viewer.path.clone();
                    return self.request_view(path, Some(kind));
                }
            }
            Command::NextEncoding => {
                if matches!(&viewer.doc, crate::viewer::Document::Lines(_)) {
                    let path = viewer.path.clone();
                    let kind = viewer.kind;
                    let encoding = crate::viewer::encoding::next_choice(viewer.encoding);
                    return self.request_view_with_encoding(path, Some(kind), encoding, true);
                }
            }
            // h / Backspace go back to the list, as they go up a level there ("l opens, h goes
            // back"). A vi-style option (h / l scroll sideways in the viewer) would remap h to
            // ScrollLeft in the keymap (command.rs); Backspace keeps closing.
            Command::Quit | Command::Back | Command::View | Command::ParentDirectory => {
                self.close_viewer()
            }
            Command::Suspend => return Effect::Suspend,
            _ => {}
        }
        Effect::None
    }

    fn close_viewer(&mut self) {
        self.viewer = None;
        self.pending_view = None;
        self.mode = self.list_mode();
    }

    /// F3 / v on the cursor entry (P2-1, P2-2).
    fn view_cursor_entry(&mut self) -> Effect {
        let Some(entry) = self.pane.cursor_entry().cloned() else {
            return Effect::None;
        };
        self.view_entry(&entry)
    }

    pub(crate) fn view_entry(&mut self, entry: &FileEntry) -> Effect {
        let name = entry.display_name.clone();
        match entry.kind {
            EntryKind::File | EntryKind::Symlink(LinkTarget::File | LinkTarget::Unknown) => {
                let path = entry.path.clone();
                self.request_view(path, None)
            }
            EntryKind::Symlink(LinkTarget::Broken) => {
                self.status = Some(Message::BrokenSymlink { name });
                Effect::None
            }
            _ => {
                self.status = Some(Message::NotAFile { name });
                Effect::None
            }
        }
    }

    fn request_view(&mut self, path: PathBuf, kind: Option<ViewerKind>) -> Effect {
        self.request_view_with_encoding(path, kind, None, false)
    }

    fn request_view_with_encoding(
        &mut self,
        path: PathBuf,
        kind: Option<ViewerKind>,
        encoding: Option<&'static encoding_rs::Encoding>,
        preserve_position: bool,
    ) -> Effect {
        self.next_id += 1;
        let id = self.next_id;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.pending_view = Some((id, name, preserve_position));
        self.mode = Mode::ViewerLoading;
        Effect::OpenViewer {
            id,
            path,
            kind,
            encoding,
        }
    }

    /// Applies a viewer open result. Dropped if `id` is not the open in flight (P2-13).
    pub fn on_viewer(&mut self, id: u64, result: ViewerResult) {
        let Some((pending_id, name, preserve_position)) = self.pending_view.take() else {
            return;
        };
        if pending_id != id {
            self.pending_view = Some((pending_id, name, preserve_position));
            return;
        }
        match result {
            Ok(opened) => {
                // A better viewer failed and another one is shown: say why (C28).
                if let Some((failed, reason)) = opened.fallback.clone() {
                    self.status = Some(Message::ViewerFallback { failed, reason });
                }
                let mut viewer = ViewerState::new(opened);
                if preserve_position
                    && let (Some(previous), crate::viewer::Document::Lines(doc)) =
                        (self.viewer.as_ref(), &viewer.doc)
                {
                    viewer.top_line = previous.top_line.min(doc.lines.len().saturating_sub(1));
                    viewer.wrap = previous.wrap;
                    viewer.hscroll = previous.hscroll;
                    viewer.top_row = 0;
                }
                self.viewer = Some(viewer);
                self.mode = Mode::Viewer;
            }
            Err(error) => {
                self.status = Some(match error {
                    ViewError::NotAFile => Message::NotAFile { name },
                    ViewError::Io(reason) => Message::CannotView { name, reason },
                });
                self.mode = if self.viewer.is_some() {
                    Mode::Viewer
                } else {
                    self.list_mode()
                };
            }
        }
    }

    /// Id of the viewer open in flight, if any. `run` cancels a running external provider whose
    /// open is no longer this one (q / Esc while loading, a newer open; C30).
    pub fn pending_view_id(&self) -> Option<u64> {
        self.pending_view.as_ref().map(|(id, _, _)| *id)
    }

    /// Name of the file being opened, while `Mode::ViewerLoading`.
    pub fn loading_view_name(&self) -> Option<&str> {
        self.pending_view.as_ref().map(|(_, n, _)| n.as_str())
    }

    /// The application set for this file's extension, if any (R-110-4).
    pub(crate) fn open_app_effect(&self, entry: &FileEntry) -> Option<Effect> {
        if !matches!(
            entry.kind,
            EntryKind::File | EntryKind::Symlink(LinkTarget::File | LinkTarget::Unknown)
        ) {
            return None;
        }
        let extension = entry
            .path
            .extension()?
            .to_string_lossy()
            .to_ascii_lowercase();
        self.open_extensions
            .contains(&extension)
            .then(|| Effect::OpenWithApp {
                path: entry.path.clone(),
            })
    }

    /// `o` (R-110-3): files, and on macOS application bundles, which are folders.
    pub(crate) fn open_external(&mut self, entry: Option<FileEntry>) -> Effect {
        let Some(entry) = entry else {
            return Effect::None;
        };
        let bundle = cfg!(target_os = "macos")
            && entry.kind.is_dir_like()
            && !entry.is_parent_link()
            && entry
                .path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("app"));
        let file = matches!(
            entry.kind,
            EntryKind::File | EntryKind::Symlink(LinkTarget::File | LinkTarget::Unknown)
        );
        if file || bundle {
            return Effect::OpenExternal { path: entry.path };
        }
        let name = entry.display_name;
        self.status = Some(if entry.kind == EntryKind::Symlink(LinkTarget::Broken) {
            Message::BrokenSymlink { name }
        } else {
            Message::NotAFile { name }
        });
        Effect::None
    }

    /// The file `o` was pressed on is a program: ask before running it.
    pub fn ask_run(&mut self, path: PathBuf) {
        self.run_target = Some(path);
        self.mode = Mode::ConfirmRun;
    }

    /// An outside application ended or failed to start.
    pub fn on_open_finished(&mut self, result: Result<(), String>) {
        if let Err(reason) = result {
            self.status = Some(Message::CannotOpenExternal { reason });
        }
    }

    /// Enter/l (R-101-5, R-110-1).
    fn enter(&mut self) -> Effect {
        let Some(entry) = self.pane.cursor_entry() else {
            return Effect::None;
        };
        match entry.kind {
            EntryKind::ParentLink => self.parent(),
            kind if kind.is_dir_like() => {
                let dir = entry.path.clone();
                self.request_load(dir, CursorFocus::First)
            }
            EntryKind::Symlink(LinkTarget::Broken) => {
                self.status = Some(Message::BrokenSymlink {
                    name: entry.display_name.clone(),
                });
                Effect::None
            }
            // R-110-2: from Phase 2, Enter on a file behaves like View.
            _ => {
                if let Some(effect) = self.open_app_effect(entry) {
                    effect
                } else if let Some(path) = self.archive_cursor_path() {
                    self.open_archive(path)
                } else {
                    self.view_cursor_entry()
                }
            }
        }
    }

    /// Backspace/h (R-102-4, R-300-2).
    fn parent(&mut self) -> Effect {
        let Some(parent) = self.pane.current_dir.parent().map(PathBuf::from) else {
            self.status = Some(Message::AtRoot);
            return Effect::None;
        };
        let focus = match self.pane.current_dir_name() {
            Some(name) => CursorFocus::Name(name),
            None => CursorFocus::First,
        };
        self.request_load(parent, focus)
    }

    /// Applies a listing result. Dropped if `id` is not the load in flight.
    ///
    /// On failure the current listing stays and the error goes to status (R-301 / design prompt §18).
    pub fn on_listing(&mut self, id: u64, result: ListingResult) {
        let Some((pending_id, dir, focus)) = self.pending.take() else {
            return;
        };
        if pending_id != id {
            self.pending = Some((pending_id, dir, focus));
            return;
        }
        match result {
            Ok(listing) => {
                self.pane.set_listing(dir, listing, focus);
                // Sizes and times may have changed: ask for the details again.
                self.info_key = None;
                // So may the contents: preview the cursor item again.
                self.preview_key = None;
                // A new folder while choosing a destination: start on the "here" row again.
                self.dest_on_here = self.mode == Mode::SelectDestination;
            }
            Err(reason) => {
                self.status = Some(Message::CannotOpen {
                    path: dir.to_string_lossy().into_owned(),
                    reason,
                })
            }
        }
    }

    /// Sets a status message (e.g. after resuming from suspend).
    pub fn set_status(&mut self, message: Message) {
        self.status = Some(message);
    }
}
