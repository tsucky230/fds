//! App state transitions (AC 7-10, 20, 24-31 / R-110, R-150-3, R-300-2).

use std::path::{Path, PathBuf};

use fds::app::{App, Effect, Mode};
use fds::command::Command;
use fds::entry::{EntryKind, FileEntry, LinkTarget};
use fds::i18n::Catalog;
use fds::message::{Feature, Message};
use fds::pane::CursorFocus;

fn root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\")
    } else {
        PathBuf::from("/")
    }
}

fn entry(dir: &Path, name: &str, kind: EntryKind) -> FileEntry {
    FileEntry::new(dir.join(name), name.into(), kind, false)
}

/// Starts the app in `dir` and completes the first load with `listing`.
fn started(dir: PathBuf, listing: Vec<FileEntry>) -> App {
    let (mut app, effect) = App::new(dir.clone());
    let Effect::Load { id, dir: d, focus } = effect else {
        panic!("expected Load, got {effect:?}")
    };
    assert_eq!(d, dir);
    assert_eq!(focus, CursorFocus::First);
    app.on_listing(id, Ok(listing));
    app
}

fn work() -> PathBuf {
    root().join("work")
}

fn work_app() -> App {
    let w = work();
    started(
        w.clone(),
        vec![
            entry(&w, "src", EntryKind::Directory),
            entry(&w, "link", EntryKind::Symlink(LinkTarget::Directory)),
            entry(&w, "a.txt", EntryKind::File),
            entry(&w, "dead", EntryKind::Symlink(LinkTarget::Broken)),
        ],
    )
}

fn cursor_to(app: &mut App, name: &str) {
    while app.pane.cursor_entry().map(|e| e.display_name.as_str()) != Some(name) {
        let before = app.pane.cursor;
        app.handle(Command::CursorDown);
        assert_ne!(before, app.pane.cursor, "{name} not found");
    }
}

#[test]
fn start_requests_load_and_shows_entries() {
    let app = work_app();
    assert!(!app.is_loading());
    assert_eq!(app.pane.entries.len(), 5); // [..] + 4
}

#[test]
fn enter_directory_requests_load_with_first_focus() {
    let mut app = work_app();
    cursor_to(&mut app, "src");
    let effect = app.handle(Command::Enter);
    assert!(
        matches!(&effect, Effect::Load { dir, focus: CursorFocus::First, .. } if *dir == work().join("src"))
    );
    assert!(app.is_loading());
}

#[test]
fn enter_symlink_to_directory_uses_link_path() {
    let mut app = work_app();
    cursor_to(&mut app, "link");
    let effect = app.handle(Command::Enter);
    assert!(matches!(&effect, Effect::Load { dir, .. } if *dir == work().join("link")));
}

#[test]
fn enter_on_parent_link_goes_up() {
    let mut app = work_app();
    app.handle(Command::CursorUp);
    let effect = app.handle(Command::Enter);
    assert!(
        matches!(&effect, Effect::Load { dir, focus: CursorFocus::Name(n), .. } if *dir == root() && n == "work")
    );
}

#[test]
fn enter_on_file_opens_the_viewer() {
    // R-110-2: from Phase 2, Enter on a regular file behaves like View.
    let mut app = work_app();
    cursor_to(&mut app, "a.txt");
    let effect = app.handle(Command::Enter);
    assert!(
        matches!(&effect, Effect::OpenViewer { path, kind: None, .. } if *path == work().join("a.txt"))
    );
    assert_eq!(app.mode, Mode::ViewerLoading);
}

#[test]
fn enter_on_broken_symlink_reports_it() {
    let mut app = work_app();
    cursor_to(&mut app, "dead");
    assert_eq!(app.handle(Command::Enter), Effect::None);
    assert_eq!(
        app.status,
        Some(Message::BrokenSymlink {
            name: "dead".into()
        })
    );
}

#[test]
fn parent_requests_load_focused_on_current_name() {
    let mut app = work_app();
    let effect = app.handle(Command::ParentDirectory);
    assert!(
        matches!(&effect, Effect::Load { dir, focus: CursorFocus::Name(n), .. } if *dir == root() && n == "work")
    );
}

#[test]
fn left_arrow_goes_to_parent_like_h() {
    // In the list, Left is Parent (the destination guide says "Left/h: up"); the viewer scrolls.
    let mut app = work_app();
    let effect = app.handle(Command::ScrollLeft);
    assert!(
        matches!(&effect, Effect::Load { dir, focus: CursorFocus::Name(n), .. } if *dir == root() && n == "work"),
        "{effect:?}"
    );
}

#[test]
fn parent_at_root_does_not_move() {
    let mut app = started(root(), vec![entry(&root(), "x", EntryKind::File)]);
    assert_eq!(app.handle(Command::ParentDirectory), Effect::None);
    assert_eq!(app.status, Some(Message::AtRoot));
    assert!(!app.is_loading());
}

