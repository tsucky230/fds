//! The input box for Rename / mkdir / search (UX follow-up). The text used to be drawn as plain
//! text on the status row with no cursor, so the user could not see where typing goes.
//!
//! Layout (bottom of the list area; the list is drawn above it and stays fully visible):
//!   ┌ Rename: old.txt ─────────────┐
//!   │ new name here▌               │   <- the cursor cell is drawn reversed
//!   │ Enter: OK  Esc: cancel  ...  │
//!   └──────────────────────────────┘

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;

use crate::app::{App, InputPurpose};
use crate::i18n::Catalog;
use crate::text::{display_width, sanitize_for_display, truncate_to_width};

/// Rows the box needs (border, text, hint, border).
pub const BOX_HEIGHT: u16 = 4;

/// The visible part of the input line: text before the cursor, the grapheme under the cursor
/// (a space at the end of the text) and the text after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub before: String,
    pub at: String,
    pub after: String,
    /// Display column of the cursor inside the window (0-based).
    pub col: usize,
}

/// Fits `text` with the cursor (a grapheme index) into `width` columns.
///
/// Behavior:
/// - Control characters are shown as '?'.
/// - When the cursor would fall outside the window, leading graphemes are dropped so the cursor
///   cell is inside the window.
/// - The text after the cursor is cut to the remaining width. `width == 0` gives an empty window.
pub fn window(text: &str, cursor: usize, width: usize) -> Window {
    let clean = sanitize_for_display(text);
    let graphemes: Vec<&str> = clean.graphemes(true).collect();
    let cursor = cursor.min(graphemes.len());
    if width == 0 {
        return Window {
            before: String::new(),
            at: String::new(),
            after: String::new(),
            col: 0,
        };
    }
    let at = graphemes.get(cursor).copied().unwrap_or(" ");
    let at_width = display_width(at).max(1);
    // Drop leading graphemes until "before + cursor cell" fits.
    let mut start = 0;
    let mut before_width: usize = graphemes[..cursor].iter().map(|g| display_width(g)).sum();
    while start < cursor && before_width + at_width > width {
        before_width -= display_width(graphemes[start]);
        start += 1;
    }
    let before: String = graphemes[start..cursor].concat();
    let rest: String = graphemes.get(cursor + 1..).unwrap_or(&[]).concat();
    let room = width.saturating_sub(before_width + at_width);
    Window {
        before,
        at: at.to_string(),
        after: truncate_to_width(&rest, room),
        col: before_width,
    }
}

/// Area left for the list above the box (the whole area when the box does not fit).
pub fn list_area(area: Rect) -> Rect {
    if fits(area) {
        Rect {
            height: area.height - BOX_HEIGHT,
            ..area
        }
    } else {
        area
    }
}

fn fits(area: Rect) -> bool {
    area.height > BOX_HEIGHT && area.width >= 10
}

/// Draws the box in the bottom rows of `area` and puts the terminal cursor on the edit position.
pub fn render(frame: &mut Frame, app: &App, catalog: &Catalog, area: Rect) {
    let Some(input) = app.input.as_ref() else {
        return;
    };
    if !fits(area) {
        return;
    }
    let list = list_area(area);
    let boxed = Rect {
        y: area.y + list.height,
        height: BOX_HEIGHT,
        ..area
    };
    let title = match &input.purpose {
        InputPurpose::Rename { old, .. } => catalog.text("input.title_rename", &[("old", old)]),
        InputPurpose::Mkdir => catalog.text("input.title_mkdir", &[]),
        InputPurpose::ArchiveCreate { .. } => catalog.text("input.title_archive_create", &[]),
        InputPurpose::ArchiveExtract { .. } => catalog.text("input.title_archive_extract", &[]),
        InputPurpose::Search { .. } => catalog.text("input.title_search", &[]),
        InputPurpose::Command { .. } => catalog.text(
            "input.title_command",
            &[("count", &app.command_target_count().to_string())],
        ),
    };
    let hint_key = match input.purpose {
        InputPurpose::Search { .. } => "input.hint_search",
        InputPurpose::Command { .. } => "input.hint_command",
        _ => "input.hint",
    };
    frame.render_widget(Clear, boxed);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(truncate_to_width(
            &format!(" {} ", sanitize_for_display(&title)),
            usize::from(boxed.width.saturating_sub(2)),
        ));
    let inner = block.inner(boxed);
    frame.render_widget(block, boxed);

    // One column of left padding keeps the text off the border.
    let width = usize::from(inner.width.saturating_sub(1));
    let w = window(input.editor.text(), input.editor.cursor(), width);
    let col = u16::try_from(w.col).unwrap_or(0);
    let line = Line::from(vec![
        Span::raw(" "),
        Span::raw(w.before),
        Span::styled(w.at, Style::new().add_modifier(Modifier::REVERSED)),
        Span::raw(w.after),
    ]);
    frame.render_widget(Paragraph::new(line), Rect { height: 1, ..inner });
    let hint = truncate_to_width(&catalog.text(hint_key, &[]), usize::from(inner.width));
    frame.render_widget(
        Paragraph::new(Line::styled(hint, Style::new().add_modifier(Modifier::DIM))),
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );
    // The real cursor too, so terminals/IMEs place the composition at the edit position.
    frame.set_cursor_position((inner.x.saturating_add(1).saturating_add(col), inner.y));
}
