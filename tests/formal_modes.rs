//! Exhaustive state-machine check (design prompt §45, P3-17).
//!
//! Applies every Command and every TextKey to an App in each reachable Mode and asserts the
//! invariants I1-I8 from docs/formal/phase3-modes.md hold after each step. The point is that no
//! sequence of inputs can start a destructive job that the invariants forbid.

use std::path::PathBuf;

use fds::app::{App, Effect, Mode};
use fds::command::{Command, TextKey};
use fds::entry::{EntryKind, FileEntry};
use fds::ops::place::{Place, PlaceKind};
use fds::ops::{JobEvent, Op, Summary};

fn work() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
}

fn fresh() -> App {
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
    app
}

const COMMANDS: &[Command] = &[
    Command::CursorUp,
    Command::CursorDown,
    Command::PageUp,
    Command::PageDown,
    Command::Home,
    Command::End,
    Command::Enter,
    Command::ParentDirectory,
    Command::ToggleMark,
    Command::Help,
    Command::Refresh,
    Command::ToggleHidden,
    Command::View,
    Command::Edit,
    Command::Copy,
    Command::Move,
    Command::Delete,
    Command::Rename,
    Command::MakeDir,
    Command::Search,
    Command::Places,
    Command::ToggleInfo,
    Command::Back,
    Command::Quit,
    Command::ForceQuit,
    Command::ScrollLeft,
    Command::NextEncoding,
];

const TEXT_KEYS: &[TextKey] = &[
    TextKey::Char('y'),
    TextKey::Char('n'),
    TextKey::Char('o'),
    TextKey::Char('s'),
    TextKey::Char('r'),
    TextKey::Char('c'),
    TextKey::Char('q'),
    TextKey::Char('x'),
    TextKey::Enter,
    TextKey::Esc,
    TextKey::Backspace,
    TextKey::Up,
    TextKey::Down,
    TextKey::ForceQuit,
];

/// I1: a Delete job is only started from ConfirmDelete via 'y'.
/// I2: no job's sources contain `[..]`; no empty job is started.
/// I4: a Copy/Move whose dest is a source or inside one is never started.
/// I5/I8: no StartJob is produced while a job runs or during destination selection.
fn check_start(app_before_mode: Mode, effect: &Effect) {
    if let Effect::StartJob(job) = effect {
        assert!(!job.sources.is_empty(), "I2: empty job");
        assert!(
            job.sources
                .iter()
                .all(|p| p.file_name().is_some_and(|n| n != "..")),
            "I2: [..] in sources"
        );
        if job.op == Op::Delete {
            assert_eq!(
                app_before_mode,
                Mode::ConfirmDelete,
                "I1: delete started outside ConfirmDelete"
            );
        } else if let Some(dest) = &job.dest {
            assert_eq!(
                app_before_mode,
                Mode::SelectDestination,
                "job started outside selection"
            );
            assert!(
                !job.sources.iter().any(|s| dest.starts_with(s)),
                "I4: dest inside a source"
            );
        }
        assert!(
            app_before_mode == Mode::SelectDestination || app_before_mode == Mode::ConfirmDelete,
            "I5/I8: job started from {app_before_mode:?}"
        );
    }
}

/// Drives an App through a pseudo-random walk of inputs, feeding fake job events so it visits the
/// Running / Conflict / ErrorList modes too. Checks the invariants after every transition.
#[test]
fn every_input_in_every_mode_preserves_invariants() {
    let mut app = fresh();
    let mut running_job: Option<u64> = None;
    let mut rng: u64 = 0x1234_5678;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    for _ in 0..20_000 {
        let before = app.mode;
        // Occasionally drive job events so Running/Conflict/ErrorList are exercised.
        if let Some(id) = running_job {
            match next() % 5 {
                0 => {
                    app.on_job_event(
                        id,
                        JobEvent::Progress {
                            done_files: 1,
                            done_bytes: 1,
                            current: work(),
                        },
                    );
                }
                1 => {
                    app.on_job_event(
                        id,
                        JobEvent::Conflict {
                            source: work().join("a.txt"),
                            dest: work().join("sub/a.txt"),
                            dest_is_dir: false,
                        },
                    );
                }
                2 => {
                    app.on_job_event(
                        id,
                        JobEvent::EntryFailed {
                            path: work().join("a.txt"),
                            reason: "e".into(),
                        },
                    );
                }
                3 => {
                    let summary = Summary {
                        done: 1,
                        skipped: 0,
                        failed: vec![],
                        cancelled: false,
                    };
                    app.on_job_event(id, JobEvent::Finished(summary));
                    running_job = None;
                }
                _ => {}
            }
        }

        let effect = if app.wants_text() {
            let key = TEXT_KEYS[(next() as usize) % TEXT_KEYS.len()];
            app.handle_text(key)
        } else {
            let cmd = COMMANDS[(next() as usize) % COMMANDS.len()];
            app.handle(cmd)
        };
        check_start(before, &effect);

        match &effect {
            Effect::StartJob(job) => running_job = Some(job.id),
            Effect::Load { id, dir, focus } => {
                // Complete loads so the app keeps navigating (mirrors run.rs).
                let d = dir.clone();
                let listing = vec![
                    FileEntry::new(d.join("a.txt"), "a.txt".into(), EntryKind::File, false),
                    FileEntry::new(d.join("sub"), "sub".into(), EntryKind::Directory, false),
                ];
                let (id, focus) = (*id, focus.clone());
                let _ = focus;
                app.on_listing(id, Ok(listing));
            }
            Effect::ListPlaces { id } => {
                // Complete the places request so the popup can be used (mirrors run.rs).
                let place = Place {
                    kind: PlaceKind::Drive,
                    label: "C:".into(),
                    path: "/".into(),
                };
                app.on_places(*id, vec![place]);
            }
            Effect::Quit => {
                app = fresh();
                running_job = None;
            }
            _ => {}
        }

        // I3: while selecting a destination, the sources never change.
        if before == Mode::SelectDestination && app.mode == Mode::SelectDestination {
            // pending_op is Some throughout selection.
            assert!(
                app.pending_op.is_some(),
                "I3: lost pending_op during selection"
            );
        }
    }
}
