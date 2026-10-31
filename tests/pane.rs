//! Pane state (AC 4-6, 9, 15, 24, 26-29 / R-101, R-102, R-104, R-107, R-109).

use std::path::{Path, PathBuf};

use fds::entry::{EntryKind, FileEntry};
use fds::pane::{CursorFocus, Pane};

fn root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\")
    } else {
        PathBuf::from("/")
    }
}

fn dir() -> PathBuf {
    root().join("work")
}

fn file(dir: &Path, name: &str) -> FileEntry {
    FileEntry::new(
        dir.join(name),
        name.into(),
        EntryKind::File,
        name.starts_with('.'),
    )
}

fn folder(dir: &Path, name: &str) -> FileEntry {
    FileEntry::new(
        dir.join(name),
        name.into(),
        EntryKind::Directory,
        name.starts_with('.'),
    )
}

/// `/work` with: [..], .git/, src/, target/, .env, a.txt, b.txt
fn sample(dir: &Path) -> Vec<FileEntry> {
    vec![
        file(dir, "b.txt"),
        folder(dir, "src"),
        file(dir, ".env"),
        folder(dir, ".git"),
        file(dir, "a.txt"),
        folder(dir, "target"),
    ]
}

fn names(p: &Pane) -> Vec<String> {
    p.entries.iter().map(|e| e.display_name.clone()).collect()
}

fn loaded() -> Pane {
    let mut p = Pane::new(dir());
    p.set_listing(dir(), sample(&dir()), CursorFocus::First);
    p
}

fn cursor_name(p: &Pane) -> String {
    p.cursor_entry()
        .map(|e| e.display_name.clone())
        .unwrap_or_default()
}

#[test]
fn listing_adds_parent_link_and_sorts() {
    let p = loaded();
    assert_eq!(
        names(&p),
        ["..", ".git", "src", "target", ".env", "a.txt", "b.txt"]
    );
    assert_eq!(p.current_dir, dir());
}

#[test]
fn root_has_no_parent_link() {
    let mut p = Pane::new(root());
    p.set_listing(root(), vec![file(&root(), "x")], CursorFocus::First);
    assert_eq!(names(&p), ["x"]);
    assert_eq!(p.cursor, 0);
}

#[test]
fn enter_places_cursor_after_parent_link() {
    let p = loaded();
    assert_eq!(p.cursor, 1);
    assert_eq!(cursor_name(&p), ".git");
}

#[test]
fn empty_directory_puts_cursor_on_parent_link() {
    let mut p = Pane::new(dir());
    p.set_listing(dir(), Vec::new(), CursorFocus::First);
    assert_eq!(names(&p), [".."]);
    assert_eq!(p.cursor, 0);
}

#[test]
fn empty_root_does_not_panic() {
    let mut p = Pane::new(root());
    p.set_listing(root(), Vec::new(), CursorFocus::First);
    assert!(p.entries.is_empty());
    assert_eq!(p.cursor_entry(), None);
    p.cursor_up();
    p.cursor_down();
    assert!(!p.toggle_mark());
    p.ensure_visible(5);
    p.toggle_hidden();
    assert_eq!((p.cursor, p.scroll), (0, 0));
}

#[test]
fn parent_focus_selects_previous_directory() {
    let mut p = Pane::new(dir());
    p.set_listing(dir(), sample(&dir()), CursorFocus::Name("target".into()));
    assert_eq!(cursor_name(&p), "target");
}

#[test]
fn parent_focus_falls_back_to_first() {
    let mut p = Pane::new(dir());
    p.set_listing(dir(), sample(&dir()), CursorFocus::Name("gone".into()));
    assert_eq!(p.cursor, 1);
}

#[test]
fn cursor_stays_in_range() {
    let mut p = loaded();
    p.cursor_up();
    p.cursor_up();
    p.cursor_up();
    assert_eq!(p.cursor, 0);
    for _ in 0..20 {
        p.cursor_down();
    }
    assert_eq!(p.cursor, p.entries.len() - 1);
}

#[test]
fn mark_toggles_and_skips_parent_link() {
    let mut p = loaded();
    assert!(p.toggle_mark()); // .git
    assert!(p.entries[1].marked);
    assert_eq!(p.cursor, 1, "mark does not move the cursor");
    assert!(p.toggle_mark());
    assert!(!p.entries[1].marked);

    p.cursor_up(); // [..]
    assert!(!p.toggle_mark());
    assert!(!p.entries[0].marked);
    assert!(p.marked_entries().is_empty());
}

#[test]
fn marks_can_hold_multiple_entries() {
    let mut p = loaded();
    p.toggle_mark();
    p.cursor_down();
    p.toggle_mark();
    let marked: Vec<_> = p
        .marked_entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    assert_eq!(marked, [".git", "src"]);
}

#[test]
fn directory_change_clears_marks() {
    let mut p = loaded();
    p.toggle_mark();
    let other = root().join("other");
    p.set_listing(
        other.clone(),
        vec![folder(&other, ".git")],
        CursorFocus::First,
    );
    assert!(p.marked_entries().is_empty());
    // Coming back to the same directory with First (not Keep) also clears.
    p.set_listing(dir(), sample(&dir()), CursorFocus::First);
    assert!(p.marked_entries().is_empty());
}

#[test]
fn refresh_keeps_marks_and_cursor() {
    let mut p = loaded();
    p.cursor_down(); // src
    p.toggle_mark();
    p.cursor_down(); // target
    p.set_listing(dir(), sample(&dir()), CursorFocus::Keep);
    assert_eq!(cursor_name(&p), "target");
    let marked: Vec<_> = p
        .marked_entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    assert_eq!(marked, ["src"]);
}

