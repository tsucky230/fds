//! Plain-text layout: line splitting, TAB expansion, wrapping and horizontal slicing (P2-6).
//! Pure logic. Widths are terminal display columns (see `crate::text`).

use super::styled::{Span, SpanStyle, StyledLine};
use crate::text::{display_width, sanitize_for_display};
use unicode_segmentation::UnicodeSegmentation;

/// TAB stops every this many columns.
pub const TAB_WIDTH: usize = 8;

/// Splits decoded text into logical lines.
///
/// Behavior:
/// - "\r\n", "\n" and a lone "\r" each end a line.
/// - A trailing line break does not create an extra empty line ("a\n" -> ["a"]).
/// - Empty input -> no lines. "\n" -> [""]. "a\n\nb" -> ["a", "", "b"].
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '\n' {
            lines.push(&text[start..i]);
            start = i + 1;
        } else if c == '\r' {
            lines.push(&text[start..i]);
            let mut end = i + 1;
            if chars.peek().map(|(_, c)| *c) == Some('\n') {
                chars.next();
                end += 1;
            }
            start = end;
        }
    }

    if start < text.len() {
        lines.push(&text[start..]);
    }

    lines
}

/// Expands TABs to the next multiple of `TAB_WIDTH` display columns and replaces other control
/// characters with `crate::text::sanitize_for_display` rules ('?').
///
/// Column counting uses display width (full-width chars count 2), so
/// "日\tx" -> "日" + 6 spaces + "x" (the TAB moves from column 2 to column 8).
pub fn expand_line(line: &str) -> String {
    let mut out = String::new();
    let mut parts = line.split('\t');

    if let Some(first) = parts.next() {
        let first = sanitize_for_display(first);
        let mut col = display_width(&first);
        out.push_str(&first);
        for segment in parts {
            let spaces = TAB_WIDTH.saturating_sub(col % TAB_WIDTH);
            out.push_str(&" ".repeat(spaces));
            let segment = sanitize_for_display(segment);
            col = col
                .saturating_add(spaces)
                .saturating_add(display_width(&segment));
            out.push_str(&segment);
        }
    }

    out
}

/// Converts decoded text into plain styled lines (`split_lines` + `expand_line`).
pub fn to_lines(text: &str) -> Vec<StyledLine> {
    split_lines(text)
        .into_iter()
        .map(|l| StyledLine::plain(expand_line(l)))
        .collect()
}

/// Wraps one styled line into rows of at most `width` display columns.
///
/// Behavior:
/// - Splits at grapheme boundaries (never inside a grapheme); styles are kept per span,
///   so a span may be split across rows.
/// - A grapheme wider than `width` (e.g. a full-width char when width == 1) gets its own row.
/// - An empty line yields exactly one empty row. `width == 0` yields one row with the whole line.
/// - Empty spans are not emitted.
pub fn wrap_line(line: &StyledLine, width: usize) -> Vec<StyledLine> {
    if width == 0 {
        return vec![line.clone()];
    }

    let mut rows: Vec<StyledLine> = Vec::new();
    let mut current_spans: Vec<Span> = Vec::new();
    let mut current_text = String::new();
    let mut current_style = SpanStyle::Plain;
    let mut row_width: usize = 0;

    for span in &line.spans {
        if span.text.is_empty() {
            continue;
        }

        if !current_text.is_empty() && current_style != span.style {
            current_spans.push(Span {
                text: std::mem::take(&mut current_text),
                style: current_style,
            });
        }
        current_style = span.style;

        for g in span.text.graphemes(true) {
            let w = display_width(g);

            if w == 0 {
                current_text.push_str(g);
                continue;
            }

            if row_width == 0 && w > width {
                if !current_text.is_empty() {
                    current_spans.push(Span {
                        text: std::mem::take(&mut current_text),
                        style: current_style,
                    });
                }
                current_text.push_str(g);
                current_spans.push(Span {
                    text: std::mem::take(&mut current_text),
                    style: current_style,
                });
                rows.push(StyledLine {
                    spans: std::mem::take(&mut current_spans),
                });
                row_width = 0;
                continue;
            }

            if row_width.saturating_add(w) > width {
                if !current_text.is_empty() {
                    current_spans.push(Span {
                        text: std::mem::take(&mut current_text),
                        style: current_style,
                    });
                }
                rows.push(StyledLine {
                    spans: std::mem::take(&mut current_spans),
                });
                row_width = 0;
            }

            current_text.push_str(g);
            row_width += w;
        }
    }

    if !current_text.is_empty() {
        current_spans.push(Span {
            text: current_text,
            style: current_style,
        });
    }

    if !current_spans.is_empty() {
        rows.push(StyledLine {
            spans: current_spans,
        });
    } else if rows.is_empty() {
        // An empty line still takes one row.
        rows.push(StyledLine::default());
    }

    rows
}

/// Returns the part of `line` that is visible from display column `start` for `width` columns
/// (used when wrapping is off, P2-6).
///
/// Behavior:
/// - Graphemes that start before `start` are dropped; a full-width grapheme that would be cut at
///   the left edge is replaced by a single space so columns stay aligned.
/// - Stops before a grapheme that would exceed `start + width`.
/// - Keeps span styles; empty spans are not emitted.
pub fn slice_columns(line: &StyledLine, start: usize, width: usize) -> StyledLine {
    if width == 0 {
        return StyledLine::default();
    }

    let end = start.saturating_add(width);
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut style = SpanStyle::Plain;
    let mut col: usize = 0;
    let mut done = false;

    for span in &line.spans {
        if done {
            break;
        }
        if span.text.is_empty() {
            continue;
        }

        if !text.is_empty() && style != span.style {
            spans.push(Span {
                text: std::mem::take(&mut text),
                style,
            });
        }
        style = span.style;

        for g in span.text.graphemes(true) {
            let w = display_width(g);
            let g_start = col;
            let g_end = col.saturating_add(w);

            if g_end <= start {
                col = g_end;
                continue;
            }

            if g_start >= end {
                done = true;
                break;
            }

            if g_start < start {
                text.push(' ');
                col = g_end;
                continue;
            }

            if g_end > end {
                done = true;
                break;
            }

            text.push_str(g);
            col = g_end;
        }
    }

    if !text.is_empty() {
        spans.push(Span { text, style });
    }

    StyledLine { spans }
}

/// Number of rows `line` takes when wrapped at `width` (same rules as `wrap_line`).
pub fn row_count(line: &StyledLine, width: usize) -> usize {
    wrap_line(line, width).len()
}
