//! The folder list in the preview pane: Tab, the keys inside it and the target of the preview
//! (R-200-13; no filesystem here).

use std::path::PathBuf;
use std::time::Instant;

use fds::app::{App, Effect, Mode};
use fds::app_preview::PreviewView;
use fds::command::Command;
use fds::entry::{EntryKind, FileEntry};
use fds::i18n::Catalog;
use fds::message::Message;
use fds::preview::PreviewDoc;
use fds::ui::render;
use fds::ui_viewer::ImageSlot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

fn root() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
}

fn entry(dir: &std::path::Path, name: &str, kind: EntryKind) -> FileEntry {
    FileEntry::new(dir.join(name), name.into(), kind, false)
}

/// Cursor on the folder `docs`, a file after it.
fn app() -> App {
    let dir = root();
    let (mut app, effect) = App::new(dir.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![
            entry(&dir, "docs", EntryKind::Directory),
            entry(&dir, "a.txt", EntryKind::File),
        ]),
    );
    while app.pane.cursor_entry().unwrap().display_name != "docs" {
        app.handle(Command::CursorDown);
    }
    app
}

fn inner() -> PathBuf {
    root().join("docs")
}

/// Answers the folder preview of the cursor item.
fn load_folder(app: &mut App, names: &[(&str, EntryKind)]) {
    let req = app.want_preview(Instant::now()).expect("a request");
    assert_eq!(req.path, inner());
    let entries = names.iter().map(|(n, k)| entry(&inner(), n, *k)).collect();
    app.on_preview(
        req.id,
        Ok(PreviewDoc::Folder {
            entries,
            more: false,
        }),
    );
}

fn in_peek() -> App {
    let mut app = app();
    load_folder(
        &mut app,
        &[
            ("sub", EntryKind::Directory),
            ("x.txt", EntryKind::File),
            ("y.txt", EntryKind::File),
        ],
    );
    assert_eq!(app.handle(Command::NextViewer), Effect::None);
    assert_eq!(app.mode, Mode::PreviewBrowse);
    app
}

#[test]
fn tab_on_a_folder_moves_into_its_list() {
    let app = in_peek();
    let peek = app.peek.as_ref().unwrap();
    assert_eq!(peek.dir_name, "docs");
    assert_eq!(peek.entries.len(), 4);
    assert!(peek.entries[0].is_parent_link());
    // The cursor starts on the first item, so its content shows at once.
    assert_eq!(peek.cursor, 1);
}

#[test]
fn moving_in_the_list_changes_what_is_previewed() {
    let mut app = in_peek();
    let req = app.want_preview(Instant::now()).expect("the first item");
    assert_eq!(req.path, inner().join("sub"));
    app.on_preview(req.id, Ok(PreviewDoc::Binary { file_len: 0 }));
    app.handle(Command::CursorDown);
    assert!(app.stale_preview().is_none());
    let req = app.want_preview(Instant::now()).expect("the next item");
    assert_eq!(req.path, inner().join("x.txt"));
    // The cursor stops at both ends.
    app.handle(Command::End);
    app.handle(Command::CursorDown);
    assert_eq!(app.peek.as_ref().unwrap().cursor, 3);
    app.handle(Command::Home);
    app.handle(Command::CursorUp);
    assert_eq!(app.peek.as_ref().unwrap().cursor, 0);
}

#[test]
fn tab_and_esc_go_back_without_moving_the_left_cursor() {
    for key in [Command::NextViewer, Command::Back, Command::ParentDirectory] {
        let mut app = in_peek();
        let cursor = app.pane.cursor;
        app.handle(Command::CursorDown);
        assert_eq!(app.handle(key), Effect::None);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.peek.is_none());
        assert_eq!(app.pane.cursor, cursor);
        assert_eq!(app.pane.current_dir, root());
    }
}

#[test]
fn the_parent_link_goes_back_and_previews_nothing() {
    let mut app = in_peek();
    let cursor = app.pane.cursor;
    app.handle(Command::Home);
    assert_eq!(app.preview_view(), PreviewView::Empty);
    assert_eq!(app.want_preview(Instant::now()), None);
    // v has nothing to open.
    assert_eq!(app.handle(Command::View), Effect::None);
    assert_eq!(app.mode, Mode::PreviewBrowse);

    assert_eq!(app.handle(Command::Enter), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.peek.is_none());
    assert_eq!(app.pane.cursor, cursor);
    assert_eq!(app.pane.current_dir, root());
}

#[test]
fn enter_on_a_folder_moves_the_left_list_there() {
    let mut app = in_peek();
    let Effect::Load { dir, .. } = app.handle(Command::Enter) else {
        panic!("expected a listing load")
    };
    assert_eq!(dir, inner().join("sub"));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.peek.is_none());
}

#[test]
fn a_file_opens_in_the_viewer_and_closing_comes_back() {
    for key in [Command::Enter, Command::View] {
        let mut app = in_peek();
        app.handle(Command::CursorDown);
        let Effect::OpenViewer { path, .. } = app.handle(key) else {
            panic!("expected a viewer")
        };
        assert_eq!(path, inner().join("x.txt"));
        assert_eq!(app.mode, Mode::ViewerLoading);
        // Cancelled while loading.
        app.handle(Command::Back);
        assert_eq!(app.mode, Mode::PreviewBrowse);
        assert_eq!(app.peek.as_ref().unwrap().cursor, 2);
    }
}