#[test]
fn refresh_with_vanished_entry_keeps_index() {
    let mut p = loaded();
    for _ in 0..3 {
        p.cursor_down();
    } // index 4: .env
    assert_eq!(cursor_name(&p), ".env");
    let listing: Vec<_> = sample(&dir())
        .into_iter()
        .filter(|e| e.display_name != ".env")
        .collect();
    p.set_listing(dir(), listing, CursorFocus::Keep);
    assert_eq!(p.cursor, 4);
    assert_eq!(cursor_name(&p), "a.txt");
}

#[test]
fn refresh_with_vanished_last_entry_clamps() {
    let mut p = loaded();
    for _ in 0..10 {
        p.cursor_down();
    }
    assert_eq!(cursor_name(&p), "b.txt");
    p.set_listing(dir(), vec![file(&dir(), "a.txt")], CursorFocus::Keep);
    assert_eq!(p.cursor, p.entries.len() - 1);
    assert_eq!(cursor_name(&p), "a.txt");
}

#[test]
fn hidden_toggle_filters_and_persists() {
    let mut p = loaded();
    assert!(p.show_hidden);
    p.toggle_hidden();
    assert!(!p.show_hidden);
    assert_eq!(names(&p), ["..", "src", "target", "a.txt", "b.txt"]);

    // The setting survives a directory change.
    let other = root().join("other");
    p.set_listing(
        other.clone(),
        vec![file(&other, ".x"), file(&other, "y")],
        CursorFocus::First,
    );
    assert_eq!(names(&p), ["..", "y"]);

    p.toggle_hidden();
    assert_eq!(names(&p), ["..", ".x", "y"]);
}

#[test]
fn hidden_toggle_moves_cursor_to_nearest_previous_visible() {
    let mut p = loaded();
    for _ in 0..3 {
        p.cursor_down();
    } // .env (index 4), previous visible after hiding: target
    p.toggle_hidden();
    assert_eq!(cursor_name(&p), "target");
    p.toggle_hidden();
    assert_eq!(cursor_name(&p), "target");
}

#[test]
fn marks_survive_hidden_toggle() {
    let mut p = loaded();
    p.cursor_down(); // src
    p.toggle_mark();
    p.toggle_hidden();
    p.toggle_hidden();
    let marked: Vec<_> = p
        .marked_entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    assert_eq!(marked, ["src"]);
}

#[test]
fn scroll_follows_cursor() {
    let d = dir();
    let listing: Vec<_> = (0..50).map(|i| file(&d, &format!("f{i:02}"))).collect();
    let mut p = Pane::new(d.clone());
    p.set_listing(d, listing, CursorFocus::First);
    p.ensure_visible(10);
    assert_eq!(p.scroll, 0);
    for _ in 0..20 {
        p.cursor_down();
    }
    p.ensure_visible(10);
    assert_eq!(p.cursor, 21);
    assert_eq!(p.scroll, 12);
    for _ in 0..15 {
        p.cursor_up();
    }
    p.ensure_visible(10);
    assert_eq!(p.scroll, 6);
}

#[test]
fn scroll_clamps_when_terminal_grows() {
    let d = dir();
    let listing: Vec<_> = (0..30).map(|i| file(&d, &format!("f{i:02}"))).collect();
    let mut p = Pane::new(d.clone());
    p.set_listing(d, listing, CursorFocus::First);
    for _ in 0..40 {
        p.cursor_down();
    }
    p.ensure_visible(5);
    assert_eq!(p.scroll, 26);
    p.ensure_visible(20);
    assert_eq!(p.scroll, 11);
    p.ensure_visible(100);
    assert_eq!(p.scroll, 0);
    p.ensure_visible(0);
    assert_eq!(p.scroll, 0);
}

fn pane_with(n: usize) -> Pane {
    let d = dir();
    let listing: Vec<_> = (0..n).map(|i| file(&d, &format!("f{i:03}"))).collect();
    let mut p = Pane::new(d.clone());
    p.set_listing(d, listing, CursorFocus::First);
    p
}

#[test]
fn page_moves_by_visible_rows_and_clamps() {
    let mut p = pane_with(50); // 51 entries with [..]
    p.ensure_visible(10); // page size = 10
    assert_eq!(p.cursor, 1);
    p.page_down();
    assert_eq!(p.cursor, 11);
    p.page_down();
    p.page_down();
    p.page_down();
    p.page_down();
    assert_eq!(p.cursor, 50, "clamped to the last entry");
    p.page_up();
    assert_eq!(p.cursor, 40);
    for _ in 0..10 {
        p.page_up();
    }
    assert_eq!(p.cursor, 0, "clamped to the first entry");
}

#[test]
fn page_before_first_render_moves_by_one() {
    let mut p = pane_with(5);
    p.page_down();
    assert_eq!(p.cursor, 2);
}

#[test]
fn home_and_end() {
    let mut p = pane_with(20);
    p.cursor_end();
    assert_eq!(p.cursor, 20);
    p.cursor_home();
    assert_eq!(p.cursor, 0);
}

#[test]
fn paging_empty_pane_does_not_panic() {
    let mut p = Pane::new(root());
    p.set_listing(root(), Vec::new(), CursorFocus::First);
    p.page_down();
    p.page_up();
    p.cursor_end();
    p.cursor_home();
    assert_eq!(p.cursor, 0);
}
