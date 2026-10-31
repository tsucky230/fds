//! Display width and truncation (AC 17, 18 / R-303, R-304).

use fds::text::{display_width, sanitize_for_display, truncate_to_width};

#[test]
fn width_counts_columns_not_bytes() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("日本語"), 6);
    assert_eq!(display_width("e\u{301}"), 1); // e + combining acute
    assert_eq!(display_width(""), 0);
}

#[test]
fn sanitize_replaces_control_characters() {
    assert_eq!(sanitize_for_display("a\nb\tc\u{1b}[31m"), "a?b?c?[31m");
    assert_eq!(sanitize_for_display("x\u{7f}\u{9b}y"), "x??y");
    assert_eq!(sanitize_for_display("日本 ok"), "日本 ok");
}

#[test]
fn short_text_is_unchanged() {
    assert_eq!(truncate_to_width("abc", 3), "abc");
    assert_eq!(truncate_to_width("abc", 10), "abc");
    assert_eq!(truncate_to_width("", 0), "");
}

#[test]
fn long_ascii_gets_ellipsis() {
    assert_eq!(truncate_to_width("abcdef", 4), "abc…");
    assert_eq!(truncate_to_width("abcdef", 1), "…");
    assert_eq!(truncate_to_width("abcdef", 0), "");
}

#[test]
fn full_width_is_not_split() {
    // "日本語" is 6 columns. With 4 columns: "日" (2) + "…" (1) = 3; "日本" + "…" = 5 > 4.
    assert_eq!(truncate_to_width("日本語", 4), "日…");
    assert_eq!(truncate_to_width("日本語", 5), "日本…");
    let t = truncate_to_width("日本語", 2);
    assert!(display_width(&t) <= 2);
    assert_eq!(t, "…");
}

#[test]
fn graphemes_are_not_split() {
    // "e + combining" must stay together.
    let s = "e\u{301}e\u{301}e\u{301}e\u{301}";
    assert_eq!(truncate_to_width(s, 3), "e\u{301}e\u{301}…");
    // Family emoji (ZWJ sequence) is one grapheme; never cut in the middle.
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}xyz";
    let t = truncate_to_width(family, 3);
    assert!(t == "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}…" || t == "…");
    assert!(display_width(&t) <= 3);
}

#[test]
fn result_never_exceeds_width() {
    let samples = ["abc日本語def", "😀😀😀😀", "ｱｲｳｴｵ", "a\u{301}b\u{302}c"];
    for s in samples {
        for w in 0..12 {
            let t = truncate_to_width(s, w);
            assert!(display_width(&t) <= w, "{s:?} at {w} -> {t:?}");
        }
    }
}
