//! Viewer scroll state (P2-6, P2-10, P2-12).

use std::fs;
use std::path::PathBuf;

use fds::viewer::hex::HexSource;
use fds::viewer::styled::StyledLine;
use fds::viewer::view::ViewerState;
use fds::viewer::{Document, Opened, TextDoc, ViewerKind};

fn lines_viewer(lines: &[&str]) -> ViewerState {
    let doc = TextDoc {
        lines: lines.iter().map(|l| StyledLine::plain(*l)).collect(),
        encoding: "UTF-8".into(),
        truncated: false,
        file_len: 0,
    };
    ViewerState::new(Opened {
        path: PathBuf::from("f.txt"),
        candidates: vec![ViewerKind::Text, ViewerKind::Hex],
        kind: ViewerKind::Text,
        document: Document::Lines(doc),
        encoding: None,
        fallback: None,
    })
}

fn numbered(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("line{i}")).collect()
}

fn visible(v: &ViewerState) -> Vec<String> {
    v.visible_lines().iter().map(StyledLine::text).collect()
}

#[test]
fn scrolls_logical_lines_and_clamps() {
    let owned = numbered(10);
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut v = lines_viewer(&refs);
    v.set_viewport(20, 4);
    assert_eq!(visible(&v), ["line0", "line1", "line2", "line3"]);
    v.scroll_down(1);
    assert_eq!(visible(&v)[0], "line1");
    v.page_down();
    assert_eq!(visible(&v)[0], "line5");
    v.page_down();
    assert_eq!(
        visible(&v),
        ["line6", "line7", "line8", "line9"],
        "stops at the last page"
    );
    v.scroll_up(100);
    assert_eq!(visible(&v)[0], "line0");
    v.end();
    assert_eq!(visible(&v)[0], "line6");
    v.home();
    assert_eq!(visible(&v)[0], "line0");
}

#[test]
fn wrapped_rows_scroll_one_row_at_a_time() {
    let mut v = lines_viewer(&["aaaaaaaaaa", "b"]); // wraps to 3 rows at width 4
    v.set_viewport(4, 2);
    assert_eq!(visible(&v), ["aaaa", "aaaa"]);
    v.scroll_down(1);
    assert_eq!(visible(&v), ["aaaa", "aa"]);
    v.scroll_down(1);
    assert_eq!(visible(&v), ["aa", "b"]);
    v.scroll_down(1);
    assert_eq!(visible(&v), ["aa", "b"], "already at the end");
    v.end();
    assert_eq!(visible(&v), ["aa", "b"]);
}

#[test]
fn resize_keeps_the_same_logical_line_on_top() {
    let mut v = lines_viewer(&["aaaaaaaaaa", "second", "third"]);
    v.set_viewport(4, 2);
    v.scroll_down(2); // top: line 0, row 2 ("aa")
    assert_eq!(visible(&v)[0], "aa");
    v.set_viewport(40, 2); // line 0 is now one row; row index must clamp
    assert_eq!(visible(&v)[0], "aaaaaaaaaa");
    assert_eq!(v.top_line, 0);
}

#[test]
fn no_wrap_and_horizontal_scroll() {
    let long = "0123456789abcdefghij";
    let mut v = lines_viewer(&[long, "x"]);
    v.set_viewport(8, 5);
    v.toggle_wrap();
    assert!(!v.wrap);
    assert_eq!(visible(&v), ["01234567", "x"]);
    v.scroll_right();
    assert_eq!(visible(&v)[0], "89abcdef");
    for _ in 0..10 {
        v.scroll_right();
    }
    assert!(
        !visible(&v)[0].is_empty(),
        "at least one column of the widest line stays visible"
    );
    v.scroll_left();
    v.scroll_left();
    v.scroll_left();
    v.scroll_left();
    assert_eq!(visible(&v)[0], "01234567");
    v.toggle_wrap();
    v.scroll_right();
    assert_eq!(v.hscroll, 0, "no horizontal scroll while wrapping");
}

#[test]
fn empty_document_and_before_first_render() {
    let mut v = lines_viewer(&[]);
    v.scroll_down(5);
    v.page_down();
    v.end();
    v.scroll_up(1);
    assert!(visible(&v).is_empty());
    v.set_viewport(10, 3);
    assert!(visible(&v).is_empty());
}

#[test]
fn next_kind_cycles_candidates() {
    let v = lines_viewer(&["a"]);
    assert_eq!(v.next_kind(), Some(ViewerKind::Hex));
}

#[test]
fn hex_rows_scroll_and_end() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("b.bin");
    let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    fs::write(&path, &data).unwrap();
    let mut v = ViewerState::new(Opened {
        path: path.clone(),
        candidates: vec![ViewerKind::Hex],
        kind: ViewerKind::Hex,
        document: Document::Hex(HexSource::open(&path, 1000).unwrap()),
        encoding: None,
        fallback: None,
    });
    v.set_viewport(80, 10); // 16 bytes per row, 63 rows
    let rows = v.visible_hex();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0], (0, data[0..16].to_vec()));
    v.page_down();
    assert_eq!(v.visible_hex()[0].0, 160);
    v.end();
    let rows = v.visible_hex();
    assert_eq!(rows.last().unwrap(), &(992, data[992..1000].to_vec()));
    assert_eq!(rows.len(), 10);
    v.set_viewport(60, 10); // 8 bytes per row
    assert_eq!(v.hex_per_row(), 8);
    assert!(v.visible_hex().iter().all(|(_, b)| b.len() <= 8));
    assert_eq!(v.next_kind(), None);
}

/// P2-12 / Gemini §38 review: scrolling an 8 MiB document must not re-wrap the whole file per key.
/// The bound is generous for debug builds on slow CI machines; the old O(file) code took seconds.
#[test]
fn scrolling_a_large_document_is_fast() {
    use std::time::{Duration, Instant};
    let line = "0123456789abcdef".repeat(4); // 64 columns
    let lines: Vec<&str> = std::iter::repeat_n(line.as_str(), 131_072).collect();
    let mut v = lines_viewer(&lines);
    let started = Instant::now();
    v.set_viewport(40, 30); // every line wraps to 2 rows
    for _ in 0..20 {
        v.scroll_down(1);
        v.page_down();
        v.set_viewport(40, 30);
        let _ = v.visible_lines();
    }
    v.end();
    v.set_viewport(40, 30);
    v.scroll_up(1);
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
    assert_eq!(visible(&v).len(), 30);
    // Right scroll with wrap off must not scan every line either.
    v.toggle_wrap();
    let started = Instant::now();
    for _ in 0..20 {
        v.scroll_right();
    }
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "took {:?}",
        started.elapsed()
    );
}
