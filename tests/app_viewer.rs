//! Viewer transitions in App (P2-1, P2-2, P2-3, P2-9, P2-13).

use std::path::{Path, PathBuf};

use fds::app::{App, Effect, Mode};
use fds::command::Command;
use fds::entry::{EntryKind, FileEntry, LinkTarget};
use fds::message::Message;
use fds::viewer::styled::StyledLine;
use fds::viewer::{Document, Opened, TextDoc, ViewError, ViewerKind};

fn work() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
}

fn entry(dir: &Path, name: &str, kind: EntryKind) -> FileEntry {
    FileEntry::new(dir.join(name), name.into(), kind, false)
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
            entry(&w, "sub", EntryKind::Directory),
            entry(&w, "a.txt", EntryKind::File),
            entry(&w, "link.txt", EntryKind::Symlink(LinkTarget::File)),
            entry(&w, "dead", EntryKind::Symlink(LinkTarget::Broken)),
            entry(&w, "fifo", EntryKind::Other),
        ]),
    );
    app
}

fn cursor_to(app: &mut App, name: &str) {
    app.handle(Command::Home);
    while app.pane.cursor_entry().map(|e| e.display_name.as_str()) != Some(name) {
        let before = app.pane.cursor;
        app.handle(Command::CursorDown);
        assert_ne!(before, app.pane.cursor, "{name} not found");
    }
}

fn opened(path: PathBuf, kind: ViewerKind) -> Opened {
    Opened {
        path,
        candidates: vec![ViewerKind::Text, ViewerKind::Hex],
        kind,
        document: Document::Lines(TextDoc {
            lines: (0..100)
                .map(|i| StyledLine::plain(format!("l{i}")))
                .collect(),
            encoding: "UTF-8".into(),
            truncated: false,
            file_len: 0,
        }),
        encoding: None,
        fallback: None,
    }
}

/// Opens a.txt and completes the load.
fn open_a(app: &mut App) -> PathBuf {
    cursor_to(app, "a.txt");
    let Effect::OpenViewer { id, path, kind, .. } = app.handle(Command::View) else {
        panic!()
    };
    assert_eq!(kind, None);
    app.on_viewer(id, Ok(opened(path.clone(), ViewerKind::Text)));
    path
}

#[test]
fn f3_and_enter_open_files_and_file_symlinks() {
    let mut app = app();
    for (name, cmd) in [
        ("a.txt", Command::View),
        ("link.txt", Command::View),
        ("a.txt", Command::Enter),
    ] {
        cursor_to(&mut app, name);
        let effect = app.handle(cmd);
        assert!(
            matches!(&effect, Effect::OpenViewer { path, .. } if *path == work().join(name)),
            "{name}: {effect:?}"
        );
        assert_eq!(app.mode, Mode::ViewerLoading);
        app.handle(Command::Back);
        assert_eq!(app.mode, Mode::Normal);
    }
}

