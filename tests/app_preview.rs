//! When the preview pane asks for a load: `App::want_preview` / `on_preview` / `stale_preview` /
//! `preview_view` (R-200-9; no filesystem here).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use fds::app::{App, Effect};
use fds::app_preview::{PREVIEW_ABANDON, PreviewView};
use fds::command::{Command, TextKey};
use fds::entry::{EntryKind, FileEntry};
use fds::preview::{PreviewDoc, PreviewError};

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
            FileEntry::new(w.join("sub"), "sub".into(), EntryKind::Directory, false),
        ]),
    );
    // Folders sort first ([..], sub, a.txt, b.txt): start on a.txt.
    app.handle(Command::CursorDown);
    assert_eq!(app.pane.cursor_entry().unwrap().path, w.join("a.txt"));
    app
}

fn hex(byte: u8) -> PreviewDoc {
    PreviewDoc::Binary {
        file_len: u64::from(byte),
    }
}

#[test]
fn the_preview_is_on_by_default() {
    assert!(app().show_preview);
}

#[test]
fn asks_for_the_item_under_the_cursor_once() {
    let mut app = app();
    let now = Instant::now();
    let req = app.want_preview(now).expect("first request");
    assert_eq!(req.path, work().join("a.txt"));
    assert_eq!(req.show_hidden, app.pane.show_hidden, "follows the list");
    assert!(
        app.want_preview(now).is_none(),
        "in flight: not asked twice"
    );
    app.on_preview(req.id, Ok(hex(1)));
    assert!(app.want_preview(now).is_none(), "loaded: not asked again");
}

#[test]
fn an_answer_is_shown_only_for_the_item_it_belongs_to() {
    let mut app = app();
    let now = Instant::now();
    assert_eq!(app.preview_view(), PreviewView::Loading);
    let req = app.want_preview(now).unwrap();
    assert_eq!(app.preview_view(), PreviewView::Loading);
    app.on_preview(req.id, Ok(hex(1)));
    assert_eq!(app.preview_view(), PreviewView::Ready(&hex(1)));
    app.handle(Command::CursorDown);
    assert_eq!(
        app.preview_view(),
        PreviewView::Loading,
        "a.txt's content must not appear under b.txt"
    );
    app.handle(Command::CursorUp);
    assert_eq!(app.preview_view(), PreviewView::Ready(&hex(1)));
}

#[test]
fn errors_are_shown_for_their_item() {
    let mut app = app();
    let req = app.want_preview(Instant::now()).unwrap();
    let error = PreviewError::Io("Permission denied".into());
    app.on_preview(req.id, Err(error.clone()));
    assert_eq!(app.preview_view(), PreviewView::Failed(&error));
}

#[test]
fn one_load_at_a_time_and_the_latest_cursor_wins() {
    let mut app = app();
    let t0 = Instant::now();
    let first = app.want_preview(t0).unwrap();
    app.handle(Command::CursorDown); // b.txt
    app.handle(Command::CursorUp); // a.txt
    app.handle(Command::CursorUp); // sub
    let later = t0 + Duration::from_millis(100);
    assert!(app.want_preview(later).is_none(), "a.txt is still loading");
    assert_eq!(
        app.stale_preview(),
        Some(first.id),
        "a.txt is no longer wanted"
    );
    app.on_preview(first.id, Ok(hex(1)));
    assert_eq!(app.stale_preview(), None);
    let next = app.want_preview(later).expect("the cursor item now");
    assert_eq!(next.path, work().join("sub"));
    assert_ne!(next.id, first.id);
}

#[test]
fn a_wanted_load_is_not_stale() {
    let mut app = app();
    let req = app.want_preview(Instant::now()).unwrap();
    assert_eq!(app.stale_preview(), None);
    app.on_preview(req.id, Ok(hex(1)));
    assert_eq!(app.stale_preview(), None, "nothing in flight");
}

#[test]
fn a_stuck_load_is_given_up_after_the_abandon_time() {
    let one_ms = Duration::from_millis(1);
    // (elapsed since the stuck load started, a new request is made)
    for (elapsed, asks) in [
        (Duration::ZERO, false),
        (one_ms, false),
        (PREVIEW_ABANDON - one_ms, false),
        (PREVIEW_ABANDON, true),
        (PREVIEW_ABANDON + one_ms, true),
        (PREVIEW_ABANDON * 10, true),
    ] {
        let mut app = app();
        let t0 = Instant::now();
        let stuck = app.want_preview(t0).unwrap();
        app.handle(Command::CursorDown);
        let got = app.want_preview(t0 + elapsed);
        assert_eq!(got.is_some(), asks, "{elapsed:?}");
        if let Some(next) = got {
            assert_eq!(next.path, work().join("b.txt"));
            // The late answer of the given-up load is ignored.
            app.on_preview(stuck.id, Ok(hex(1)));
            assert_eq!(app.preview_view(), PreviewView::Loading, "{elapsed:?}");
            app.on_preview(next.id, Ok(hex(2)));
            assert_eq!(app.preview_view(), PreviewView::Ready(&hex(2)));
        }
    }
}

#[test]
fn a_still_wanted_load_is_never_given_up() {
    let mut app = app();
    let t0 = Instant::now();
    app.want_preview(t0).unwrap();
    assert!(app.want_preview(t0 + PREVIEW_ABANDON * 10).is_none());
}

#[test]
fn a_cancelled_answer_is_asked_again_when_the_item_is_back() {
    let mut app = app();
    let t0 = Instant::now();
    let first = app.want_preview(t0).unwrap();
    app.handle(Command::CursorDown);
    assert_eq!(app.stale_preview(), Some(first.id));
    app.handle(Command::CursorUp); // back on a.txt before the answer came
    app.on_preview(first.id, Err(PreviewError::Cancelled));
    assert_eq!(app.preview_view(), PreviewView::Loading);
    let again = app.want_preview(t0).expect("not stuck on Loading");
    assert_eq!(again.path, work().join("a.txt"));
}

