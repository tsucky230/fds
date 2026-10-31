//! App transitions for Phase 3 (P3-1, P3-2, P3-9, P3-12 to P3-16, §45).

use std::path::PathBuf;

use fds::app::{App, Effect, Mode};
use fds::app_ops::SimpleKind;
use fds::command::{Command, TextKey};
use fds::entry::{EntryKind, FileEntry};
use fds::message::Message;
use fds::ops::simple::SimpleError;
use fds::ops::{JobEvent, Op, Summary};

fn work() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
}

fn app_with(names: &[(&str, EntryKind)]) -> App {
    let w = work();
    let (mut app, effect) = App::new(w.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    let listing = names
        .iter()
        .map(|(n, k)| FileEntry::new(w.join(n), (*n).into(), *k, false))
        .collect();
    app.on_listing(id, Ok(listing));
    app
}

fn files() -> App {
    app_with(&[
        ("a.txt", EntryKind::File),
        ("b.txt", EntryKind::File),
        ("sub", EntryKind::Directory),
    ])
}

fn cursor_to(app: &mut App, name: &str) {
    // Go to the top of the list. While choosing a destination Home means "the here row", so
    // leave that row and walk up with Up instead.
    app.handle(Command::Home);
    if app.dest_on_here {
        app.handle(Command::CursorDown);
        while app.pane.cursor > 0 {
            app.handle(Command::CursorUp);
        }
    }
    while app.pane.cursor_entry().map(|e| e.display_name.as_str()) != Some(name) {
        let before = app.pane.cursor;
        app.handle(Command::CursorDown);
        assert_ne!(before, app.pane.cursor, "{name} not found");
    }
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_text(TextKey::Char(c));
    }
}

// ---------- Copy / Move destination (P3-2, §45 I3, I8) ----------

#[test]
fn copy_selects_destination_then_starts_the_job() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Copy);
    assert_eq!(app.mode, Mode::SelectDestination);
    // Navigate into sub without opening a viewer.
    cursor_to(&mut app, "sub");
    let Effect::Load { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_listing(id, Ok(vec![]));
    assert_eq!(
        app.mode,
        Mode::SelectDestination,
        "still selecting after navigating"
    );
    let effect = app.handle(Command::Copy);
    let Effect::StartJob(job) = effect else {
        panic!("{effect:?}")
    };
    assert_eq!(job.op, Op::Copy);
    assert_eq!(job.sources, [work().join("a.txt")]);
    assert_eq!(job.dest, Some(work().join("sub")));
    assert_eq!(app.mode, Mode::Running);
}

#[test]
fn left_arrow_goes_up_while_choosing_a_destination() {
    // The guide on a folder row says "Left/h: up"; Left must do that, on any row.
    for leave_here in [false, true] {
        let mut app = files();
        cursor_to(&mut app, "a.txt");
        app.handle(Command::Copy);
        if leave_here {
            app.handle(Command::CursorDown);
        }
        let effect = app.handle(Command::ScrollLeft);
        assert!(
            matches!(&effect, Effect::Load { dir, .. } if Some(dir.as_path()) == work().parent()),
            "{effect:?}"
        );
        assert_eq!(app.mode, Mode::SelectDestination);
    }
}

#[test]
fn destination_inside_a_source_is_refused() {
    let mut app = app_with(&[("sub", EntryKind::Directory)]);
    cursor_to(&mut app, "sub");
    app.handle(Command::Copy); // source = /work/sub
    app.handle(Command::CursorDown); // leave the "here" row; the cursor is on sub
    let Effect::Load { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_listing(id, Ok(vec![]));
    assert_eq!(app.pane.current_dir, work().join("sub"));
    let effect = app.handle(Command::Copy); // dest = /work/sub, inside the source
    assert_eq!(effect, Effect::None);
    assert_eq!(
        app.status,
        Some(Message::InsideSource { name: "sub".into() })
    );
    assert_eq!(app.mode, Mode::SelectDestination);
}

#[test]
fn other_operations_are_blocked_while_selecting() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Copy);
    for cmd in [
        Command::Delete,
        Command::Rename,
        Command::MakeDir,
        Command::Move,
    ] {
        assert_eq!(app.handle(cmd), Effect::None, "{cmd:?}");
        assert_eq!(app.mode, Mode::SelectDestination);
        assert_eq!(app.status, Some(Message::FinishSelectionFirst));
    }
    app.handle(Command::Back);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::Cancelled));
}