#[test]
fn full_navigation_round_trip() {
    let mut app = work_app();
    cursor_to(&mut app, "src");
    let Effect::Load { id, dir, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_listing(id, Ok(vec![entry(&dir, "main.rs", EntryKind::File)]));
    assert_eq!(app.pane.current_dir, work().join("src"));

    let Effect::Load { id, dir, .. } = app.handle(Command::ParentDirectory) else {
        panic!()
    };
    let w = work();
    app.on_listing(
        id,
        Ok(vec![
            entry(&w, "src", EntryKind::Directory),
            entry(&w, "a.txt", EntryKind::File),
        ]),
    );
    assert_eq!(dir, w);
    assert_eq!(app.pane.cursor_entry().unwrap().display_name, "src");
}

#[test]
fn failed_load_keeps_listing_and_reports_error() {
    let mut app = work_app();
    cursor_to(&mut app, "src");
    let Effect::Load { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_listing(id, Err("Access is denied.".into()));
    assert_eq!(app.pane.current_dir, work());
    assert!(!app.is_loading());
    assert_eq!(
        app.status,
        Some(Message::CannotOpen {
            path: work().join("src").to_string_lossy().into_owned(),
            reason: "Access is denied.".into()
        })
    );
}

#[test]
fn stale_results_are_ignored() {
    let mut app = work_app();
    cursor_to(&mut app, "src");
    let Effect::Load { id: first, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    let Effect::Load { id: second, .. } = app.handle(Command::Refresh) else {
        panic!()
    };
    assert_ne!(first, second);
    app.on_listing(first, Ok(Vec::new()));
    assert_eq!(app.pane.current_dir, work(), "stale result must not apply");
    assert!(app.is_loading());
    app.on_listing(second, Ok(Vec::new()));
    assert!(!app.is_loading());
}

#[test]
fn refresh_keeps_focus() {
    let mut app = work_app();
    let effect = app.handle(Command::Refresh);
    assert!(
        matches!(&effect, Effect::Load { dir, focus: CursorFocus::Keep, .. } if *dir == work())
    );
}

#[test]
fn help_opens_and_any_command_closes_it() {
    let mut app = work_app();
    app.handle(Command::Help);
    assert_eq!(app.mode, Mode::Help);
    let cursor = app.pane.cursor;
    // Quit while Help is shown only closes Help.
    assert_eq!(app.handle(Command::Quit), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
    app.handle(Command::Help);
    app.handle(Command::CursorDown);
    assert_eq!(
        app.pane.cursor, cursor,
        "command is not executed while closing Help"
    );
}

#[test]
fn quit_and_suspend_effects() {
    let mut app = work_app();
    assert_eq!(app.handle(Command::Suspend), Effect::Suspend);
    assert_eq!(app.handle(Command::ForceQuit), Effect::Quit);
    // q / F10 ask first (R-200-14; tests/quit_confirm.rs).
    assert_eq!(app.handle(Command::Quit), Effect::None);
    assert_eq!(app.mode, Mode::ConfirmQuit);
}

#[test]
fn toggle_hidden_reports_state() {
    let mut app = work_app();
    app.handle(Command::ToggleHidden);
    assert_eq!(app.status, Some(Message::HiddenHidden));
    app.handle(Command::ToggleHidden);
    assert_eq!(app.status, Some(Message::HiddenShown));
}

#[test]
fn next_command_clears_status() {
    let mut app = work_app();
    app.handle(Command::View);
    app.handle(Command::CursorDown);
    assert_eq!(app.status, None);
}

#[test]
fn messages_render_in_english() {
    let c = Catalog::english();
    let m = Message::NotAvailable {
        feature: Feature::Copy,
        phase: 3,
    };
    assert_eq!(m.render(&c), "Copy is not available yet (Phase 3)");
    let m = Message::CannotOpen {
        path: "/x".into(),
        reason: "denied".into(),
    };
    assert_eq!(m.render(&c), "Cannot open /x: denied");
}

#[test]
fn duplicate_load_of_same_directory_is_not_requested() {
    // Mashing Enter while a slow directory loads must not spawn a thread per key press.
    let mut app = work_app();
    cursor_to(&mut app, "src");
    let Effect::Load { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    assert_eq!(app.handle(Command::Enter), Effect::None);
    assert!(app.is_loading());
    // The original request still completes normally.
    app.on_listing(id, Ok(Vec::new()));
    assert_eq!(app.pane.current_dir, work().join("src"));
}

#[test]
fn load_of_different_directory_supersedes_pending_one() {
    let mut app = work_app();
    cursor_to(&mut app, "src");
    let Effect::Load { id: first, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    let Effect::Load {
        id: second, dir, ..
    } = app.handle(Command::ParentDirectory)
    else {
        panic!("a different directory must still be requested")
    };
    assert_ne!(first, second);
    assert_eq!(dir, root());
}

#[test]
fn paging_commands_move_the_cursor() {
    let mut app = work_app(); // [..], link, src, a.txt, dead
    assert_eq!(app.handle(Command::End), Effect::None);
    assert_eq!(app.pane.cursor, 4);
    app.handle(Command::Home);
    assert_eq!(app.pane.cursor, 0);
    app.handle(Command::PageDown);
    assert!(app.pane.cursor > 0);
    app.handle(Command::PageUp);
    assert_eq!(app.pane.cursor, 0);
}
