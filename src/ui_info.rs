//! The info panel: details of the item under the cursor, drawn under the list so the user can
//! judge a command (delete, move, rename) before issuing it.
//!
//!   ┌ Info ─────────────────────────────────────────┐
//!   │ a.txt  File                                   │
//!   │ Permissions 755 rwxr-xr-x  Owner 1000:1000    │   (Windows: Attributes R--A)
//!   │ Size 2.0 KiB  Modified 2023-11-14 22:13       │
//!   │ 3 marked: 2 files, 1 folders, 120 B (...)     │   (only when something is marked)
//!   └───────────────────────────────────────────────┘
//!
//! Details are shown only when they belong to the item under the cursor: a stale answer for
//! another item must never be presented as this item's (it could mislead a delete).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::i18n::Catalog;
use crate::info::{
    Info, InfoKind, MarkSummary, format_mode, format_size, format_time, format_win_attrs,
};
use crate::text::{sanitize_for_display, truncate_to_width};

/// Rows the panel needs (border, 4 lines, border).
pub const PANEL_HEIGHT: u16 = 6;

/// The list must keep at least this many rows when the panel is shown.
const MIN_LIST_ROWS: u16 = 5;

fn fits(app: &App, area: Rect) -> bool {
    app.show_info && area.height >= PANEL_HEIGHT + MIN_LIST_ROWS && area.width >= 20
}

/// Area left for the list above the panel (the whole area when the panel is hidden or does
/// not fit).
pub fn list_area(app: &App, area: Rect) -> Rect {
    if fits(app, area) {
        Rect {
            height: area.height - PANEL_HEIGHT,
            ..area
        }
    } else {
        area
    }
}

/// Draws the panel in the bottom rows of `area`.
pub fn render(frame: &mut Frame, app: &App, catalog: &Catalog, area: Rect) {
    if !fits(app, area) {
        return;
    }
    let boxed = Rect {
        y: area.y + area.height - PANEL_HEIGHT,
        height: PANEL_HEIGHT,
        ..area
    };
    frame.render_widget(Clear, boxed);
    let title = format!(" {} ", catalog.text("info.title", &[]));
    let block = Block::default()
        .borders(Borders::ALL)
        .title(truncate_to_width(
            &title,
            usize::from(boxed.width.saturating_sub(2)),
        ));
    let inner = block.inner(boxed);
    frame.render_widget(block, boxed);

    let cursor_path = app
        .pane
        .cursor_entry()
        .filter(|e| !e.is_parent_link())
        .map(|e| &e.path);
    let mut lines = match (&app.info, cursor_path) {
        (Some(info), Some(path)) if &info.path == path => detail_lines(info, catalog),
        (_, Some(_)) => vec![catalog.text("info.loading", &[])],
        _ => Vec::new(),
    };
    if let Some(marks) = app.info_marks {
        lines.resize(3, String::new());
        lines.push(marks_line(marks, catalog));
    }
    let lines: Vec<Line> = lines
        .iter()
        .take(usize::from(inner.height))
        .map(|l| {
            Line::styled(
                truncate_to_width(
                    &format!(" {}", sanitize_for_display(l)),
                    usize::from(inner.width),
                ),
                Style::new().add_modifier(Modifier::empty()),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn join(parts: Vec<String>) -> String {
    parts.join("   ")
}

/// Lines 1-3: name and kind, permissions or attributes, size and dates.
pub fn detail_lines(info: &Info, catalog: &Catalog) -> Vec<String> {
    let name = info
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| info.path.display().to_string());
    let kind = match info.kind {
        InfoKind::File => catalog.text("info.kind_file", &[]),
        InfoKind::Dir => catalog.text("info.kind_dir", &[]),
        InfoKind::Symlink => catalog.text("info.kind_symlink", &[]),
        InfoKind::Other => catalog.text("info.kind_other", &[]),
    };
    let mut first = vec![name, kind];
    if let Some((n, capped)) = info.dir_items {
        let key = if capped {
            "info.items_capped"
        } else {
            "info.items"
        };
        first.push(catalog.text(key, &[("n", &n.to_string())]));
    }
    // "(broken)" goes before the target: a long target is cut at the end of the line.
    if info.link_broken {
        first.push(catalog.text("info.link_broken", &[]));
    }
    if let Some(target) = &info.link_target {
        first.push(catalog.text("info.link_to", &[("target", &target.display().to_string())]));
    }

    let mut second = Vec::new();
    if let Some(mode) = info.unix_mode {
        second.push(catalog.text("info.perms", &[("mode", &format_mode(mode))]));
    }
    if let Some((uid, gid)) = info.owner {
        second.push(catalog.text(
            "info.owner",
            &[("uid", &uid.to_string()), ("gid", &gid.to_string())],
        ));
    }
    if let Some(attrs) = info.win_attrs {
        second.push(catalog.text("info.attrs", &[("attrs", &format_win_attrs(attrs))]));
    }
    if info.readonly {
        second.push(catalog.text("info.readonly", &[]));
    }

    let tz = jiff::tz::TimeZone::system();
    let mut third = Vec::new();
    if let Some(size) = info.size {
        third.push(catalog.text("info.size", &[("size", &format_size(size))]));
    }
    if let Some(t) = info.modified {
        third.push(catalog.text("info.modified", &[("time", &format_time(t, &tz))]));
    }
    if let Some(t) = info.created {
        third.push(catalog.text("info.created", &[("time", &format_time(t, &tz))]));
    }
    vec![join(first), join(second), join(third)]
}

fn marks_line(marks: MarkSummary, catalog: &Catalog) -> String {
    let n = (marks.files + marks.dirs).to_string();
    catalog.text(
        "info.marks",
        &[
            ("n", &n),
            ("files", &marks.files.to_string()),
            ("dirs", &marks.dirs.to_string()),
            ("size", &format_size(marks.bytes)),
        ],
    )
}