#[test]
fn help_comes_back_to_the_list() {
    let mut app = in_peek();
    app.handle(Command::Help);
    assert_eq!(app.mode, Mode::Help);
    app.handle(Command::Back);
    assert_eq!(app.mode, Mode::PreviewBrowse);
}

#[test]
fn keys_that_change_files_do_nothing_in_the_list() {
    let mut app = in_peek();
    let marked_before = app.pane.entries.iter().filter(|e| e.marked).count();
    for key in [
        Command::ToggleMark,
        Command::Copy,
        Command::Move,
        Command::Delete,
        Command::Rename,
        Command::MakeDir,
        Command::Edit,
        Command::ArchiveCreate,
        Command::ArchiveExtract,
        Command::Search,
        Command::Places,
        Command::Refresh,
        Command::ToggleHidden,
        Command::TogglePreview,
    ] {
        assert_eq!(app.handle(key), Effect::None, "{key:?}");
        assert_eq!(app.mode, Mode::PreviewBrowse, "{key:?}");
    }
    assert_eq!(
        app.pane.entries.iter().filter(|e| e.marked).count(),
        marked_before
    );
    assert!(app.peek.as_ref().unwrap().entries.iter().all(|e| !e.marked));
    assert_eq!(app.handle(Command::Quit), Effect::None);
    assert_eq!(app.mode, Mode::ConfirmQuit);
    assert!(app.peek.is_none());
}

#[test]
fn tab_elsewhere_stays_in_the_list() {
    // On a file.
    let mut app = app();
    app.handle(Command::CursorDown);
    assert_eq!(app.pane.cursor_entry().unwrap().display_name, "a.txt");
    app.handle(Command::NextViewer);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, None);

    // On `[..]`.
    let mut app = self::app();
    app.handle(Command::Home);
    if app.pane.cursor_entry().unwrap().is_parent_link() {
        app.handle(Command::NextViewer);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status, None);
    }

    // The preview has not arrived.
    let mut app = self::app();
    app.handle(Command::NextViewer);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::PeekLoading));

    // An empty folder.
    let mut app = self::app();
    load_folder(&mut app, &[]);
    app.handle(Command::NextViewer);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::PeekEmpty));

    // No room, or no pane.
    let mut app = self::app();
    load_folder(&mut app, &[("x", EntryKind::File)]);
    app.peek_fits = false;
    app.handle(Command::NextViewer);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::PeekUnavailable));
    let mut app = self::app();
    load_folder(&mut app, &[("x", EntryKind::File)]);
    app.show_preview = false;
    app.handle(Command::NextViewer);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::PeekUnavailable));
}

fn draw(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let catalog = Catalog::english();
    terminal
        .draw(|f| render(f, app, &catalog, &mut ImageSlot::default()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn the_pane_splits_only_in_the_list() {
    let mut app = in_peek();
    let req = app.want_preview(Instant::now()).unwrap();
    app.on_preview(
        req.id,
        Ok(PreviewDoc::Folder {
            entries: vec![entry(&inner().join("sub"), "deep.txt", EntryKind::File)],
            more: false,
        }),
    );
    let buf = draw(&mut app, 100, 30);
    let rows = text(&buf);
    let row_of = |needle: &str| {
        rows.iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not in {rows:#?}"))
    };
    assert!(rows.iter().any(|r| r.contains("[..]")), "{rows:#?}");
    let (title, sub, content_title, deep) = (
        row_of(" docs "),
        row_of("[sub]/"),
        row_of(" sub "),
        row_of("deep.txt"),
    );
    assert!(
        title < sub && sub < content_title && content_title < deep,
        "{rows:#?}"
    );

    // The cursor is reversed on the right; on the left it is only underlined.
    let sub_x = (40..100)
        .find(|&x| buf[(x, sub as u16)].symbol() == "s")
        .unwrap();
    assert!(
        buf[(sub_x, sub as u16)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    let left = rows.iter().position(|r| r.contains("[docs]/")).unwrap();
    assert!(!buf[(3, left as u16)].modifier.contains(Modifier::REVERSED));
    assert!(
        buf[(3, left as u16)]
            .modifier
            .contains(Modifier::UNDERLINED)
    );

    // Back on the left the pane is one box again.
    app.handle(Command::Back);
    let rows = text(&draw(&mut app, 100, 30));
    assert!(
        !rows
            .iter()
            .any(|r| r.contains("[sub]/") && r.contains(" sub "))
    );
    assert_eq!(rows.iter().filter(|r| r.contains(" docs ")).count(), 1);
}

#[test]
fn a_screen_too_small_for_the_split_goes_back_to_the_list() {
    for (w, h) in [(100, 12), (60, 30), (30, 5)] {
        let mut app = in_peek();
        draw(&mut app, w, h);
        assert_eq!(app.mode, Mode::Normal, "{w}x{h}");
        assert!(app.peek.is_none(), "{w}x{h}");
    }
    let mut app = in_peek();
    for (w, h) in [(80, 24), (200, 60), (80, 16)] {
        draw(&mut app, w, h);
        assert_eq!(app.mode, Mode::PreviewBrowse, "{w}x{h}");
    }
}
