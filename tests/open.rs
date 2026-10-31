//! Opening outside FDS: which files are programs, the question before running one, and the
//! applications from the settings (R-110-3, R-110-4).

use std::path::PathBuf;

use fds::app::{App, Effect, Mode};
use fds::command::{Command, TextKey};
use fds::entry::{EntryKind, FileEntry, LinkTarget};
use fds::message::Message;
use fds::open::{Launch, Os, apps_from_settings, classify_name};
use fds::settings::Settings;

fn root() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" })
}

fn app_on(name: &str, kind: EntryKind) -> App {
    let (mut app, effect) = App::new(root());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![FileEntry::new(
            root().join(name),
            name.into(),
            kind,
            false,
        )]),
    );
    while app.pane.cursor_entry().unwrap().display_name != name {
        app.handle(Command::CursorDown);
    }
    app
}

#[test]
fn windows_programs_are_found_however_they_are_written() {
    for name in [
        "a.exe",
        "A.EXE",
        "a.exe.",
        "a.exe ",
        "report.pdf.exe",
        "a.com",
    ] {
        assert_eq!(
            classify_name(name, Os::Windows, false),
            Launch::RunDirectly,
            "{name}"
        );
    }
    for name in [
        "a.bat", "a.CMD", "a.ps1", "a.vbs", "a.js", "a.lnk", "a.reg", "a.msi", "a.scr",
    ] {
        assert_eq!(
            classify_name(name, Os::Windows, false),
            Launch::RunThroughOs,
            "{name}"
        );
    }
    for name in ["a.pdf", "a.txt", "a", "a.exe.txt"] {
        assert_eq!(
            classify_name(name, Os::Windows, false),
            Launch::Document,
            "{name}"
        );
    }
}

#[test]
fn unix_programs_depend_on_the_name_and_the_permission() {
    assert_eq!(
        classify_name("a.app", Os::MacOs, false),
        Launch::RunThroughOs
    );
    assert_eq!(
        classify_name("a.command", Os::MacOs, false),
        Launch::RunThroughOs
    );
    assert_eq!(
        classify_name("a.command", Os::MacOs, true),
        Launch::RunDirectly
    );
    assert_eq!(
        classify_name("a.desktop", Os::Other, false),
        Launch::RunThroughOs
    );
    for os in [Os::MacOs, Os::Other] {
        assert_eq!(classify_name("tool", os, true), Launch::RunDirectly);
        assert_eq!(classify_name("run.SH", os, true), Launch::RunDirectly);
        // No permission: a document. The permission alone does not make a PDF a program.
        assert_eq!(classify_name("tool", os, false), Launch::Document);
        assert_eq!(classify_name("run.sh", os, false), Launch::Document);
        assert_eq!(classify_name("a.pdf", os, true), Launch::Document);
    }
}

#[cfg(unix)]
#[test]
fn a_link_is_judged_by_its_target() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let tmp = tempfile::tempdir().unwrap();
    let program = tmp.path().join("tool");
    std::fs::write(&program, "#!/bin/sh\n").unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&program, permissions).unwrap();
    let link = tmp.path().join("notes.txt");
    symlink(&program, &link).unwrap();
    assert_eq!(fds::open::classify(&link), Launch::RunDirectly);
}

#[test]
fn o_hands_files_to_the_event_loop_and_refuses_the_rest() {
    let mut app = app_on("a.pdf", EntryKind::File);
    assert_eq!(
        app.handle(Command::OpenExternal),
        Effect::OpenExternal {
            path: root().join("a.pdf")
        }
    );

    let mut app = app_on("dir", EntryKind::Directory);
    assert_eq!(app.handle(Command::OpenExternal), Effect::None);
    assert_eq!(app.status, Some(Message::NotAFile { name: "dir".into() }));

    let mut app = app_on("gone", EntryKind::Symlink(LinkTarget::Broken));
    assert_eq!(app.handle(Command::OpenExternal), Effect::None);
    assert_eq!(
        app.status,
        Some(Message::BrokenSymlink {
            name: "gone".into()
        })
    );
}

fn asking() -> App {
    let mut app = app_on("tool", EntryKind::File);
    app.ask_run(root().join("tool"));
    assert_eq!(app.mode, Mode::ConfirmRun);
    assert!(app.wants_text());
    app
}

#[test]
fn only_y_runs_a_program() {
    for key in ['y', 'Y'] {
        let mut app = asking();
        assert_eq!(
            app.handle_text(TextKey::Char(key)),
            Effect::RunExecutable {
                path: root().join("tool")
            }
        );
        assert_eq!(app.mode, Mode::Normal);
    }
}

#[test]
fn enter_and_every_other_key_answer_no() {
    for key in [
        TextKey::Enter,
        TextKey::Esc,
        TextKey::Char('n'),
        TextKey::Char('N'),
        TextKey::Char('q'),
        TextKey::Backspace,
    ] {
        let mut app = asking();
        assert_eq!(app.handle_text(key), Effect::None, "{key:?}");
        assert_eq!(app.mode, Mode::Normal, "{key:?}");
        assert_eq!(app.status, Some(Message::Cancelled), "{key:?}");
        assert_eq!(app.run_target, None, "{key:?}");
    }
}

#[test]
fn enter_uses_the_set_application_and_v_never_does() {
    let mut app = app_on("a.PDF", EntryKind::File);
    app.open_extensions.insert("pdf".into());
    assert_eq!(
        app.handle(Command::Enter),
        Effect::OpenWithApp {
            path: root().join("a.PDF")
        }
    );
    assert!(matches!(
        app.handle(Command::View),
        Effect::OpenViewer { .. }
    ));

    // Other extensions keep the viewer.
    let mut app = app_on("a.txt", EntryKind::File);
    app.open_extensions.insert("pdf".into());
    assert!(matches!(
        app.handle(Command::Enter),
        Effect::OpenViewer { .. }
    ));
}

#[test]
fn settings_accept_a_command_or_a_table() {
    let settings: Settings = toml::from_str(
        r#"
[open.by_extension]
PDF = "zathura --fork"
mp4 = { command = "mpv", wait = false }
txt = { command = "less" }
empty = ""
"#,
    )
    .unwrap();
    let apps = apps_from_settings(&settings.open.by_extension);
    assert_eq!(apps["pdf"].argv, ["zathura", "--fork"]);
    assert!(apps["pdf"].wait);
    assert!(!apps["mp4"].wait);
    assert!(apps["txt"].wait);
    assert!(!apps.contains_key("empty"));
}

#[test]
fn a_failure_reaches_the_status_bar() {
    let mut app = app_on("a.pdf", EntryKind::File);
    app.on_open_finished(Err("open: no application".into()));
    assert_eq!(
        app.status,
        Some(Message::CannotOpenExternal {
            reason: "open: no application".into()
        })
    );
}
