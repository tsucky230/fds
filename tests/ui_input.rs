//! Visible window of the one-line input box (Rename / mkdir / search). The cursor must always be
//! inside the window, whatever the text width.

use fds::text::display_width;
use fds::ui_input::window;

fn parts(w: &fds::ui_input::Window) -> (&str, &str, &str) {
    (w.before.as_str(), w.at.as_str(), w.after.as_str())
}

#[test]
fn short_text_is_shown_whole_with_cursor_at_end() {
    let w = window("abc", 3, 20);
    assert_eq!(parts(&w), ("abc", " ", ""));
    assert_eq!(w.col, 3);
}

#[test]
fn cursor_in_the_middle_splits_the_text() {
    let w = window("abcd", 1, 20);
    assert_eq!(parts(&w), ("a", "b", "cd"));
    assert_eq!(w.col, 1);
}

#[test]
fn long_text_scrolls_so_the_cursor_stays_visible() {
    let text = "0123456789abcdefghij";
    for cursor in 0..=20 {
        let w = window(text, cursor, 8);
        assert!(w.col < 8, "cursor {cursor} col {}", w.col);
        let shown = display_width(&w.before) + display_width(&w.at) + display_width(&w.after);
        assert!(shown <= 8, "cursor {cursor} shown {shown}");
    }
}

#[test]
fn wide_characters_count_two_columns() {
    let w = window("あいう", 2, 20);
    assert_eq!(w.before, "あい");
    assert_eq!(w.at, "う");
    assert_eq!(w.col, 4);
}

#[test]
fn control_characters_are_replaced_and_zero_width_is_safe() {
    let w = window("a\u{1b}b", 3, 10);
    assert_eq!(w.before, "a?b");
    let w = window("abc", 1, 0);
    assert_eq!(w.col, 0);
    let w = window("", 0, 5);
    assert_eq!((parts(&w), w.col), (("", " ", ""), 0));
}
