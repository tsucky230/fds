//! Rendering (design prompt §6, §16). Reads App state only; never touches the filesystem.
//!
//! Every string drawn goes through `sanitize_for_display` and `truncate_to_width`.

use std::time::SystemTime;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::app::{App, InputPurpose, Mode};
use crate::command::KEY_HELP;
use crate::entry::{EntryKind, FileEntry, LinkTarget};
use crate::i18n::Catalog;
use crate::layout::{self, LayoutMode};
use crate::ops::DeleteMode;
use crate::supporter;
use crate::text::{display_width, sanitize_for_display, truncate_to_width};
use crate::ui_archive;
use crate::ui_dest;
use crate::ui_preview;
use crate::ui_viewer::{self, ImageSlot, Screen};

/// Draws one frame. Takes `&mut App` to adjust scroll to the list area's height.
/// `images` holds terminal-specific image state for the viewer.
pub fn render(frame: &mut Frame, app: &mut App, catalog: &Catalog, images: &mut ImageSlot) {
    // `render_browse` sets it again when this frame draws the pane.
    app.preview_shown = false;
    let area = framed_area(frame, app.show_frame);
    let mode = layout::decide(area.width, area.height);
    if mode == LayoutMode::TooSmall {
        if app.mode == Mode::PreviewBrowse {
            app.leave_peek();
        }
        let line = fit(&catalog.text("screen.too_small", &[]), area.width);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    let regions = layout::regions(mode);
    let area = if regions.show_title {
        let band = Paragraph::new(title_text(area.width, &clock_text()))
            .style(Style::new().add_modifier(Modifier::REVERSED));
        frame.render_widget(band, Rect { height: 1, ..area });
        Rect {
            y: area.y + 1,
            height: area.height - 1,
            ..area
        }
    } else {
        area
    };
    let rows = Layout::vertical([
        Constraint::Length(u16::from(regions.show_path)),
        Constraint::Min(1),
        Constraint::Length(u16::from(regions.show_status)),
        Constraint::Length(u16::from(regions.show_key_guide)),
    ])
    .split(area);
    app.peek_fits = rows[1].height >= ui_preview::PEEK_MIN_HEIGHT;
    if app.mode == Mode::PreviewBrowse && !app.peek_fits {
        app.leave_peek();
    }

    if matches!(app.mode, Mode::Viewer | Mode::ViewerHelp) {
        let status = app.status.as_ref().map(|m| m.render(catalog));
        let help = app.mode == Mode::ViewerHelp;
        if let Some(viewer) = app.viewer.as_mut() {
            let screen = Screen {
                rows: &rows,
                regions,
                help,
                status,
                vi_keys: app.vi_keys,
            };
            ui_viewer::render(frame, viewer, catalog, images, screen);
            return;
        }
    }

    if regions.show_path {
        let path = if let Some(view) = app.archive.as_ref().filter(|_| archive_visible(app)) {
            format!(
                "{}!/{prefix}",
                view.path.display(),
                prefix = view.prefix.display()
            )
        } else {
            app.pane.current_dir.to_string_lossy().into_owned()
        };
        let line = Line::styled(
            fit(&format!(" {path}"), area.width),
            Style::new().add_modifier(Modifier::BOLD),
        );
        frame.render_widget(Paragraph::new(line), rows[0]);
    }
    match app.mode {
        Mode::ArchiveBrowse => ui_archive::render(frame, app, rows[1]),
        Mode::ArchiveLoading => {
            render_lines(frame, rows[1], vec![catalog.text("archive.loading", &[])])
        }
        Mode::ArchiveRunning => {
            render_lines(frame, rows[1], vec![catalog.text("archive.running", &[])])
        }
        Mode::Help => render_help(frame, rows[1], app, catalog),
        Mode::ConfirmDelete => render_lines(frame, rows[1], confirm_delete_lines(app, catalog)),
        Mode::Conflict => render_lines(frame, rows[1], conflict_lines(app, catalog)),
        Mode::ErrorList => render_lines(frame, rows[1], error_lines(app, catalog)),
        Mode::SelectDestination => ui_dest::render_destination(frame, app, catalog, rows[1]),
        Mode::Places => {
            // The popup floats over the screen it was opened from.
            if app.places_return == Mode::SelectDestination {
                ui_dest::render_destination(frame, app, catalog, rows[1]);
            } else {
                render_browse(frame, app, catalog, images, rows[1]);
            }
            ui_dest::render_places(frame, app, catalog, rows[1]);
        }
        Mode::Input => {
            // The input box takes the bottom rows; the list stays visible above it.
            if archive_visible(app) {
                ui_archive::render(frame, app, crate::ui_input::list_area(rows[1]));
            } else {
                render_browse(
                    frame,
                    app,
                    catalog,
                    images,
                    crate::ui_input::list_area(rows[1]),
                );
            }
            crate::ui_input::render(frame, app, catalog, rows[1]);
        }
        Mode::PreviewBrowse => render_browse(frame, app, catalog, images, rows[1]),
        Mode::ConfirmRun if app.peek.is_some() => {
            render_browse(frame, app, catalog, images, rows[1]);
        }
        Mode::Normal | Mode::ConfirmQuit | Mode::ConfirmRun => {
            // The info panel sits under the list and the preview (the list keeps at least a
            // few rows).
            render_browse(
                frame,
                app,
                catalog,
                images,
                crate::ui_info::list_area(app, rows[1]),
            );
            crate::ui_info::render(frame, app, catalog, rows[1]);
        }
        _ => render_list(frame, app, rows[1]),
    }
    // Asked on the bottom row (the key guide's, or the status bar's when there is no guide).
    let question = question_text(app, catalog, area.width);
    if regions.show_status {
        // Style the whole row, not just the text, so the bar spans the full width.
        let text = match &question {
            Some(question) if !regions.show_key_guide => question.clone(),
            _ => status_text(app, catalog),
        };
        let status = Paragraph::new(fit(&text, area.width))
            .style(Style::new().add_modifier(Modifier::REVERSED));
        frame.render_widget(status, rows[2]);
    }
    if regions.show_key_guide {
        let guide = match app.mode {
            // What Enter will do depends on the cursor, so say it explicitly (M23).
            Mode::SelectDestination => ui_dest::guide_text(app, catalog),
            Mode::Normal => {
                let key = if regions.short_key_guide {
                    "guide.short"
                } else {
                    "guide.full"
                };
                catalog.text(key, &[])
            }
            Mode::PreviewBrowse => catalog.text("guide.peek", &[]),
            Mode::ConfirmQuit | Mode::ConfirmRun => question.clone().unwrap_or_default(),
            Mode::ArchiveBrowse => catalog.text("archive.guide", &[]),
            Mode::ArchiveLoading => catalog.text("archive.loading_guide", &[]),
            Mode::ArchiveRunning => catalog.text("archive.running_guide", &[]),
            // Dialogs, popups and running operations show their own keys; the list's keys
            // do nothing there, so offering them would mislead.
            _ => String::new(),
        };
        let style = if question.is_some() {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        frame.render_widget(
            Paragraph::new(fit(&guide, area.width)).style(style),
            rows[3],
        );
    }
}

const COPYRIGHT: &str = "© 2026 Qraft Node Kyoto";

/// The title band (R-200-12): name, version and copyright on the left, the clock on the right.
/// The copyright goes first when the width runs out.
pub fn title_text(width: u16, clock: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let width = usize::from(width);
    let right = format!("{clock} ");
    let short = format!(" fds {version}");
    let full = format!("{short} {COPYRIGHT}");
    for left in [&full, &short] {
        let used = display_width(left) + display_width(&right);
        if used < width {
            return format!("{left}{:pad$}{right}", "", pad = width - used);
        }
    }
    truncate_to_width(&short, width)
}

fn clock_text() -> String {
    crate::info::format_clock(SystemTime::now(), &jiff::tz::TimeZone::system())
}

/// Smallest terminal that still leaves a full-size layout inside the frame (40x10 inside).
const FRAME_MIN_WIDTH: u16 = 42;
const FRAME_MIN_HEIGHT: u16 = 12;

/// Draws the frame around the whole screen (so the extent of fds is visible on a black
/// terminal) and returns the area inside it. Without the frame, or when the terminal is too
/// small to spare the border, returns the whole screen.
fn framed_area(frame: &mut Frame, show_frame: bool) -> Rect {
    let area = frame.area();
    if !show_frame || area.width < FRAME_MIN_WIDTH || area.height < FRAME_MIN_HEIGHT {
        return area;
    }
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// The list, with the preview pane right of it when the preview is on and fits (R-200-9).
fn render_browse(
    frame: &mut Frame,
    app: &mut App,
    catalog: &Catalog,
    images: &mut ImageSlot,
    area: Rect,
) {
    let (list, pane) = ui_preview::split(app, area);
    app.preview_shown = pane.is_some();
    if pane.is_none() && app.mode == Mode::PreviewBrowse {
        app.leave_peek();
    }
    render_list(frame, app, list);
    let Some(pane) = pane else {
        return;
    };
    if app.peek.is_some() {
        let (top, bottom) = ui_preview::split_peek(pane);
        ui_preview::render_peek(frame, app, catalog, top);
        ui_preview::render(frame, app, catalog, images, bottom);
    } else {
        ui_preview::render(frame, app, catalog, images, pane);
    }
}

fn render_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let height = usize::from(area.height);
    app.pane.ensure_visible(height);
    // Overflowing lists get a scrollbar in the last column, so the position is visible.
    let overflow = app.pane.entries.len() > height && area.width > 2;
    let scrollbar_area = Rect {
        x: area.x + area.width.saturating_sub(1),
        width: 1,
        ..area
    };
    let area = if overflow {
        Rect {
            width: area.width - 1,
            ..area
        }
    } else {
        area
    };
    let pane = &app.pane;
    let lines: Vec<Line> = pane
        .entries
        .iter()
        .enumerate()
        .skip(pane.scroll)
        .take(height)
        .map(|(i, entry)| {
            let mut style = entry_style(entry);
            if entry.marked {
                style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }
            if i == pane.cursor {
                // The cursor is in the pane's list: this one only says where that list came from.
                style = style.add_modifier(if app.peek.is_some() {
                    Modifier::UNDERLINED
                } else {
                    Modifier::REVERSED
                });
            }
            Line::styled(fit(&entry_text(entry), area.width), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
    if overflow {
        let mut state = ScrollbarState::new(pane.entries.len().saturating_sub(height))
            .viewport_content_length(height)
            .position(pane.scroll);
        frame.render_stateful_widget(scrollbar(), scrollbar_area, &mut state);
    }
}

/// The y/n question on the bottom row. The name gives way, so `(y/N)` always shows.
fn question_text(app: &App, catalog: &Catalog, width: u16) -> Option<String> {
    match app.mode {
        Mode::ConfirmQuit => Some(catalog.text("status.confirm_quit", &[])),
        Mode::ConfirmRun => {
            let name = app.run_target.as_ref()?.file_name()?.to_string_lossy();
            let name = sanitize_for_display(&name);
            let empty = catalog.text("status.confirm_run", &[("name", "")]);
            let room = usize::from(width)
                .saturating_sub(unicode_width::UnicodeWidthStr::width(empty.as_str()));
            let name = truncate_to_width(&name, room);
            Some(catalog.text("status.confirm_run", &[("name", &name)]))
        }
        _ => None,
    }
}

/// The scrollbar of every list (R-200-7): a thin thumb and no track, next to the borders.
pub(crate) fn scrollbar() -> Scrollbar<'static> {
    Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(None)
        .thumb_symbol("┃")
}

/// One row of text: mark column, then the label. Also used by the preview pane.
pub(crate) fn entry_text(entry: &FileEntry) -> String {
    let mark = if entry.marked { '*' } else { ' ' };
    format!("{mark}{}", entry_label(entry))
}

/// The kind and the name (R-200-11). `[` takes the column before the name, so directory and file
/// names line up. Links and other kinds keep their tag.
pub(crate) fn entry_label(entry: &FileEntry) -> String {
    let name = &entry.display_name;
    let tag = match entry.kind {
        EntryKind::ParentLink => return "[..]".to_string(),
        EntryKind::Directory => return format!("[{name}]/"),
        EntryKind::File => return format!(" {name}"),
        EntryKind::Symlink(LinkTarget::Directory) => "[L>D]",
        EntryKind::Symlink(LinkTarget::File) => "[L>F]",
        EntryKind::Symlink(LinkTarget::Broken) => "[L>!]",
        EntryKind::Symlink(LinkTarget::Unknown) => "[L>?]",
        EntryKind::Other => "[OTH]",
    };
    format!("{tag} {name}")
}

/// Bold and colored, so that a file named `[a]` does not read as the directory `a` (R-200-11).
pub(crate) fn dir_style() -> Style {
    Style::new().add_modifier(Modifier::BOLD).fg(Color::Cyan)
}

/// The base style of a row, before marks and the cursor.
pub(crate) fn entry_style(entry: &FileEntry) -> Style {
    if entry.kind.is_dir_like() {
        dir_style()
    } else {
        Style::new()
    }
}

/// Draws plain text lines into `area` (dialog bodies).
fn render_lines(frame: &mut Frame, area: Rect, lines: Vec<String>) {
    let lines: Vec<Line> = lines
        .iter()
        .take(usize::from(area.height))
        .map(|l| Line::from(fit(l, area.width)))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Up to this many targets are listed in the delete confirmation; the rest is summarized.
const CONFIRM_LIST_LIMIT: usize = 10;

fn confirm_delete_lines(app: &App, catalog: &Catalog) -> Vec<String> {
    let count = app.delete_targets.len().to_string();
    let key = if app.delete_mode == DeleteMode::Trash {
        "ops.confirm_trash"
    } else {
        "ops.confirm_delete"
    };
    let mut lines = vec![format!(" {}", catalog.text(key, &[("count", &count)]))];
    for path in app.delete_targets.iter().take(CONFIRM_LIST_LIMIT) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        lines.push(format!(
            "   {}",
            name.unwrap_or_else(|| path.display().to_string())
        ));
    }
    let more = app.delete_targets.len().saturating_sub(CONFIRM_LIST_LIMIT);
    if more > 0 {
        lines.push(format!(
            "   {}",
            catalog.text("ops.and_more", &[("n", &more.to_string())])
        ));
    }
    lines
}

fn conflict_lines(app: &App, catalog: &Catalog) -> Vec<String> {
    let Some(c) = &app.conflict else {
        return Vec::new();
    };
    let name = c
        .dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| c.dest.display().to_string());
    vec![
        format!(" {}", catalog.text("ops.conflict", &[("name", &name)])),
        format!("   {}", c.dest.display()),
    ]
}

fn error_lines(app: &App, catalog: &Catalog) -> Vec<String> {
    let count = app.failures.len().to_string();
    let mut lines = vec![format!(
        " {}",
        catalog.text("ops.error_list", &[("count", &count)])
    )];
    for (path, reason) in &app.failures {
        lines.push(format!("   {}: {reason}", path.display()));
    }
    lines
}

/// The status row while an operation is in progress or asking something.
fn operation_status(app: &App, catalog: &Catalog) -> Option<String> {
    let selecting = app.mode == Mode::SelectDestination
        || (app.mode == Mode::Places && app.places_return == Mode::SelectDestination);
    match app.mode {
        _ if selecting => {
            let pending = app.pending_op.as_ref()?;
            let key = match pending.op {
                crate::ops::Op::Move => "ops.select_move",
                _ => "ops.select_copy",
            };
            let count = pending.sources.len().to_string();
            Some(catalog.text(key, &[("count", &count)]))
        }
        Mode::Running | Mode::Conflict => {
            let job = app.job.as_ref()?;
            let op = match job.op {
                crate::ops::Op::Copy => "ops.name_copy",
                crate::ops::Op::Move => "ops.name_move",
                crate::ops::Op::Delete => "ops.name_delete",
                crate::ops::Op::Trash => "ops.name_trash",
            };
            let op = catalog.text(op, &[]);
            let (done, total) = (job.done_files.to_string(), job.total_files.to_string());
            let bytes = if job.total_bytes > 0 {
                format!(
                    "  {} / {}",
                    crate::info::format_size(job.done_bytes),
                    crate::info::format_size(job.total_bytes)
                )
            } else {
                String::new()
            };
            let mut text = catalog.text(
                "ops.running",
                &[
                    ("op", &op),
                    ("done", &done),
                    ("total", &total),
                    ("bytes", &bytes),
                ],
            );
            if let Some(name) = job.current.as_ref().and_then(|c| c.file_name()) {
                text.push_str("  ");
                text.push_str(&name.to_string_lossy());
            }
            Some(text)
        }
        _ => None,
    }
}

fn archive_visible(app: &App) -> bool {
    app.mode == Mode::ArchiveBrowse
        || (app.mode == Mode::Input
            && matches!(
                app.input.as_ref().map(|input| &input.purpose),
                Some(InputPurpose::ArchiveExtract {
                    return_mode: Mode::ArchiveBrowse,
                    ..
                })
            ))
}

fn status_text(app: &App, catalog: &Catalog) -> String {
    if let Some(name) = app.loading_view_name() {
        return format!(" {}", catalog.text("status.opening", &[("name", name)]));
    }
    if let Some(text) = operation_status(app, catalog) {
        return format!(" {text}");
    }
    if let Some(status) = &app.status {
        return format!(" {}", status.render(catalog));
    }
    if archive_visible(app)
        && let Some(view) = &app.archive
    {
        let children = view.children();
        let files = children.iter().filter(|m| !m.is_dir).count().to_string();
        let dirs = children.iter().filter(|m| m.is_dir).count().to_string();
        return format!(
            " {}",
            catalog.text("status.counts", &[("files", &files), ("dirs", &dirs)])
        );
    }
    if app.mode == Mode::ArchiveLoading {
        return format!(" {}", catalog.text("archive.loading", &[]));
    }
    if app.mode == Mode::ArchiveRunning {
        return format!(" {}", catalog.text("archive.running", &[]));
    }
    let entries = &app.pane.entries;
    let dirs = entries
        .iter()
        .filter(|e| !e.is_parent_link() && e.kind.is_dir_like())
        .count();
    let files = entries.iter().filter(|e| !e.kind.is_dir_like()).count();
    let marked = entries.iter().filter(|e| e.marked).count();
    let (files, dirs) = (files.to_string(), dirs.to_string());
    let mut text = format!(
        " {}",
        catalog.text("status.counts", &[("files", &files), ("dirs", &dirs)])
    );
    if marked > 0 {
        // Tell what can be done with the selection (the commands were not discoverable, M23).
        let marked = marked.to_string();
        text = format!(
            " {}",
            catalog.text("status.marked_actions", &[("n", &marked)])
        );
    }
    if app.is_loading() {
        text.push_str(&format!("   {}", catalog.text("status.loading", &[])));
    }
    text
}

/// Key list. Split into columns when it does not fit the height (AC 30 on small screens).
fn render_help(frame: &mut Frame, area: Rect, app: &App, catalog: &Catalog) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    // The version first, so it stays visible when the title is cut on a narrow screen.
    let title = format!(
        " FDS {}  {}",
        env!("CARGO_PKG_VERSION"),
        catalog.text("help.title", &[])
    );
    frame.render_widget(
        Paragraph::new(fit(&title, area.width)),
        Rect { height: 1, ..area },
    );

    let body = Rect {
        y: area.y + 1,
        height: area.height - 1,
        ..area
    };
    let rows_per_column = usize::from(body.height).max(1);
    let columns = KEY_HELP.len().div_ceil(rows_per_column).max(1);
    let column_width = body.width / u16::try_from(columns).unwrap_or(u16::MAX).max(1);
    let first_key = |keys: &'static str| keys.split(" / ").next().unwrap_or(keys);
    let mut x = body.x;
    for chunk in KEY_HELP.chunks(rows_per_column) {
        // Too narrow for "keys + action": each column takes only the room its first keys need.
        let width = if column_width < 24 {
            let longest = chunk.iter().map(|(keys, _)| first_key(keys).len());
            u16::try_from(longest.max().unwrap_or(0) + 2).unwrap_or(column_width)
        } else {
            column_width
        };
        let width = width.min(body.right().saturating_sub(x));
        let lines: Vec<Line> = chunk
            .iter()
            .map(|(keys, action)| {
                if column_width < 24 {
                    return Line::from(fit(&format!(" {}", first_key(keys)), width));
                }
                let action = catalog.text(action, &[]);
                Line::from(fit(&format!(" {keys:<13} {action}"), width))
            })
            .collect();
        let rect = Rect { x, width, ..body };
        frame.render_widget(Paragraph::new(lines), rect);
        x += width;
    }
    let mut supporter_lines = supporter::help_lines(&app.supporter, catalog);
    supporter_lines.push(catalog.text("legal.notice", &[]));
    supporter_lines.push(catalog.text("legal.license", &[]));
    let offset = KEY_HELP.len() + 1;
    if usize::from(body.height) >= offset + supporter_lines.len() {
        let blank = Rect {
            y: body
                .y
                .saturating_add(u16::try_from(KEY_HELP.len()).unwrap_or(u16::MAX)),
            height: 1,
            ..body
        };
        frame.render_widget(Paragraph::new(""), blank);
        let section = Rect {
            y: blank.y.saturating_add(1),
            height: u16::try_from(supporter_lines.len()).unwrap_or(u16::MAX),
            ..body
        };
        let lines: Vec<Line> = supporter_lines
            .iter()
            .map(|line| Line::from(fit(&format!(" {line}"), body.width)))
            .collect();
        frame.render_widget(Paragraph::new(lines), section);
    }
}

/// Replaces control characters and fits the text into `width` columns.
fn fit(s: &str, width: u16) -> String {
    truncate_to_width(&sanitize_for_display(s), usize::from(width))
}