#[test]
fn nothing_selected_reports_and_does_nothing() {
    let mut app = files();
    app.handle(Command::Home); // [..]
    for cmd in [Command::Copy, Command::Move, Command::Delete] {
        assert_eq!(app.handle(cmd), Effect::None, "{cmd:?}");
        assert_eq!(app.status, Some(Message::NothingSelected));
        assert_eq!(app.mode, Mode::Normal);
    }
}

// ---------- Delete confirmation (P3-9, §45 I1) ----------

#[test]
fn y_deletes_permanently_by_default_and_trashes_in_trash_mode() {
    // R-402-2 default, R-402-3 setting (M1-a).
    use fds::ops::DeleteMode;
    for (mode, op) in [
        (None, Op::Delete),
        (Some(DeleteMode::Permanent), Op::Delete),
        (Some(DeleteMode::Trash), Op::Trash),
    ] {
        let mut app = files();
        if let Some(mode) = mode {
            app.delete_mode = mode;
        }
        cursor_to(&mut app, "a.txt");
        app.handle(Command::Delete);
        assert_eq!(app.mode, Mode::ConfirmDelete);
        let effect = app.handle_text(TextKey::Char('y'));
        let Effect::StartJob(job) = effect else {
            panic!("{mode:?}: {effect:?}")
        };
        assert_eq!(job.op, op, "{mode:?}");
        assert_eq!(job.sources, [work().join("a.txt")]);
        assert_eq!(job.dest, None);
    }
}

#[test]
fn delete_needs_confirmation() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Delete);
    assert_eq!(app.mode, Mode::ConfirmDelete);
    assert!(app.wants_text());
    // 'n' cancels, nothing happens.
    assert_eq!(app.handle_text(TextKey::Char('n')), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::Cancelled));

    app.handle(Command::Delete);
    let effect = app.handle_text(TextKey::Char('y'));
    let Effect::StartJob(job) = effect else {
        panic!("{effect:?}")
    };
    assert_eq!(job.op, Op::Delete);
    assert_eq!(job.sources, [work().join("a.txt")]);
    assert_eq!(app.mode, Mode::Running);
}

// ---------- Rename / mkdir (P3-12, P3-13) ----------

#[test]
fn rename_prefills_and_emits_effect() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Rename);
    assert_eq!(app.mode, Mode::Input);
    // Replace the stem, keep ".txt".
    app.handle_text(TextKey::Backspace); // deletes 'a' (cursor is before ".txt")
    typed(&mut app, "renamed");
    let effect = app.handle_text(TextKey::Enter);
    assert_eq!(
        effect,
        Effect::Rename {
            dir: work(),
            old: "a.txt".into(),
            new: "renamed.txt".into()
        }
    );
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn mkdir_from_empty_input() {
    let mut app = files();
    app.handle(Command::MakeDir);
    assert_eq!(app.mode, Mode::Input);
    typed(&mut app, "newdir");
    let effect = app.handle_text(TextKey::Enter);
    assert_eq!(
        effect,
        Effect::MakeDir {
            dir: work(),
            name: "newdir".into()
        }
    );
}

#[test]
fn simple_result_reloads_or_reports() {
    let mut app = files();
    let effect = app.on_simple_result(SimpleKind::Mkdir, Ok(work().join("newdir")));
    assert!(matches!(effect, Effect::Load { .. }), "reloads");

    let mut app = files();
    let effect = app.on_simple_result(
        SimpleKind::Rename {
            old: "a.txt".into(),
        },
        Err(SimpleError::Exists(work().join("b.txt"))),
    );
    assert_eq!(effect, Effect::None);
    assert!(matches!(app.status, Some(Message::OperationFailed { .. })));
}

// ---------- search (P3-14) ----------

