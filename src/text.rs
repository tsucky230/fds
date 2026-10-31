//! Display width and safe truncation (design prompt §17 / R-303, R-304). Pure logic.
//!
//! Works in terminal display width (columns), not Rust string length (bytes or chars).

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Returns the display width. Same value as `unicode_width::UnicodeWidthStr::width`.
/// Full-width characters are 2, combining characters are 0.
pub fn display_width(s: &str) -> usize {
    s.width()
}

/// Replaces control characters in a file name for display (prevents escape-sequence injection).
///
/// Behavior:
/// - Each char where `char::is_control()` is true (C0, DEL, C1; includes newline, TAB, ESC)
///   is replaced with `'?'`.
/// - All other chars are kept as is.
pub fn sanitize_for_display(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Truncates so the display width is at most `max_width`.
///
/// Behavior:
/// - If `display_width(s) <= max_width`, return `s` unchanged.
/// - Otherwise, take grapheme clusters (`unicode_segmentation`, extended) from the start and append
///   `'…'` (width 1), returning the longest such string whose total width is at most `max_width`.
///   Never cut inside a grapheme (e.g. inside combining sequences or joined emoji).
///   If a full-width char would straddle the limit, leave it out (the result may be narrower
///   than `max_width`).
/// - If `max_width == 0`, return an empty string.
/// - If `max_width == 1` and truncation is needed, return `"…"`.
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    if display_width(s) <= max_width {
        return s.to_string();
    }

    let mut used = 0usize;
    let mut prefix = String::new();

    for g in s.graphemes(true) {
        let w = g.width();
        if used.saturating_add(w).saturating_add(1) <= max_width {
            prefix.push_str(g);
            used += w;
        } else {
            break;
        }
    }

    prefix.push('…');
    prefix
}
