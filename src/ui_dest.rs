//! Rendering for choosing a Copy / Move destination and for the drives/places popup
//! (M23 UX follow-up). Everything drawn goes through `sanitize_for_display` / `truncate_to_width`.
//!
//! Layout while choosing a destination (top to bottom):
//!   banner (3 rows, yellow):  what is being copied/moved, the current destination and how it
//!                             relates to the source (same folder / above / inside / other drive)
//!   "here" row:               "[Copy here] <- Enter"; the cursor starts here
//!   the folder list           (files are dimmed: they cannot be a destination)

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::i18n::Catalog;
use crate::ops::Op;
use crate::ops::place::{PlaceKind, Relation, relation};
use crate::text::{sanitize_for_display, truncate_to_width};

/// Banner rows. With very little room only the first row is drawn.
const BANNER_ROWS: u16 = 3;

fn fit(s: &str, width: u16) -> String {
    truncate_to_width(&sanitize_for_display(s), usize::from(width))
}

fn banner_style() -> Style {
    Style::new()
        .bg(Color::Yellow)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

fn op_key(op: Op) -> &'static str {
    match op {
        Op::Copy => "ops.name_copy",
        Op::Move => "ops.name_move",
        Op::Delete => "ops.name_delete",
        Op::Trash => "ops.name_trash",
    }
}

/// "a.txt" / "a.txt and 2 more (3 items)".
fn what(app: &App, catalog: &Catalog) -> String {
    let Some(pending) = &app.pending_op else {
        return String::new();
    };
    let first = pending
        .sources
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let total = pending.sources.len();
    if total <= 1 {
        return catalog.text("ops.what_one", &[("name", &first)]);
    }
    let (n, total) = ((total - 1).to_string(), total.to_string());
    catalog.text(
        "ops.what_many",
        &[("name", &first), ("n", &n), ("total", &total)],
    )
}

/// The relation text shown after the destination path, e.g. "(same folder as the source)".
fn relation_text(app: &App, catalog: &Catalog) -> String {
    let Some(pending) = &app.pending_op else {
        return String::new();
    };
    let key = match relation(&pending.origin, &app.pane.current_dir) {
        Relation::Same => "ops.rel_same",
        Relation::Above => "ops.rel_above",
        Relation::Below => "ops.rel_below",
        Relation::OtherDrive => "ops.rel_other_drive",
        Relation::Elsewhere => return String::new(),
    };
    format!("({})", catalog.text(key, &[]))
}

/// The three banner lines.
pub fn banner_lines(app: &App, catalog: &Catalog) -> Vec<String> {
    let Some(pending) = &app.pending_op else {
        return Vec::new();
    };
    let op = catalog.text(op_key(pending.op), &[]);
    let path = app.pane.current_dir.display().to_string();
    let dest = catalog.text("ops.dest_path", &[("path", &path)]);
    let relation = relation_text(app, catalog);
    vec![
        format!(
            " {}",
            catalog.text(
                "ops.dest_banner",
                &[("op", &op), ("what", &what(app, catalog))]
            )
        ),
        format!(" {dest}  {relation}"),
        String::new(),
    ]
}

/// Key guide for the bottom row while choosing a destination. It depends on where the cursor is,
/// so the user always sees what Enter will do.
pub fn guide_text(app: &App, catalog: &Catalog) -> String {
    let op = app
        .pending_op
        .as_ref()
        .map(|p| catalog.text(op_key(p.op), &[]))
        .unwrap_or_default();
    let key = if app.dest_on_here {
        "ops.dest_guide_here"
    } else {
        "ops.dest_guide_folder"
    };
    catalog.text(key, &[("op", &op)])
}

/// Draws banner + "here" row + list into `area` (replaces `render_list` in SelectDestination).
pub fn render_destination(frame: &mut Frame, app: &mut App, catalog: &Catalog, area: Rect) {
    let banner_height = if area.height >= 8 { BANNER_ROWS } else { 1 };
    let banner = banner_lines(app, catalog);
    let banner_area = Rect {
        height: banner_height.min(area.height),
        ..area
    };
    let lines: Vec<Line> = banner
        .iter()
        .take(usize::from(banner_area.height))
        .map(|l| Line::from(fit(l, banner_area.width)))
        .collect();
    frame.render_widget(Paragraph::new(lines).style(banner_style()), banner_area);

    let rest = Rect {
        y: area.y.saturating_add(banner_area.height),
        height: area.height.saturating_sub(banner_area.height),
        ..area
    };
    if rest.height == 0 {
        return;
    }
    // "here" row
    let here_key = match app.pending_op.as_ref().map(|p| p.op) {
        Some(Op::Move) => "ops.here_move",
        _ => "ops.here_copy",
    };
    let here_style = if app.dest_on_here {
        Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::new().add_modifier(Modifier::BOLD)
    };
    let here = Line::styled(
        fit(&format!(" {}", catalog.text(here_key, &[])), rest.width),
        here_style,
    );
    frame.render_widget(Paragraph::new(here), Rect { height: 1, ..rest });

    let list_area = Rect {
        y: rest.y.saturating_add(1),
        height: rest.height.saturating_sub(1),
        ..rest
    };
    if list_area.height == 0 {
        return;
    }
    let height = usize::from(list_area.height);
    app.pane.ensure_visible(height);
    let pane = &app.pane;
    let lines: Vec<Line> = pane
        .entries
        .iter()
        .enumerate()
        .skip(pane.scroll)
        .take(height)
        .map(|(i, entry)| {
            let mut style = crate::ui::entry_style(entry);
            if !entry.kind.is_dir_like() {
                // Files cannot be destinations: dim them.
                style = style.add_modifier(Modifier::DIM);
            }
            if i == pane.cursor && !app.dest_on_here {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let label = format!(" {}", crate::ui::entry_label(entry));
            Line::styled(fit(&label, list_area.width), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
}

/// The drives / places popup, centered over the list.
pub fn render_places(frame: &mut Frame, app: &App, catalog: &Catalog, area: Rect) {
    if area.width < 20 || area.height < 5 {
        return;
    }
    let rows = app
        .places
        .len()
        .min(usize::from(area.height.saturating_sub(4)));
    let height = u16::try_from(rows)
        .unwrap_or(1)
        .saturating_add(4)
        .min(area.height);
    let width = area.width.min(64);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let title = format!(" {} ", catalog.text("places.title", &[]));
    let block = Block::default()
        .borders(Borders::ALL)
        .title(fit(&title, width.saturating_sub(2)));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let visible = usize::from(inner.height.saturating_sub(1));
    let first = app.places_cursor.saturating_sub(visible.saturating_sub(1));
    let mut lines: Vec<Line> = app
        .places
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(i, place)| {
            let label = match place.kind {
                PlaceKind::Home => catalog.text("places.home", &[]),
                PlaceKind::Origin => catalog.text("places.origin", &[]),
                PlaceKind::Root => catalog.text("places.root", &[]),
                PlaceKind::Drive => {
                    format!("{}  {}", place.label, catalog.text("places.drive", &[]))
                }
                PlaceKind::Mount => catalog.text("places.mount", &[]),
            };
            let text = format!(" {label:<28} {}", place.path.display());
            let style = if i == app.places_cursor {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            Line::styled(fit(&text, inner.width), style)
        })
        .collect();
    lines.push(Line::styled(
        fit(&catalog.text("places.guide", &[]), inner.width),
        Style::new().add_modifier(Modifier::DIM),
    ));
    frame.render_widget(Paragraph::new(lines), inner);
}