#[test]
fn search_moves_the_cursor_and_esc_restores() {
    let mut app = app_with(&[
        ("apple", EntryKind::File),
        ("banana", EntryKind::File),
        ("cherry", EntryKind::File),
    ]);
    app.handle(Command::Home); // [..], index 0
    let start = app.pane.cursor;
    app.handle(Command::Search);
    assert_eq!(app.mode, Mode::Input);
    typed(&mut app, "ban");
    assert_eq!(app.pane.cursor_entry().unwrap().display_name, "banana");
    app.handle_text(TextKey::Esc);
    assert_eq!(app.pane.cursor, start, "Esc restores the original cursor");
    assert_eq!(app.mode, Mode::Normal);

    app.handle(Command::Search);
    typed(&mut app, "err"); // matches cherry
    app.handle_text(TextKey::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.pane.cursor_entry().unwrap().display_name, "cherry");
}

// ---------- job events and history (P3-6, P3-11, P3-16) ----------

/// Starts a Copy job by selecting `sub` as the destination.
fn start_copy(app: &mut App) -> u64 {
    cursor_to(app, "a.txt");
    app.handle(Command::Copy);
    cursor_to(app, "sub");
    let Effect::Load { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_listing(id, Ok(vec![]));
    let Effect::StartJob(job) = app.handle(Command::Copy) else {
        panic!()
    };
    job.id
}

#[test]
fn job_progress_conflict_and_finish() {
    let mut app = files();
    let id = start_copy(&mut app);
    app.on_job_event(id, JobEvent::Scanned { files: 1, bytes: 5 });
    app.on_job_event(
        id,
        JobEvent::Progress {
            done_files: 0,
            done_bytes: 2,
            current: work().join("a.txt"),
        },
    );
    // A conflict pauses in Mode::Conflict.
    app.on_job_event(
        id,
        JobEvent::Conflict {
            source: work().join("a.txt"),
            dest: work().join("sub/a.txt"),
            dest_is_dir: false,
        },
    );
    assert_eq!(app.mode, Mode::Conflict);
    let effect = app.handle_text(TextKey::Char('o'));
    assert!(matches!(effect, Effect::AnswerConflict { .. }));
    assert_eq!(app.mode, Mode::Running);

    // Stale event from another job is ignored.
    assert_eq!(
        app.on_job_event(
            id + 99,
            JobEvent::Progress {
                done_files: 9,
                done_bytes: 9,
                current: work()
            }
        ),
        Effect::None
    );

    let summary = Summary {
        done: 1,
        skipped: 0,
        failed: vec![],
        cancelled: false,
    };
    let effect = app.on_job_event(id, JobEvent::Finished(summary));
    assert!(
        matches!(effect, Effect::Load { .. }),
        "reloads after finishing"
    );
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.history.len(), 1);
    assert!(matches!(app.status, Some(Message::JobDone { .. })));
}

#[test]
fn failures_open_the_error_list() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Delete);
    let Effect::StartJob(job) = app.handle_text(TextKey::Char('y')) else {
        panic!()
    };
    app.on_job_event(
        job.id,
        JobEvent::EntryFailed {
            path: work().join("a.txt"),
            reason: "denied".into(),
        },
    );
    let summary = Summary {
        done: 0,
        skipped: 0,
        failed: vec![(work().join("a.txt"), "denied".into())],
        cancelled: false,
    };
    app.on_job_event(job.id, JobEvent::Finished(summary));
    assert_eq!(app.mode, Mode::ErrorList);
    assert_eq!(app.failures.len(), 1);
    app.handle(Command::CursorDown); // any key closes
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.failures.is_empty());
}

#[cfg(unix)]
#[test]
fn rename_refuses_names_that_are_not_valid_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let w = work();
    let (mut app, effect) = App::new(w.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    let raw = std::ffi::OsString::from_vec(b"bad\xffname".to_vec());
    app.on_listing(
        id,
        Ok(vec![FileEntry::new(
            w.join(&raw),
            raw.clone(),
            EntryKind::File,
            false,
        )]),
    );
    app.handle(Command::Rename);
    assert_eq!(
        app.mode,
        Mode::Normal,
        "no editor for a name we cannot round-trip"
    );
    assert!(matches!(app.status, Some(Message::OperationFailed { .. })));
}

// ---------- destination selection UX (M23 follow-up) ----------

#[test]
fn selection_starts_on_the_here_row_and_enter_runs_it() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Copy);
    assert!(app.dest_on_here, "the first row is 'copy here'");
    // Enter on the 'here' row copies into the current folder (same folder -> auto renamed copy).
    let Effect::StartJob(job) = app.handle(Command::Enter) else {
        panic!()
    };
    assert_eq!(job.op, Op::Copy);
    assert_eq!(job.dest, Some(work()));
}