#[test]
fn non_files_are_not_opened() {
    let mut app = app();
    cursor_to(&mut app, "sub");
    assert_eq!(app.handle(Command::View), Effect::None);
    assert_eq!(app.status, Some(Message::NotAFile { name: "sub".into() }));
    cursor_to(&mut app, "fifo");
    assert_eq!(app.handle(Command::View), Effect::None);
    assert_eq!(
        app.status,
        Some(Message::NotAFile {
            name: "fifo".into()
        })
    );
    cursor_to(&mut app, "dead");
    assert_eq!(app.handle(Command::View), Effect::None);
    assert_eq!(
        app.status,
        Some(Message::BrokenSymlink {
            name: "dead".into()
        })
    );
    app.handle(Command::Home); // [..]
    assert_eq!(app.handle(Command::View), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn viewer_opens_and_every_close_key_restores_the_list() {
    for close in [Command::Quit, Command::Back, Command::View] {
        let mut app = app();
        cursor_to(&mut app, "a.txt");
        app.handle(Command::ToggleMark);
        app.handle(Command::ToggleHidden);
        let cursor = app.pane.cursor;
        open_a(&mut app);
        assert_eq!(app.mode, Mode::Viewer);
        app.handle(Command::CursorDown);
        app.handle(Command::End);
        assert_eq!(app.handle(close), Effect::None, "{close:?}");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.viewer.is_none());
        assert_eq!(app.pane.cursor, cursor);
        assert_eq!(app.pane.marked_entries().len(), 1);
        assert!(!app.pane.show_hidden);
    }
}

#[test]
fn ctrl_c_quits_from_the_viewer_and_list() {
    let mut app = app();
    open_a(&mut app);
    assert_eq!(app.handle(Command::ForceQuit), Effect::Quit);
    let mut app = self::app();
    assert_eq!(app.handle(Command::ForceQuit), Effect::Quit);
}

#[test]
fn scroll_commands_move_the_viewer() {
    let mut app = app();
    open_a(&mut app);
    app.viewer.as_mut().unwrap().set_viewport(20, 10);
    app.handle(Command::CursorDown);
    assert_eq!(app.viewer.as_ref().unwrap().top_line, 1);
    app.handle(Command::PageDown);
    assert_eq!(app.viewer.as_ref().unwrap().top_line, 11);
    app.handle(Command::End);
    assert_eq!(app.viewer.as_ref().unwrap().top_line, 90);
    app.handle(Command::Home);
    assert_eq!(app.viewer.as_ref().unwrap().top_line, 0);
    app.handle(Command::ToggleWrap);
    assert!(!app.viewer.as_ref().unwrap().wrap);
}

#[test]
fn tab_reopens_with_the_next_viewer() {
    let mut app = app();
    let path = open_a(&mut app);
    let Effect::OpenViewer {
        id, path: p, kind, ..
    } = app.handle(Command::NextViewer)
    else {
        panic!()
    };
    assert_eq!(p, path);
    assert_eq!(kind, Some(ViewerKind::Hex));
    assert_eq!(app.mode, Mode::ViewerLoading);
    // Cancel keeps the current viewer.
    app.handle(Command::Back);
    assert_eq!(app.mode, Mode::Viewer);
    app.on_viewer(id, Ok(opened(path.clone(), ViewerKind::Hex)));
    assert_eq!(
        app.viewer.as_ref().unwrap().kind,
        ViewerKind::Text,
        "cancelled result is ignored"
    );
}

#[test]
fn the_open_in_flight_is_known_until_it_ends() {
    // run cancels an external provider whose open is no longer the pending one (C30).
    let mut app = app();
    assert_eq!(app.pending_view_id(), None);
    cursor_to(&mut app, "a.txt");
    let Effect::OpenViewer { id, path, .. } = app.handle(Command::View) else {
        panic!()
    };
    assert_eq!(app.pending_view_id(), Some(id));
    app.handle(Command::Back);
    assert_eq!(app.pending_view_id(), None, "q / Esc while loading");
    let Effect::OpenViewer { id, .. } = app.handle(Command::View) else {
        panic!()
    };
    assert_eq!(app.pending_view_id(), Some(id));
    app.on_viewer(id, Ok(opened(path, ViewerKind::Text)));
    assert_eq!(app.pending_view_id(), None, "finished");
    let Effect::OpenViewer { id: tab_id, .. } = app.handle(Command::NextViewer) else {
        panic!()
    };
    assert_eq!(app.pending_view_id(), Some(tab_id));
}

#[test]
fn loading_can_be_cancelled_and_stale_results_are_ignored() {
    let mut app = app();
    cursor_to(&mut app, "a.txt");
    let Effect::OpenViewer { id, path, .. } = app.handle(Command::View) else {
        panic!()
    };
    // Other commands are ignored while loading.
    assert_eq!(app.handle(Command::CursorDown), Effect::None);
    assert_eq!(
        app.handle(Command::Quit),
        Effect::None,
        "q cancels instead of quitting"
    );
    assert_eq!(app.mode, Mode::Normal);
    app.on_viewer(id, Ok(opened(path, ViewerKind::Text)));
    assert!(app.viewer.is_none());
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn open_errors_go_to_the_status_line() {
    let mut app = app();
    cursor_to(&mut app, "a.txt");
    let Effect::OpenViewer { id, .. } = app.handle(Command::View) else {
        panic!()
    };
    app.on_viewer(id, Err(ViewError::Io("denied".into())));
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.status,
        Some(Message::CannotView {
            name: "a.txt".into(),
            reason: "denied".into()
        })
    );
}

#[test]
fn viewer_help_returns_to_the_viewer() {
    let mut app = app();
    open_a(&mut app);
    app.handle(Command::Help);
    assert_eq!(app.mode, Mode::ViewerHelp);
    app.handle(Command::Quit);
    assert_eq!(app.mode, Mode::Viewer);
}

/// M17 follow-up: `p` cycles the image protocol, only while an image is shown.
#[test]
fn p_cycles_the_image_protocol_only_for_images() {
    use fds::viewer::image::ImageDoc;
    let mut app = app();
    open_a(&mut app);
    assert_eq!(
        app.handle(Command::NextImageProtocol),
        Effect::None,
        "text viewer ignores p"
    );

    cursor_to(&mut app, "a.txt");
    app.handle(Command::Back);
    let Effect::OpenViewer { id, path, .. } = app.handle(Command::View) else {
        panic!()
    };
    app.on_viewer(
        id,
        Ok(Opened {
            path,
            candidates: vec![ViewerKind::Image, ViewerKind::Hex],
            kind: ViewerKind::Image,
            document: Document::Image(ImageDoc {
                format: None,
                dimensions: None,
                file_len: 0,
                image: None,
                problem: None,
            }),
            encoding: None,
            fallback: None,
        }),
    );
    assert_eq!(app.mode, Mode::Viewer);
    assert_eq!(
        app.handle(Command::NextImageProtocol),
        Effect::CycleImageProtocol
    );
    // In the list, Ctrl+P does nothing without a picture preview.
    app.handle(Command::Back);
    assert_eq!(app.handle(Command::NextImageProtocol), Effect::None);
}

// ---------- manual encoding switch (E, M1-b) ----------

fn with_lines(path: PathBuf, n: usize, encoding: Option<&'static encoding_rs::Encoding>) -> Opened {
    let mut o = opened(path, ViewerKind::Text);
    if let Document::Lines(doc) = &mut o.document {
        doc.lines.truncate(n);
    }
    o.encoding = encoding;
    o
}

#[test]
fn e_reopens_with_the_next_encoding_and_keeps_the_place() {
    let mut app = app();
    let path = open_a(&mut app);
    app.viewer.as_mut().unwrap().set_viewport(20, 10);
    for _ in 0..30 {
        app.handle(Command::CursorDown);
    }
    let top = app.viewer.as_ref().unwrap().top_line;
    assert!(top > 0);
    let Effect::OpenViewer {
        id,
        path: p,
        kind,
        encoding,
    } = app.handle(Command::NextEncoding)
    else {
        panic!()
    };
    assert_eq!(p, path);
    assert_eq!(
        kind,
        Some(ViewerKind::Text),
        "same viewer, only the encoding changes"
    );
    assert_eq!(encoding, Some(encoding_rs::UTF_8));
    assert_eq!(app.mode, Mode::ViewerLoading);
    app.on_viewer(
        id,
        Ok(with_lines(path.clone(), 100, Some(encoding_rs::UTF_8))),
    );
    let viewer = app.viewer.as_ref().unwrap();
    assert_eq!(viewer.encoding, Some(encoding_rs::UTF_8));
    assert_eq!(viewer.top_line, top, "the place in the text is kept");

    let Effect::OpenViewer { encoding, .. } = app.handle(Command::NextEncoding) else {
        panic!()
    };
    assert_eq!(encoding, Some(encoding_rs::SHIFT_JIS));
}

#[test]
fn the_place_is_clamped_when_the_new_text_is_shorter() {
    let mut app = app();
    let path = open_a(&mut app);
    app.viewer.as_mut().unwrap().set_viewport(20, 10);
    app.handle(Command::End);
    assert!(app.viewer.as_ref().unwrap().top_line >= 80);
    let Effect::OpenViewer { id, .. } = app.handle(Command::NextEncoding) else {
        panic!()
    };
    app.on_viewer(id, Ok(with_lines(path, 5, Some(encoding_rs::UTF_8))));
    assert!(app.viewer.as_ref().unwrap().top_line < 5);
}

#[test]
fn e_is_ignored_in_hex_and_in_the_list() {
    let mut app = app();
    assert_eq!(app.handle(Command::NextEncoding), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
    let path = open_a(&mut app);
    let Effect::OpenViewer { id, .. } = app.handle(Command::NextViewer) else {
        panic!()
    };
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("h.bin");
    std::fs::write(&file, b"abc").unwrap();
    let mut hex = opened(path, ViewerKind::Hex);
    hex.document = Document::Hex(fds::viewer::hex::HexSource::open(&file, 3).unwrap());
    app.on_viewer(id, Ok(hex));
    assert_eq!(app.viewer.as_ref().unwrap().kind, ViewerKind::Hex);
    assert_eq!(app.handle(Command::NextEncoding), Effect::None);
    assert_eq!(app.mode, Mode::Viewer);
}

#[test]
fn opening_a_file_again_detects_and_starts_at_the_top() {
    let mut app = app();
    let path = open_a(&mut app);
    app.viewer.as_mut().unwrap().set_viewport(20, 10);
    app.handle(Command::End);
    let Effect::OpenViewer { id, .. } = app.handle(Command::NextEncoding) else {
        panic!()
    };
    app.on_viewer(
        id,
        Ok(with_lines(path.clone(), 100, Some(encoding_rs::UTF_8))),
    );
    app.handle(Command::Back);
    assert_eq!(app.mode, Mode::Normal);
    let Effect::OpenViewer { id, encoding, .. } = app.handle(Command::View) else {
        panic!()
    };
    assert_eq!(encoding, None, "a new open detects the encoding again");
    app.on_viewer(id, Ok(with_lines(path, 100, None)));
    let viewer = app.viewer.as_ref().unwrap();
    assert_eq!((viewer.top_line, viewer.encoding), (0, None));
}

#[test]
fn a_fallback_is_reported_in_the_status_line() {
    let mut app = app();
    cursor_to(&mut app, "a.txt");
    let Effect::OpenViewer { id, path, .. } = app.handle(Command::View) else {
        panic!()
    };
    let mut o = opened(path, ViewerKind::Text);
    o.fallback = Some((
        ViewerKind::Custom("PDF text"),
        "provider timed out after 2 s".into(),
    ));
    app.on_viewer(id, Ok(o));
    assert_eq!(app.mode, Mode::Viewer);
    assert_eq!(
        app.status,
        Some(Message::ViewerFallback {
            failed: ViewerKind::Custom("PDF text"),
            reason: "provider timed out after 2 s".into(),
        })
    );
    let catalog = fds::i18n::Catalog::english();
    let text = app.status.as_ref().unwrap().render(&catalog);
    assert!(
        text.contains("PDF text") && text.contains("timed out"),
        "{text}"
    );
}

#[test]
fn a_message_in_the_viewer_goes_away_with_the_next_key() {
    // Otherwise it would hide the viewer's own status (encoding, line) until the viewer closes.
    let mut app = app();
    cursor_to(&mut app, "a.txt");
    let Effect::OpenViewer { id, path, .. } = app.handle(Command::View) else {
        panic!()
    };
    let mut o = opened(path, ViewerKind::Text);
    o.fallback = Some((
        ViewerKind::Custom("PDF text"),
        "provider failed (exit 1)".into(),
    ));
    app.on_viewer(id, Ok(o));
    assert!(app.status.is_some());
    app.handle(Command::CursorDown);
    assert_eq!(app.status, None);
    assert_eq!(app.mode, Mode::Viewer);
}

#[test]
fn h_and_backspace_close_the_viewer_back_to_the_same_list() {
    // In the list h / Backspace go up a level; in the viewer they go back to the list, so
    // "l opens, h goes back" works the same in both.
    use fds::command::{VIEWER_KEY_HELP, from_key};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    for key in [KeyCode::Char('h'), KeyCode::Backspace] {
        let cmd = from_key(KeyEvent::new(key, KeyModifiers::NONE)).unwrap();
        let mut app = app();
        cursor_to(&mut app, "a.txt");
        let dir = app.pane.current_dir.clone();
        let cursor = app.pane.cursor;
        open_a(&mut app);
        assert_eq!(app.handle(cmd), Effect::None, "{key:?}");
        assert_eq!(app.mode, Mode::Normal, "{key:?}");
        assert!(app.viewer.is_none(), "{key:?}");
        assert_eq!(app.pane.current_dir, dir, "{key:?}: still the same folder");
        assert_eq!(app.pane.cursor, cursor, "{key:?}");
    }
    // Left still scrolls sideways and l does nothing: neither closes the viewer.
    for cmd in [Command::ScrollLeft, Command::Enter] {
        let mut app = app();
        open_a(&mut app);
        app.handle(cmd);
        assert_eq!(app.mode, Mode::Viewer, "{cmd:?}");
    }
    assert!(
        VIEWER_KEY_HELP
            .iter()
            .any(|(keys, action)| *action == "vhelp.close" && keys.split(" / ").any(|k| k == "h")),
        "the viewer help lists h"
    );
}
