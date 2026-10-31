//! R-407: archive commands move through App's pure modes and effects.

use std::io;
use std::path::{Path, PathBuf};

use fds::app::{App, Effect, Mode};
use fds::archive::{Action, Member};
use fds::command::{Command, TextKey};
use fds::entry::{EntryKind, FileEntry};

fn work() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
}

fn app_with(name: &str) -> App {
    let dir = work();
    let (mut app, Effect::Load { id, .. }) = App::new(dir.clone()) else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![FileEntry::new(
            dir.join(name),
            name.into(),
            EntryKind::File,
            false,
        )]),
    );
    app.handle(Command::CursorDown);
    app
}

#[test]
fn enter_browses_an_archive_and_parent_returns_inside_it() {
    let mut app = app_with("bundle.zip");
    let Effect::OpenArchive { id, path } = app.handle(Command::Enter) else {
        panic!("archive was not opened")
    };
    assert_eq!(path, work().join("bundle.zip"));
    assert_eq!(app.mode, Mode::ArchiveLoading);
    app.on_archive_loaded(
        id,
        Ok(vec![
            Member {
                path: "folder".into(),
                is_dir: true,
                size: 0,
            },
            Member {
                path: "folder/file.txt".into(),
                is_dir: false,
                size: 3,
            },
        ]),
    );
    assert_eq!(app.mode, Mode::ArchiveBrowse);
    app.handle(Command::Enter);
    assert_eq!(app.archive.as_ref().unwrap().prefix, Path::new("folder"));
    app.handle(Command::ParentDirectory);
    assert_eq!(app.archive.as_ref().unwrap().prefix, Path::new(""));
    app.handle(Command::Quit);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn create_uses_selected_file_and_refreshes_after_completion() {
    let mut app = app_with("a.txt");
    app.handle(Command::ArchiveCreate);
    assert_eq!(app.mode, Mode::Input);
    assert_eq!(app.input.as_ref().unwrap().editor.text(), "archive.zip");
    let Effect::StartArchive { id, action } = app.handle_text(TextKey::Enter) else {
        panic!("archive job not started")
    };
    assert_eq!(
        action,
        Action::Create {
            destination: work().join("archive.zip"),
            sources: vec![work().join("a.txt")]
        }
    );
    assert_eq!(app.mode, Mode::ArchiveRunning);
    let Effect::Load { dir, .. } = app.on_archive_finished(id, Ok(())) else {
        panic!("list not refreshed")
    };
    assert_eq!(dir, work());
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn extract_uses_a_new_sibling_directory_and_can_be_cancelled() {
    let mut app = app_with("bundle.tar.gz");
    app.handle(Command::ArchiveExtract);
    assert_eq!(app.mode, Mode::Input);
    assert_eq!(app.input.as_ref().unwrap().editor.text(), "bundle");
    let Effect::StartArchive { id, action } = app.handle_text(TextKey::Enter) else {
        panic!("extract not started")
    };
    assert_eq!(
        action,
        Action::Extract {
            source: work().join("bundle.tar.gz"),
            destination: work().join("bundle")
        }
    );
    assert_eq!(app.handle(Command::Back), Effect::CancelArchive { id });
    assert_eq!(app.mode, Mode::ArchiveRunning);
    app.on_archive_finished(
        id,
        Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled")),
    );
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn a_stale_archive_result_cannot_replace_the_current_screen() {
    let mut app = app_with("bundle.zip");
    let Effect::OpenArchive { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.handle(Command::Back);
    assert_eq!(app.mode, Mode::Normal);
    app.on_archive_loaded(id, Ok(vec![]));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.archive.is_none());
}

#[test]
fn cancelling_extract_from_the_archive_returns_to_its_browser() {
    let mut app = app_with("bundle.zip");
    let Effect::OpenArchive { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_archive_loaded(id, Ok(vec![]));
    app.handle(Command::ArchiveExtract);
    assert_eq!(app.mode, Mode::Input);
    app.handle_text(TextKey::Esc);
    assert_eq!(app.mode, Mode::ArchiveBrowse);
    assert!(app.archive.is_some());
}