#[test]
fn unknown_ids_are_ignored() {
    let mut app = app();
    let req = app.want_preview(Instant::now()).unwrap();
    for id in [req.id + 1, req.id + 100, 0] {
        app.on_preview(id, Ok(hex(9)));
        assert_eq!(app.preview_view(), PreviewView::Loading, "{id}");
    }
}

#[test]
fn the_parent_link_has_no_preview_but_keeps_the_pane() {
    let mut app = app();
    let req = app.want_preview(Instant::now()).unwrap();
    app.on_preview(req.id, Ok(hex(1)));
    app.handle(Command::Home); // [..]
    assert!(app.want_preview(Instant::now()).is_none());
    assert_eq!(app.preview_view(), PreviewView::Empty);
}

#[test]
fn an_empty_folder_has_an_empty_pane() {
    let (mut app, effect) = App::new(work());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(id, Ok(Vec::new()));
    assert!(app.want_preview(Instant::now()).is_none());
    assert_eq!(app.preview_view(), PreviewView::Empty);
}

#[test]
fn toggle_preview_turns_it_off_and_on() {
    let mut app = app();
    let now = Instant::now();
    let req = app.want_preview(now).unwrap();
    app.handle(Command::TogglePreview);
    assert!(!app.show_preview);
    assert_eq!(app.preview_view(), PreviewView::Off);
    assert_eq!(
        app.stale_preview(),
        Some(req.id),
        "an unwanted load is cancelled"
    );
    app.on_preview(req.id, Err(PreviewError::Cancelled));
    assert!(app.want_preview(now).is_none(), "off: nothing is asked");
    app.handle(Command::TogglePreview);
    assert!(app.show_preview);
    assert!(app.want_preview(now).is_some(), "on again: asks");
}

#[test]
fn input_mode_keeps_previewing_and_other_modes_do_not_ask() {
    let mut app = app();
    let now = Instant::now();
    app.handle(Command::Search);
    for c in "sub".chars() {
        app.handle_text(TextKey::Char(c));
    }
    let req = app
        .want_preview(now)
        .expect("search moves the cursor; preview follows");
    assert_eq!(req.path, work().join("sub"));
    app.on_preview(req.id, Ok(hex(1)));
    app.handle_text(TextKey::Enter);

    app.handle(Command::CursorUp);
    app.handle(Command::Help);
    assert!(app.want_preview(now).is_none(), "Help: nothing is asked");
}

#[test]
fn a_new_listing_or_hidden_toggle_asks_again() {
    let mut app = app();
    let now = Instant::now();
    let req = app.want_preview(now).unwrap();
    app.on_preview(req.id, Ok(hex(1)));

    let Effect::Load { id, dir, .. } = app.handle(Command::Refresh) else {
        panic!()
    };
    let w = work();
    app.on_listing(
        id,
        Ok(vec![FileEntry::new(
            w.join("a.txt"),
            "a.txt".into(),
            EntryKind::File,
            false,
        )]),
    );
    assert_eq!(dir, w);
    let again = app.want_preview(now).expect("the file may have changed");
    assert_eq!(again.path, w.join("a.txt"));
    app.on_preview(again.id, Ok(hex(2)));

    app.handle(Command::ToggleHidden);
    let hidden = app
        .want_preview(now)
        .expect("a folder preview depends on it");
    assert_eq!(hidden.path, w.join("a.txt"));
    assert_ne!(hidden.show_hidden, again.show_hidden, "the toggled value");
    assert_eq!(hidden.show_hidden, app.pane.show_hidden);
}

#[test]
fn nothing_is_loaded_while_the_pane_is_not_on_screen() {
    let mut app = app();
    let now = Instant::now();
    app.preview_shown = false;
    assert!(
        app.want_preview(now).is_none(),
        "a hidden pane asks for nothing"
    );
    app.preview_shown = true;
    let req = app.want_preview(now).expect("on screen again: asks");
    assert_eq!(app.stale_preview(), None);
    app.preview_shown = false;
    assert_eq!(
        app.stale_preview(),
        Some(req.id),
        "the pane left the screen: cancel the load"
    );
}

#[test]
fn requests_carry_the_picture_setting() {
    let mut app = app();
    let now = Instant::now();
    assert!(app.preview_images, "pictures are on by default");
    let req = app.want_preview(now).unwrap();
    assert!(req.thumbnails);
    app.on_preview(req.id, Ok(hex(1)));
    app.preview_images = false;
    app.handle(Command::CursorDown);
    let req = app.want_preview(now).unwrap();
    assert!(!req.thumbnails);
}

#[test]
fn ctrl_p_on_the_list_switches_the_image_mode_only_for_a_picture() {
    use fds::preview::Thumbnail;
    use fds::viewer::detect::ImageFormat;
    let picture = PreviewDoc::Picture {
        format: ImageFormat::Png,
        dimensions: (2, 2),
        file_len: 10,
        thumbnail: Thumbnail(std::sync::Arc::new(image::DynamicImage::new_rgb8(2, 2))),
    };
    // (preview of the cursor item, effect of Ctrl+P)
    for (doc, effect) in [
        (Some(picture), Effect::CycleImageProtocol),
        (Some(hex(1)), Effect::None),
        (None, Effect::None),
    ] {
        let mut app = app();
        let req = app.want_preview(Instant::now()).unwrap();
        if let Some(doc) = doc.clone() {
            app.on_preview(req.id, Ok(doc));
        }
        assert_eq!(app.handle(Command::NextImageProtocol), effect, "{doc:?}");
    }
}
