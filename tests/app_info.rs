//! When the info panel asks for details: `App::want_info` / `App::on_info` (no filesystem here).

use std::path::PathBuf;

use fds::app::{App, Effect};
use fds::command::{Command, TextKey};
use fds::entry::{EntryKind, FileEntry};
use fds::info::{Info, InfoKind, MarkSummary};

fn work() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
}

fn app() -> App {
    let w = work();
    let (mut app, effect) = App::new(w.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![
            FileEntry::new(w.join("a.txt"), "a.txt".into(), EntryKind::File, false),
            FileEntry::new(w.join("b.txt"), "b.txt".into(), EntryKind::File, false),
        ]),
    );
    app
}

fn info_of(path: PathBuf) -> Info {
    Info {
        path,
        kind: InfoKind::File,
        size: Some(1),
        modified: None,
        created: None,
        unix_mode: None,
        owner: None,
        win_attrs: None,
        readonly: false,
        link_target: None,
        link_broken: false,
        dir_items: None,
    }
}

#[test]
fn asks_for_the_item_under_the_cursor_once() {
    let mut app = app();
    // The cursor starts on the first real entry (a.txt), after "[..]".
    let req = app.want_info().expect("first request");
    assert_eq!(req.path, Some(work().join("a.txt")));
    assert!(req.marked.is_empty());
    assert!(app.want_info().is_none(), "same target is not asked twice");
}

#[test]
fn a_moved_cursor_asks_again_with_a_new_id() {
    let mut app = app();
    let first = app.want_info().unwrap();
    app.handle(Command::CursorDown);
    let second = app.want_info().unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(second.path, Some(work().join("b.txt")));
}

#[test]
fn the_parent_link_has_no_details() {
    let mut app = app();
    app.handle(Command::CursorUp);
    assert!(app.want_info().is_none());
    assert!(app.info.is_none());
}

#[test]
fn marks_are_part_of_the_request() {
    let mut app = app();
    app.want_info();
    app.handle(Command::ToggleMark);
    let req = app.want_info().expect("marking changes the target");
    assert_eq!(req.marked.len(), 1);
}

#[test]
fn stale_answers_are_dropped_and_current_ones_kept() {
    let mut app = app();
    let first = app.want_info().unwrap();
    app.handle(Command::CursorDown);
    let second = app.want_info().unwrap();
    app.on_info(first.id, Some(info_of(work().join("a.txt"))), None);
    assert!(app.info.is_none(), "an answer for an old id is ignored");
    app.on_info(second.id, Some(info_of(work().join("b.txt"))), None);
    assert_eq!(app.info.as_ref().unwrap().path, work().join("b.txt"));
}

#[test]
fn mark_summary_is_kept_only_when_something_is_marked() {
    let mut app = app();
    app.handle(Command::ToggleMark);
    let req = app.want_info().unwrap();
    let summary = MarkSummary {
        files: 1,
        dirs: 0,
        bytes: 1,
    };
    app.on_info(req.id, Some(info_of(work().join("a.txt"))), Some(summary));
    assert_eq!(app.info_marks, Some(summary));
    app.handle(Command::ToggleMark);
    let req = app.want_info().unwrap();
    app.on_info(req.id, Some(info_of(work().join("a.txt"))), None);
    assert_eq!(app.info_marks, None);
}

#[test]
fn toggle_info_hides_and_shows_the_panel() {
    let mut app = app();
    assert!(app.show_info);
    app.want_info();
    app.handle(Command::ToggleInfo);
    assert!(!app.show_info);
    assert!(app.want_info().is_none(), "hidden panel asks for nothing");
    app.handle(Command::ToggleInfo);
    assert!(app.want_info().is_some(), "showing it again refreshes");
}

#[test]
fn nothing_is_asked_outside_normal_mode() {
    let mut app = app();
    app.handle(Command::Rename);
    assert!(app.want_info().is_none());
    app.handle_text(TextKey::Esc);
    app.handle(Command::Help);
    assert!(app.want_info().is_none());
}

#[test]
fn a_new_listing_refreshes_the_details() {
    let mut app = app();
    app.want_info();
    let Effect::Load { id, dir, .. } = app.handle(Command::Refresh) else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![FileEntry::new(
            dir.join("a.txt"),
            "a.txt".into(),
            EntryKind::File,
            false,
        )]),
    );
    assert!(app.want_info().is_some(), "size or time may have changed");
}