#[test]
fn down_leaves_the_here_row_and_enter_enters_a_folder() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Copy);
    app.handle(Command::CursorDown);
    assert!(!app.dest_on_here);
    cursor_to_keep_mode(&mut app, "sub");
    let Effect::Load { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_listing(id, Ok(vec![]));
    assert_eq!(app.mode, Mode::SelectDestination);
    assert!(
        app.dest_on_here,
        "a new folder puts the cursor on 'here' again"
    );
    // Up from the first row goes back to 'here'.
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Copy);
    app.handle(Command::CursorDown); // off the "here" row
    assert!(!app.dest_on_here);
    while app.pane.cursor > 0 {
        app.handle(Command::CursorUp);
    }
    app.handle(Command::CursorUp);
    assert!(
        app.dest_on_here,
        "Up from the top of the list returns to the here row"
    );
}

/// Moves the cursor to `name` without leaving destination-selection mode: up to the top first,
/// then down (the entry may be above the current cursor).
fn cursor_to_keep_mode(app: &mut App, name: &str) {
    while app.pane.cursor > 0 {
        app.handle(Command::CursorUp);
    }
    let mut guard = 0;
    while app.pane.cursor_entry().map(|e| e.display_name.as_str()) != Some(name) {
        app.handle(Command::CursorDown);
        guard += 1;
        assert!(guard < 50, "{name} not found");
    }
}

#[test]
fn move_into_the_same_folder_is_refused_with_a_message() {
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Move);
    assert_eq!(app.handle(Command::Enter), Effect::None);
    assert_eq!(app.status, Some(Message::SameFolder));
    assert_eq!(app.mode, Mode::SelectDestination);
}

#[test]
fn places_popup_opens_and_jumps() {
    use fds::ops::place::{Place, PlaceKind};
    let mut app = files();
    let Effect::ListPlaces { id } = app.handle(Command::Places) else {
        panic!()
    };
    let list = vec![
        Place {
            kind: PlaceKind::Home,
            label: "Home".into(),
            path: PathBuf::from(if cfg!(windows) {
                r"C:\Users\me"
            } else {
                "/home/me"
            }),
        },
        Place {
            kind: PlaceKind::Root,
            label: "/".into(),
            path: PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" }),
        },
    ];
    app.on_places(id, list);
    assert_eq!(app.mode, Mode::Places);
    app.handle(Command::CursorDown);
    let effect = app.handle(Command::Enter);
    assert!(matches!(effect, Effect::Load { .. }), "{effect:?}");
    assert_eq!(app.mode, Mode::Normal);

    // Esc closes without going anywhere.
    let mut app = files();
    let Effect::ListPlaces { id } = app.handle(Command::Places) else {
        panic!()
    };
    app.on_places(id, vec![]);
    assert_eq!(app.mode, Mode::Places);
    assert_eq!(app.handle(Command::Back), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn places_inside_selection_returns_to_selection_and_offers_the_origin() {
    use fds::ops::place::{Place, PlaceKind};
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::Copy);
    let Effect::ListPlaces { id } = app.handle(Command::Places) else {
        panic!()
    };
    app.on_places(
        id,
        vec![Place {
            kind: PlaceKind::Root,
            label: "/".into(),
            path: PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" }),
        }],
    );
    assert_eq!(app.mode, Mode::Places);
    assert_eq!(
        app.places[0].kind,
        PlaceKind::Origin,
        "origin folder is the first choice"
    );
    assert_eq!(app.places[0].path, work());
    let effect = app.handle(Command::Enter); // go to the origin folder
    assert!(matches!(effect, Effect::Load { .. }));
    assert_eq!(
        app.mode,
        Mode::SelectDestination,
        "still choosing a destination"
    );
    assert!(app.pending_op.is_some());
}

#[test]
fn selected_items_show_their_actions() {
    // The status line reminds what can be done once something is marked.
    let mut app = files();
    cursor_to(&mut app, "a.txt");
    app.handle(Command::ToggleMark);
    assert_eq!(app.pane.marked_entries().len(), 1);
}
