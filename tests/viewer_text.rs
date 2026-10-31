//! Text layout: lines, TABs, wrapping, horizontal slicing (P2-6).

use fds::text::display_width;
use fds::viewer::styled::{Span, SpanStyle, StyledLine};
use fds::viewer::text::{expand_line, row_count, slice_columns, split_lines, to_lines, wrap_line};

fn texts(lines: &[StyledLine]) -> Vec<String> {
    lines.iter().map(StyledLine::text).collect()
}

#[test]
fn splits_on_lf_crlf_and_cr() {
    assert_eq!(split_lines("a\nb\r\nc\rd"), ["a", "b", "c", "d"]);
    assert_eq!(split_lines("a\n"), ["a"]);
    assert_eq!(split_lines("a\n\nb"), ["a", "", "b"]);
    assert_eq!(split_lines("\n"), [""]);
    assert!(split_lines("").is_empty());
    assert_eq!(split_lines("a\r\n\r\nb"), ["a", "", "b"]);
}

#[test]
fn tabs_expand_by_display_width() {
    assert_eq!(expand_line("a\tb"), "a       b");
    assert_eq!(expand_line("\tx"), "        x");
    assert_eq!(expand_line("12345678\tx"), "12345678        x");
    assert_eq!(expand_line("日\tx"), "日      x");
    assert_eq!(expand_line("a\u{1b}[31m\u{7}"), "a?[31m?");
}

#[test]
fn to_lines_combines_split_and_expand() {
    let lines = to_lines("a\tb\r\nc");
    assert_eq!(texts(&lines), ["a       b", "c"]);
    assert!(
        lines
            .iter()
            .all(|l| l.spans.iter().all(|s| s.style == SpanStyle::Plain))
    );
}

#[test]
fn wraps_by_display_width() {
    let line = StyledLine::plain("abcdefghij");
    assert_eq!(texts(&wrap_line(&line, 4)), ["abcd", "efgh", "ij"]);
    let jp = StyledLine::plain("日本語テキスト");
    let rows = wrap_line(&jp, 5);
    assert_eq!(texts(&rows), ["日本", "語テ", "キス", "ト"]);
    assert!(rows.iter().all(|r| display_width(&r.text()) <= 5));
    assert_eq!(row_count(&jp, 5), 4);
}

#[test]
fn wrap_edge_cases() {
    assert_eq!(texts(&wrap_line(&StyledLine::plain(""), 10)), [""]);
    assert_eq!(texts(&wrap_line(&StyledLine::default(), 10)), [""]);
    assert_eq!(texts(&wrap_line(&StyledLine::plain("abc"), 0)), ["abc"]);
    // A full-width char wider than the width gets its own row.
    assert_eq!(
        texts(&wrap_line(&StyledLine::plain("日本"), 1)),
        ["日", "本"]
    );
    // Graphemes are not split.
    let s = "e\u{301}e\u{301}e\u{301}";
    assert_eq!(
        texts(&wrap_line(&StyledLine::plain(s), 2)),
        ["e\u{301}e\u{301}", "e\u{301}"]
    );
}

#[test]
fn wrap_keeps_styles_across_rows() {
    let line = StyledLine {
        spans: vec![
            Span {
                text: "abc".into(),
                style: SpanStyle::Strong,
            },
            Span {
                text: "def".into(),
                style: SpanStyle::Code,
            },
        ],
    };
    let rows = wrap_line(&line, 4);
    assert_eq!(texts(&rows), ["abcd", "ef"]);
    assert_eq!(
        rows[0].spans,
        vec![
            Span {
                text: "abc".into(),
                style: SpanStyle::Strong
            },
            Span {
                text: "d".into(),
                style: SpanStyle::Code
            },
        ]
    );
    assert_eq!(
        rows[1].spans,
        vec![Span {
            text: "ef".into(),
            style: SpanStyle::Code
        }]
    );
}

#[test]
fn slice_columns_for_horizontal_scroll() {
    let line = StyledLine::plain("0123456789");
    assert_eq!(slice_columns(&line, 0, 4).text(), "0123");
    assert_eq!(slice_columns(&line, 8, 4).text(), "89");
    assert_eq!(slice_columns(&line, 20, 4).text(), "");
    // A full-width char cut at the left edge becomes a space.
    let jp = StyledLine::plain("日本語");
    assert_eq!(slice_columns(&jp, 1, 4).text(), " 本");
    assert_eq!(slice_columns(&jp, 2, 4).text(), "本語");
    // Stops before a char that would exceed the width.
    assert_eq!(slice_columns(&jp, 0, 3).text(), "日");
}

/// Gemini §38 review: TAB expansion must be linear in the line length (8 MiB lines exist).
#[test]
fn tab_expansion_of_a_huge_line_is_fast() {
    use std::time::{Duration, Instant};
    let line = "a\t".repeat(200_000);
    let started = Instant::now();
    let out = expand_line(&line);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(out.len(), 200_000 * 8);
}
