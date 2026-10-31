//! Rendering smoke tests with ratatui's TestBackend (AC 11-14, 17, 18, 30).

use std::path::PathBuf;

use fds::app::{App, Effect};
use fds::archive::Member;
use fds::command::{Command, KEY_HELP};
use fds::entry::{EntryKind, FileEntry};
use fds::i18n::Catalog;
use fds::info::format_clock;
use fds::ui::{render, title_text};
use fds::ui_viewer::ImageSlot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

fn app_with(names: &[&str]) -> App {
    let dir = PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" });
    let (mut app, effect) = App::new(dir.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    let listing = names
        .iter()
        .map(|n| FileEntry::new(dir.join(n), (*n).into(), EntryKind::File, false))
        .collect();
    app.on_listing(id, Ok(listing));
    app
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let catalog = Catalog::english();
    terminal
        .draw(|f| render(f, app, &catalog, &mut ImageSlot::default()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn archive_browser_shows_its_members_and_counts() {
    let mut app = app_with(&["bundle.zip", "outside.txt"]);
    let Effect::OpenArchive { id, .. } = app.handle(Command::Enter) else {
        panic!()
    };
    app.on_archive_loaded(
        id,
        Ok(vec![
            Member {
                path: "inside.txt".into(),
                is_dir: false,
                size: 3,
            },
            Member {
                path: "folder".into(),
                is_dir: true,
                size: 0,
            },
        ]),
    );
    let screen = text(&draw(&mut app, 80, 24));
    assert!(screen.contains("inside.txt"), "{screen}");
    assert!(!screen.contains("outside.txt"), "{screen}");
    assert!(screen.contains("1 files   1 dirs"), "{screen}");

    app.handle(Command::ArchiveExtract);
    let prompt = text(&draw(&mut app, 80, 24));
    assert!(prompt.contains("inside.txt"), "{prompt}");
    assert!(prompt.contains("New extraction folder"), "{prompt}");
}

#[test]
fn renders_at_many_sizes_without_panic() {
    let long = "x".repeat(300);
    let mut app = app_with(&[
        "日本語のとても長いファイル名.txt",
        "e\u{301}moji😀.md",
        &long,
        "a\nb",
    ]);
    for (w, h) in [
        (1, 1),
        (39, 10),
        (40, 9),
        (40, 10),
        (79, 14),
        (80, 15),
        (80, 24),
        (200, 60),
    ] {
        draw(&mut app, w, h);
    }
}

#[test]
fn too_small_shows_warning_and_recovers() {
    let mut app = app_with(&["a.txt"]);
    let small = text(&draw(&mut app, 39, 10));
    assert!(small.contains("Terminal too small"), "{small}");
    let normal = text(&draw(&mut app, 80, 24));
    assert!(!normal.contains("Terminal too small"));
    assert!(normal.contains("a.txt"));
}

#[test]
fn control_characters_are_not_drawn_raw() {
    let mut app = app_with(&["evil\u{1b}[2Jname"]);
    let screen = text(&draw(&mut app, 80, 24));
    assert!(!screen.contains('\u{1b}'));
    assert!(screen.contains("evil?[2Jname"));
}

#[test]
fn status_bar_is_reversed_across_full_width() {
    use ratatui::style::Modifier;
    let mut app = app_with(&["a.txt"]);
    app.show_frame = false;
    let buf = draw(&mut app, 80, 24);
    // Status is the second-to-last row in Normal layout.
    let last = &buf[(79, 22)];
    assert!(
        last.modifier.contains(Modifier::REVERSED),
        "status bar must span the full width"
    );

    // With the frame, the bar spans the inside of the frame (one row and column in).
    let mut app = app_with(&["a.txt"]);
    let buf = draw(&mut app, 80, 24);
    assert!(buf[(78, 21)].modifier.contains(Modifier::REVERSED));
    assert!(buf[(1, 21)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn help_lists_every_key_even_on_small_screens() {
    for (w, h) in [(80, 24), (80, 15), (40, 10)] {
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::Help);
        let screen = text(&draw(&mut app, w, h));
        for (keys, _) in KEY_HELP {
            // Keys may be truncated in narrow columns, but each key's first part must be visible.
            let first = keys.split(" / ").next().unwrap();
            assert!(screen.contains(first), "{w}x{h}: {first} missing\n{screen}");
        }
    }
}

#[test]
fn help_shows_the_version_first_even_on_small_screens() {
    let version = format!("FDS {}", env!("CARGO_PKG_VERSION"));
    for (w, h) in [(100, 30), (80, 24), (40, 10)] {
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::Help);
        let screen = text(&draw(&mut app, w, h));
        assert!(
            screen.contains(&version),
            "{w}x{h}
{screen}"
        );
    }
}

#[test]
fn help_shows_the_supporter_section_below_the_keys_when_there_is_room() {
    // Phase 4 (C32, C37). Keys keep priority on small screens (test above).
    use fds::license::LicenseStatus;
    let mut app = app_with(&["a.txt"]);
    app.supporter.license = LicenseStatus::NotInstalled;
    app.supporter.dev_key = true;
    app.handle(Command::Help);
    let screen = text(&draw(&mut app, 120, 50));
    assert!(screen.contains("Supporter"), "{screen}");
    assert!(screen.contains("TEST KEY"), "{screen}");
    for (w, h) in [(1, 1), (40, 10), (80, 15), (80, 24)] {
        draw(&mut app, w, h);
    }
}

mod viewer_screen {
    use super::*;
    use fds::viewer::styled::StyledLine;
    use fds::viewer::{Document, Opened, TextDoc, ViewerKind};

    fn app_with_viewer(lines: Vec<&str>) -> App {
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::View);
        let dir = app.pane.current_dir.clone();
        // The only pending id is the latest one; ids start at 1 and the listing used 1.
        app.on_viewer(
            2,
            Ok(Opened {
                path: dir.join("a.txt"),
                candidates: vec![ViewerKind::Text, ViewerKind::Hex],
                kind: ViewerKind::Text,
                document: Document::Lines(TextDoc {
                    lines: lines.into_iter().map(StyledLine::plain).collect(),
                    encoding: "Shift_JIS".into(),
                    truncated: true,
                    file_len: 10,
                }),
                encoding: None,
                fallback: None,
            }),
        );
        app
    }

    #[test]
    fn the_viewer_help_shows_the_keymap_in_use() {
        // (vi_keys, shown, not shown)
        for (vi_keys, shown, hidden) in [
            (
                false,
                "q / Esc / F3 / v / h / Backspace",
                "Left / Right / h / l",
            ),
            (true, "Left / Right / h / l", "F3 / v / h /"),
        ] {
            let mut app = app_with_viewer(vec!["one"]);
            app.vi_keys = vi_keys;
            app.handle(Command::Help);
            let screen = text(&draw(&mut app, 100, 40));
            assert!(
                screen.contains(shown),
                "{vi_keys}: {shown}
{screen}"
            );
            assert!(
                !screen.contains(hidden),
                "{vi_keys}: {hidden}
{screen}"
            );
        }
    }

    #[test]
    fn viewer_shows_text_encoding_and_truncation() {
        let mut app = app_with_viewer(vec!["hello viewer", "日本語"]);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("hello viewer"), "{screen}");
        assert!(screen.contains("日"));
        assert!(screen.contains("Shift_JIS"));
        assert!(screen.contains("8 MiB"));
        assert!(screen.contains("a.txt"));
    }

    #[test]
    fn a_forced_encoding_is_marked_as_manual() {
        let mut app = app_with_viewer(vec!["hello"]);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(!screen.contains("(manual)"), "detected: {screen}");
        app.viewer.as_mut().unwrap().encoding = Some(encoding_rs::SHIFT_JIS);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("Shift_JIS (manual)"), "{screen}");
    }

    #[test]
    fn viewer_survives_all_sizes_and_too_small() {
        let long = "x".repeat(500);
        let mut app = app_with_viewer(vec![&long; 50]);
        for (w, h) in [(1, 1), (39, 10), (40, 10), (80, 24), (200, 60)] {
            draw(&mut app, w, h);
        }
        assert!(text(&draw(&mut app, 39, 10)).contains("Terminal too small"));
    }

    #[test]
    fn loading_status_is_shown() {
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::View);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("Opening a.txt"), "{screen}");
    }
}

mod ops_screen {
    use super::*;
    use fds::app::Mode;
    use fds::command::TextKey;

    fn ready() -> App {
        app_with(&["a.txt", "b.txt"])
    }

    /// Starts a Copy of a.txt into the same folder and returns the job id.
    fn copying(app: &mut App) -> u64 {
        app.handle(Command::Copy);
        let Effect::StartJob(job) = app.handle(Command::Copy) else {
            panic!()
        };
        job.id
    }

    #[test]
    fn running_status_shows_bytes_when_known() {
        use fds::ops::JobEvent;
        let mut app = ready();
        let id = copying(&mut app);
        app.on_job_event(
            id,
            JobEvent::Scanned {
                files: 1,
                bytes: 4 << 30,
            },
        );
        app.on_job_event(
            id,
            JobEvent::Progress {
                done_files: 0,
                done_bytes: 1 << 30,
                current: app.pane.current_dir.join("a.txt"),
            },
        );
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("0/1"), "{screen}");
        assert!(screen.contains("1.0 GiB / 4.0 GiB"), "{screen}");
    }

    #[test]
    fn running_status_has_no_bytes_when_there_are_none() {
        use fds::ops::JobEvent;
        let mut app = ready();
        let id = copying(&mut app);
        app.on_job_event(id, JobEvent::Scanned { files: 3, bytes: 0 });
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("0/3"), "{screen}");
        assert!(!screen.contains("0 B /"), "{screen}");
    }

    #[test]
    fn delete_confirmation_lists_targets() {
        let mut app = ready(); // the cursor starts on the first real entry, a.txt
        app.handle(Command::Delete);
        assert_eq!(app.mode, Mode::ConfirmDelete);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("Delete 1 item"), "{screen}");
        assert!(screen.contains("a.txt"), "{screen}");
        assert!(screen.contains("y/N"), "{screen}");
    }

    #[test]
    fn the_confirmation_says_permanently_or_trash() {
        // The wording is the safety net when the setting is not what the user thinks (M1-a).
        let mut app = ready();
        app.handle(Command::Delete);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("permanently"), "{screen}");
        assert!(!screen.contains("trash"), "{screen}");

        let mut app = ready();
        app.delete_mode = fds::ops::DeleteMode::Trash;
        app.handle(Command::Delete);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("Move 1 item(s) to the trash"), "{screen}");
        assert!(!screen.contains("permanently"), "{screen}");
    }

    #[test]
    fn a_trash_job_is_named_as_such() {
        use fds::ops::{JobEvent, Summary};
        let mut app = ready();
        app.delete_mode = fds::ops::DeleteMode::Trash;
        app.handle(Command::Delete);
        let Effect::StartJob(job) = app.handle_text(TextKey::Char('y')) else {
            panic!()
        };
        app.on_job_event(job.id, JobEvent::Scanned { files: 1, bytes: 0 });
        let running = text(&draw(&mut app, 100, 24));
        assert!(running.contains("Move to trash 0/1"), "{running}");
        app.on_job_event(
            job.id,
            JobEvent::Finished(Summary {
                done: 1,
                ..Summary::default()
            }),
        );
        let done = text(&draw(&mut app, 100, 24));
        assert!(done.contains("Move to trash done: 1"), "{done}");
    }

    #[test]
    fn destination_selection_and_input_prompts_are_visible() {
        let mut app = ready();
        app.handle(Command::Copy);
        assert!(text(&draw(&mut app, 80, 24)).contains("Copy 1"));

        let mut app = ready();
        app.handle(Command::MakeDir);
        app.handle_text(TextKey::Char('n'));
        app.handle_text(TextKey::Char('e'));
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("New directory"), "{screen}");
        assert!(screen.contains("ne"), "{screen}");
        assert!(screen.contains("Enter: OK"), "{screen}");
    }

    #[test]
    fn dialogs_survive_every_size() {
        let mut app = ready();
        app.handle(Command::CursorDown);
        app.handle(Command::Delete);
        for (w, h) in [(1, 1), (39, 10), (40, 10), (80, 24)] {
            draw(&mut app, w, h);
        }
    }
}

mod destination_screen {
    use super::*;
    use fds::app::Mode;
    use fds::ops::place::{Place, PlaceKind};

    fn selecting() -> App {
        let mut app = app_with(&["a.txt", "b.txt"]);
        app.handle(Command::Copy);
        assert_eq!(app.mode, Mode::SelectDestination);
        app
    }

    #[test]
    fn banner_says_what_where_and_how_it_relates() {
        let mut app = selecting();
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("Copy: a.txt"), "{screen}");
        assert!(screen.contains("Destination:"), "{screen}");
        assert!(screen.contains("same folder as the source"), "{screen}");
        assert!(screen.contains("[Copy here]"), "{screen}");
    }

    #[test]
    fn key_guide_follows_the_cursor() {
        let mut app = selecting();
        let on_here = text(&draw(&mut app, 100, 24));
        assert!(on_here.contains("Enter: Copy here"), "{on_here}");
        app.handle(Command::CursorDown);
        let on_list = text(&draw(&mut app, 100, 24));
        assert!(on_list.contains("Enter: open folder"), "{on_list}");
        assert!(on_list.contains("L: drives/places"), "{on_list}");
    }

    #[test]
    fn move_uses_its_own_words() {
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::Move);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("Move: a.txt"), "{screen}");
        assert!(screen.contains("[Move here]"), "{screen}");
    }

    #[test]
    fn places_popup_lists_drives() {
        let mut app = selecting();
        let fds::app::Effect::ListPlaces { id } = app.handle(Command::Places) else {
            panic!()
        };
        let drive = |label: &str, path: &str| Place {
            kind: PlaceKind::Drive,
            label: label.into(),
            path: path.into(),
        };
        app.on_places(id, vec![drive("C:", r"C:\"), drive("D:", r"D:\")]);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("Drives and places"), "{screen}");
        assert!(screen.contains("Where the items came from"), "{screen}");
        assert!(screen.contains("C:  Drive"), "{screen}");
        assert!(screen.contains("D:  Drive"), "{screen}");
    }

    #[test]
    fn places_popup_keeps_the_operation_and_hides_list_keys() {
        // Over the destination screen the status still says what is being copied, and the
        // list's key guide (F5 Copy ... F10 Quit) is not offered while the popup has the keys.
        let mut app = selecting();
        let fds::app::Effect::ListPlaces { id } = app.handle(Command::Places) else {
            panic!()
        };
        app.on_places(id, vec![]);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("Copy 1:"), "{screen}");
        assert!(!screen.contains("F10 Quit"), "{screen}");
    }

    #[test]
    fn list_keys_are_not_offered_outside_the_list() {
        // Running and the delete confirmation take their own keys; the list guide would lie.
        let mut running = selecting();
        running.handle(Command::Copy);
        assert_eq!(running.mode, Mode::Running);
        let mut confirming = app_with(&["a.txt", "b.txt"]);
        confirming.handle(Command::CursorDown);
        confirming.handle(Command::Delete);
        assert_eq!(confirming.mode, Mode::ConfirmDelete);
        for app in [&mut running, &mut confirming] {
            let screen = text(&draw(app, 100, 24));
            assert!(!screen.contains("F10 Quit"), "{screen}");
        }
        let mut normal = app_with(&["a.txt"]);
        assert!(text(&draw(&mut normal, 100, 24)).contains("F10 Quit"));
    }

    #[test]
    fn selection_hint_lists_the_actions() {
        let mut app = app_with(&["a.txt", "b.txt"]);
        app.handle(Command::ToggleMark);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("1 selected"), "{screen}");
        assert!(screen.contains("F5 Copy"), "{screen}");
        assert!(screen.contains("F8 Delete"), "{screen}");
    }

    #[test]
    fn small_screens_do_not_panic() {
        let mut app = selecting();
        for (w, h) in [(1, 1), (40, 10), (41, 11), (60, 12), (80, 24)] {
            draw(&mut app, w, h);
        }
        let fds::app::Effect::ListPlaces { id } = app.handle(Command::Places) else {
            panic!()
        };
        app.on_places(id, vec![]);
        for (w, h) in [(1, 1), (40, 10), (80, 24)] {
            draw(&mut app, w, h);
        }
    }
}

mod input_box {
    use super::*;

    #[test]
    fn rename_box_shows_old_name_and_current_text() {
        let mut app = app_with(&["a.txt", "b.txt"]);
        app.handle(Command::CursorDown);
        app.handle(Command::Rename);
        let screen = text(&draw(&mut app, 80, 24));
        assert!(screen.contains("Rename:"), "{screen}");
        assert!(screen.contains("Esc: cancel"), "{screen}");
    }

    #[test]
    fn input_box_survives_every_size() {
        let mut app = app_with(&["a.txt", "b.txt"]);
        app.handle(Command::MakeDir);
        for (w, h) in [(1, 1), (39, 10), (40, 10), (40, 6), (80, 24), (200, 5)] {
            draw(&mut app, w, h);
        }
    }
}

mod frame_and_scrollbar {
    use super::*;

    fn many(n: usize) -> App {
        let names: Vec<String> = (0..n).map(|i| format!("file{i:03}.txt")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        app_with(&refs)
    }

    fn cell(buf: &Buffer, x: u16, y: u16) -> String {
        buf[(x, y)].symbol().to_string()
    }

    #[test]
    fn frame_surrounds_the_whole_app_by_default() {
        let mut app = app_with(&["a.txt"]);
        assert!(app.show_frame);
        let buf = draw(&mut app, 80, 24);
        assert_eq!(cell(&buf, 0, 0), "┌");
        assert_eq!(cell(&buf, 79, 0), "┐");
        assert_eq!(cell(&buf, 0, 23), "└");
        assert_eq!(cell(&buf, 79, 23), "┘");
    }

    #[test]
    fn frame_can_be_turned_off() {
        let mut app = app_with(&["a.txt"]);
        app.show_frame = false;
        let buf = draw(&mut app, 80, 24);
        assert_ne!(cell(&buf, 0, 0), "┌");
        assert!(text(&buf).contains("a.txt"));
    }

    #[test]
    fn frame_is_skipped_when_it_would_not_leave_room() {
        let mut app = app_with(&["a.txt"]);
        for (w, h) in [(1, 1), (40, 10), (41, 12), (42, 11)] {
            let buf = draw(&mut app, w, h);
            assert_ne!(cell(&buf, 0, 0), "┌", "{w}x{h}");
        }
        let buf = draw(&mut app, 42, 12);
        assert_eq!(cell(&buf, 0, 0), "┌");
    }

    #[test]
    fn scrollbar_appears_only_when_the_list_overflows() {
        let mut few = app_with(&["a.txt", "b.txt"]);
        assert!(!text(&draw(&mut few, 80, 24)).contains('┃'));
        let mut lots = many(100);
        assert!(text(&draw(&mut lots, 80, 24)).contains('┃'));
    }

    #[test]
    fn scrollbar_thumb_moves_down_with_the_cursor() {
        let thumb_row = |app: &mut App| {
            let buf = draw(app, 80, 24);
            // The list's right edge moves with the preview pane (R-200-9): search the whole row.
            (0..24u16)
                .find(|&y| (0..80u16).any(|x| cell(&buf, x, y) == "┃"))
                .unwrap()
        };
        let mut app = many(100);
        let top = thumb_row(&mut app);
        for _ in 0..90 {
            app.handle(Command::CursorDown);
        }
        assert!(thumb_row(&mut app) > top);
    }

    #[test]
    fn everything_still_fits_in_a_framed_screen() {
        let mut app = many(100);
        for (w, h) in [(42, 12), (60, 15), (80, 24), (200, 60)] {
            draw(&mut app, w, h);
        }
    }
}

mod info_panel {
    use super::*;
    use fds::info::{Info, InfoKind, MarkSummary};
    use std::time::{Duration, UNIX_EPOCH};

    fn base(path: &str) -> Info {
        Info {
            path: PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" }).join(path),
            kind: InfoKind::File,
            size: Some(2048),
            modified: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            created: None,
            unix_mode: Some(0o755),
            owner: Some((1000, 1001)),
            win_attrs: None,
            readonly: false,
            link_target: None,
            link_broken: false,
            dir_items: None,
        }
    }

    fn loaded(info: Info, marks: Option<MarkSummary>) -> App {
        let mut app = app_with(&["a.txt", "b.txt"]);
        let req = app.want_info().unwrap();
        app.on_info(req.id, Some(info), marks);
        app
    }

    #[test]
    fn shows_permissions_owner_size_and_time() {
        let mut app = loaded(base("a.txt"), None);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("Info"), "{screen}");
        assert!(screen.contains("a.txt"), "{screen}");
        assert!(screen.contains("755 rwxr-xr-x"), "{screen}");
        assert!(screen.contains("1000:1001"), "{screen}");
        assert!(screen.contains("2.0 KiB"), "{screen}");
        assert!(screen.contains("2023-11-1"), "{screen}");
    }

    #[test]
    fn shows_windows_attributes_and_read_only() {
        let mut info = base("a.txt");
        info.unix_mode = None;
        info.owner = None;
        info.win_attrs = Some(0x1 | 0x20);
        info.readonly = true;
        let mut app = loaded(info, None);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("R--A"), "{screen}");
        assert!(screen.contains("read-only"), "{screen}");
    }

    #[test]
    fn shows_link_target_and_broken_links() {
        let mut info = base("a.txt");
        info.kind = InfoKind::Symlink;
        info.link_target = Some(PathBuf::from("gone.txt"));
        info.link_broken = true;
        let mut app = loaded(info, None);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("-> gone.txt"), "{screen}");
        assert!(screen.contains("broken"), "{screen}");
    }

    #[test]
    fn shows_folder_item_counts() {
        let mut info = base("a.txt");
        info.kind = InfoKind::Dir;
        info.size = None;
        info.dir_items = Some((12, false));
        let mut app = loaded(info.clone(), None);
        assert!(text(&draw(&mut app, 100, 24)).contains("12 items"));
        info.dir_items = Some((10_000, true));
        let mut app = loaded(info, None);
        assert!(text(&draw(&mut app, 100, 24)).contains("10000+ items"));
    }

    #[test]
    fn shows_the_marked_summary() {
        let marks = MarkSummary {
            files: 2,
            dirs: 1,
            bytes: 120,
        };
        let mut app = loaded(base("a.txt"), Some(marks));
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("3 marked"), "{screen}");
        assert!(screen.contains("120 B"), "{screen}");
    }

    #[test]
    fn details_of_another_item_are_not_shown_for_the_cursor() {
        // Details for b.txt while the cursor is on a.txt must not be presented as a.txt's.
        let mut app = loaded(base("b.txt"), None);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(!screen.contains("755 rwxr-xr-x"), "{screen}");
        assert!(screen.contains("Loading"), "{screen}");
    }

    #[test]
    fn broken_link_is_visible_even_with_a_long_target() {
        // The first line is cut at the panel width; the cut must not hide "(broken)".
        let mut info = base("a.txt");
        info.kind = InfoKind::Symlink;
        info.link_target = Some(PathBuf::from("t".repeat(300)));
        info.link_broken = true;
        let mut app = loaded(info, None);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(screen.contains("(broken)"), "{screen}");
        assert!(screen.contains("-> ttt"), "{screen}");
    }

    #[test]
    fn i_hides_the_panel() {
        let mut app = loaded(base("a.txt"), None);
        app.handle(Command::ToggleInfo);
        let screen = text(&draw(&mut app, 100, 24));
        assert!(!screen.contains("755 rwxr-xr-x"), "{screen}");
    }

    #[test]
    fn panel_is_hidden_when_the_screen_is_short_and_never_panics() {
        let mut app = loaded(base("a.txt"), None);
        let short = text(&draw(&mut app, 100, 14));
        assert!(!short.contains("755 rwxr-xr-x"), "{short}");
        for (w, h) in [(1, 1), (40, 10), (42, 12), (60, 15), (80, 24), (200, 60)] {
            draw(&mut app, w, h);
        }
    }
}

mod preview_pane {
    //! The preview pane right of the list (R-200-9).
    use super::*;
    use fds::preview::{PreviewDoc, PreviewError};
    use fds::viewer::ViewerKind;
    use fds::viewer::detect::ImageFormat;
    use fds::viewer::styled::StyledLine;
    use std::time::Instant;

    fn lines_doc(lines: &[&str]) -> PreviewDoc {
        PreviewDoc::Lines {
            kind: ViewerKind::Text,
            lines: lines.iter().map(|l| StyledLine::plain(*l)).collect(),
            encoding: "UTF-8".into(),
            truncated: false,
        }
    }

    fn with_preview(app: &mut App, result: Result<PreviewDoc, PreviewError>) {
        let req = app.want_preview(Instant::now()).expect("preview request");
        app.on_preview(req.id, result);
    }

    /// (column, row) where `needle` starts. Only for ASCII rows.
    fn find(buf: &Buffer, needle: &str) -> Option<(u16, u16)> {
        for y in 0..buf.area.height {
            let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(i) = row.find(needle) {
                return Some((i as u16, y));
            }
        }
        None
    }

    /// Columns of box corners right of the frame and the info panel (x > 1).
    fn pane_corners(buf: &Buffer) -> Vec<u16> {
        let mut xs = Vec::new();
        for y in 0..buf.area.height {
            for x in 2..buf.area.width {
                if buf[(x, y)].symbol() == "\u{250c}" {
                    xs.push(x);
                }
            }
        }
        xs
    }

    #[test]
    fn the_cursor_file_is_previewed_right_of_the_list() {
        let mut app = app_with(&["a.txt", "b.txt"]);
        with_preview(
            &mut app,
            Ok(lines_doc(&["PREVIEW-LINE-1", "PREVIEW-LINE-2"])),
        );
        let buf = draw(&mut app, 100, 30);
        let (list_x, _) = find(&buf, "b.txt").expect("list");
        let (one_x, one_y) = find(&buf, "PREVIEW-LINE-1").expect("first line");
        let (two_x, two_y) = find(&buf, "PREVIEW-LINE-2").expect("second line");
        assert!(list_x < 40, "list on the left: {list_x}");
        assert!(one_x > 40, "preview on the right: {one_x}");
        assert_eq!((two_x, two_y), (one_x, one_y + 1));
    }

    #[test]
    fn the_pane_needs_an_80_column_terminal() {
        // (terminal width, frame, preview shown)
        for (width, frame, shown) in [
            (79, true, false),
            (80, true, true),
            (81, true, true),
            (120, true, true),
            (77, false, false),
            (78, false, true),
            (80, false, true),
        ] {
            let mut app = app_with(&["a.txt"]);
            app.show_frame = frame;
            with_preview(&mut app, Ok(lines_doc(&["PREVIEW-LINE-1"])));
            let screen = text(&draw(&mut app, width, 30));
            assert_eq!(
                screen.contains("PREVIEW-LINE-1"),
                shown,
                "{width} frame={frame}\n{screen}"
            );
            assert!(screen.contains("a.txt"), "the list is always there");
        }
    }

    #[test]
    fn another_items_content_is_never_drawn() {
        let mut app = app_with(&["a.txt", "b.txt"]);
        with_preview(&mut app, Ok(lines_doc(&["PREVIEW-OF-A"])));
        app.handle(Command::CursorDown);
        let screen = text(&draw(&mut app, 100, 30));
        assert!(!screen.contains("PREVIEW-OF-A"), "{screen}");
        assert!(screen.contains("Loading"), "{screen}");
        assert!(screen.matches("b.txt").count() >= 2, "the pane names b.txt");
    }

    #[test]
    fn the_pane_keeps_its_width_on_the_parent_link() {
        let mut app = app_with(&["a.txt"]);
        with_preview(&mut app, Ok(lines_doc(&["PREVIEW-LINE-1"])));
        let on_file = pane_corners(&draw(&mut app, 100, 30));
        app.handle(Command::Home);
        let on_parent = pane_corners(&draw(&mut app, 100, 30));
        assert_eq!(on_file.len(), 1, "one preview box");
        assert_eq!(on_file, on_parent);
    }

    #[test]
    fn turning_it_off_gives_the_list_the_whole_width() {
        let long = "a-very-long-file-name-that-is-wider-than-the-list-column-xxxxx.txt";
        let mut app = app_with(&[long]);
        with_preview(&mut app, Ok(lines_doc(&["PREVIEW-LINE-1"])));
        let on = text(&draw(&mut app, 100, 30));
        assert!(on.contains("PREVIEW-LINE-1"));
        app.handle(Command::TogglePreview);
        let buf = draw(&mut app, 100, 30);
        let off = text(&buf);
        assert!(!off.contains("PREVIEW-LINE-1"), "{off}");
        assert!(off.contains(long), "{off}");
        assert!(pane_corners(&buf).is_empty());
    }

    #[test]
    fn the_info_panel_stays_under_list_and_preview() {
        let mut app = app_with(&["a.txt"]);
        with_preview(&mut app, Ok(lines_doc(&["PREVIEW-LINE-1"])));
        let req = app.want_info().unwrap();
        let info = fds::info::Info {
            path: req.path.clone().unwrap(),
            kind: fds::info::InfoKind::File,
            size: Some(2048),
            modified: None,
            created: None,
            unix_mode: Some(0o755),
            owner: None,
            win_attrs: None,
            readonly: false,
            link_target: None,
            link_broken: false,
            dir_items: None,
        };
        app.on_info(req.id, Some(info), None);
        let buf = draw(&mut app, 100, 30);
        let (_, preview_y) = find(&buf, "PREVIEW-LINE-1").unwrap();
        let (info_x, info_y) = find(&buf, "2.0 KiB").expect("info panel");
        assert!(info_y > preview_y, "the panel is below");
        assert!(info_x < 40, "the panel spans from the left");
    }

    #[test]
    fn each_kind_of_preview_is_drawn() {
        let w = PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" });
        let folder = PreviewDoc::Folder {
            entries: vec![
                FileEntry::new(w.join("dir1"), "dir1".into(), EntryKind::Directory, false),
                FileEntry::new(w.join("file1"), "file1".into(), EntryKind::File, false),
            ],
            more: true,
        };
        let cases: Vec<(Result<PreviewDoc, PreviewError>, Vec<&str>)> = vec![
            (
                Ok(PreviewDoc::Binary { file_len: 3 }),
                vec!["Binary file", "Enter: show as hex"],
            ),
            (
                Ok(PreviewDoc::Image {
                    format: ImageFormat::Png,
                    dimensions: Some((40, 20)),
                    file_len: 2048,
                }),
                vec!["PNG", "40x20"],
            ),
            (
                Ok(PreviewDoc::Image {
                    format: ImageFormat::Jpeg,
                    dimensions: None,
                    file_len: 2048,
                }),
                vec!["JPEG"],
            ),
            (Ok(folder), vec!["dir1", "file1", "and more"]),
            (Err(PreviewError::NotAFile), vec!["Not a regular file"]),
            (
                Err(PreviewError::Io("Permission denied".into())),
                vec!["Cannot read", "Permission denied"],
            ),
        ];
        for (result, expected) in cases {
            let mut app = app_with(&["a.txt"]);
            with_preview(&mut app, result.clone());
            let screen = text(&draw(&mut app, 100, 30));
            for needle in expected {
                assert!(screen.contains(needle), "{result:?}: {needle}\n{screen}");
            }
            for (w, h) in [(1, 1), (40, 10), (80, 10), (80, 24), (100, 15), (200, 60)] {
                draw(&mut app, w, h);
            }
        }
    }

    #[test]
    fn names_in_a_folder_preview_are_sanitized() {
        let w = PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" });
        let mut app = app_with(&["a.txt"]);
        with_preview(
            &mut app,
            Ok(PreviewDoc::Folder {
                entries: vec![FileEntry::new(
                    w.join("x"),
                    "evil\u{1b}[2Jname".into(),
                    EntryKind::File,
                    false,
                )],
                more: false,
            }),
        );
        let screen = text(&draw(&mut app, 100, 30));
        assert!(!screen.contains('\u{1b}'));
        assert!(screen.contains("evil?[2Jname"), "{screen}");
    }

    #[test]
    fn the_input_box_keeps_the_preview() {
        let mut app = app_with(&["a.txt"]);
        with_preview(&mut app, Ok(lines_doc(&["PREVIEW-LINE-1"])));
        app.handle(Command::Rename);
        let screen = text(&draw(&mut app, 100, 30));
        assert!(screen.contains("PREVIEW-LINE-1"), "{screen}");
    }

    #[test]
    fn drawing_tells_the_app_whether_the_pane_is_on_screen() {
        // (terminal width, pane on screen): nothing is loaded for a pane that is not drawn.
        for (width, shown) in [(79, false), (80, true), (100, true)] {
            let mut app = app_with(&["a.txt"]);
            draw(&mut app, width, 30);
            assert_eq!(app.want_preview(Instant::now()).is_some(), shown, "{width}");
        }
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::Help);
        draw(&mut app, 100, 30);
        app.handle(Command::Help); // any key closes Help
        assert!(
            app.want_preview(Instant::now()).is_none(),
            "Help drew no pane"
        );
        draw(&mut app, 100, 30);
        assert!(
            app.want_preview(Instant::now()).is_some(),
            "back on the list"
        );
    }

    // ---------- pictures in the pane (R-200-10) ----------

    fn picture(w: u32, h: u32) -> PreviewDoc {
        // Two colors, so halfblocks draw real half blocks.
        let img = image::RgbImage::from_fn(w, h, |_, y| {
            if y < h / 2 {
                image::Rgb([200, 30, 30])
            } else {
                image::Rgb([30, 30, 200])
            }
        });
        PreviewDoc::Picture {
            format: ImageFormat::Png,
            dimensions: (w, h),
            file_len: 93,
            thumbnail: fds::preview::Thumbnail(std::sync::Arc::new(
                image::DynamicImage::ImageRgb8(img),
            )),
        }
    }

    fn draw_with(app: &mut App, width: u16, height: u16, images: &mut ImageSlot) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let catalog = Catalog::english();
        terminal.draw(|f| render(f, app, &catalog, images)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// Half-block cells right of `x0`.
    fn half_blocks(buf: &Buffer, x0: u16) -> usize {
        let mut n = 0;
        for y in 0..buf.area.height {
            for x in x0..buf.area.width {
                if matches!(buf[(x, y)].symbol(), "\u{2580}" | "\u{2584}") {
                    n += 1;
                }
            }
        }
        n
    }

    fn halfblocks() -> ratatui_image::picker::Picker {
        ratatui_image::picker::Picker::halfblocks()
    }

    #[allow(deprecated)]
    fn sixel() -> ratatui_image::picker::Picker {
        let mut p =
            ratatui_image::picker::Picker::from_fontsize(ratatui_image::FontSize::new(8, 16));
        p.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
        p
    }

    #[test]
    fn a_picture_is_drawn_under_its_caption() {
        use fds::ui_preview::prepare_preview_image;
        let mut app = app_with(&["a.png", "b.txt"]);
        with_preview(&mut app, Ok(picture(40, 20)));
        let mut images = ImageSlot::default();
        assert!(!prepare_preview_image(
            &app,
            &mut images,
            Some(&halfblocks())
        ));
        let buf = draw_with(&mut app, 100, 30, &mut images);
        let screen = text(&buf);
        assert!(
            screen.contains("PNG") && screen.contains("40x20"),
            "{screen}"
        );
        assert!(half_blocks(&buf, 40) > 0, "the picture is drawn\n{screen}");
        assert_eq!(
            half_blocks(&buf, 0),
            half_blocks(&buf, 40),
            "only in the pane"
        );
        let (_, caption_y) = find(&buf, "40x20").unwrap();
        let first_block_y = (0..buf.area.height)
            .find(|&y| (40..buf.area.width).any(|x| buf[(x, y)].symbol() == "\u{2580}"))
            .unwrap();
        assert!(first_block_y > caption_y, "under the caption");
    }

    #[test]
    fn another_items_picture_is_never_drawn() {
        use fds::ui_preview::prepare_preview_image;
        let mut app = app_with(&["a.png", "b.txt"]);
        with_preview(&mut app, Ok(picture(40, 20)));
        let mut images = ImageSlot::default();
        prepare_preview_image(&app, &mut images, Some(&halfblocks()));
        app.handle(Command::CursorDown);
        // Even with a stale picture still in the slot, the pane must not draw it.
        let buf = draw_with(&mut app, 100, 30, &mut images);
        assert_eq!(half_blocks(&buf, 0), 0, "{}", text(&buf));
        assert!(!prepare_preview_image(
            &app,
            &mut images,
            Some(&halfblocks())
        ));
        assert!(images.preview_protocol.is_none() && images.preview_owner.is_none());
    }

    #[test]
    fn without_a_picker_only_the_caption_is_shown() {
        use fds::ui_preview::prepare_preview_image;
        let mut app = app_with(&["a.png"]);
        with_preview(&mut app, Ok(picture(40, 20)));
        let mut images = ImageSlot::default();
        prepare_preview_image(&app, &mut images, None);
        let buf = draw_with(&mut app, 100, 30, &mut images);
        assert!(text(&buf).contains("40x20"));
        assert_eq!(half_blocks(&buf, 0), 0);
    }

    #[test]
    fn dropping_a_graphics_picture_asks_for_a_clear() {
        use fds::ui_preview::prepare_preview_image;
        // (picker, clear needed when the picture leaves)
        for (name, picker, clear) in [
            ("halfblocks", halfblocks(), false),
            ("sixel", sixel(), true),
        ] {
            let mut app = app_with(&["a.png", "b.txt"]);
            with_preview(&mut app, Ok(picture(40, 20)));
            let mut images = ImageSlot::default();
            assert!(
                !prepare_preview_image(&app, &mut images, Some(&picker)),
                "{name}: built"
            );
            assert_eq!(images.preview_graphics, clear, "{name}");
            assert!(
                !prepare_preview_image(&app, &mut images, Some(&picker)),
                "{name}: kept"
            );
            app.handle(Command::CursorDown);
            assert_eq!(
                prepare_preview_image(&app, &mut images, Some(&picker)),
                clear,
                "{name}"
            );
            assert!(
                !prepare_preview_image(&app, &mut images, Some(&picker)),
                "{name}: once"
            );
        }
    }

    #[test]
    fn a_hidden_pane_drops_its_picture() {
        use fds::ui_preview::prepare_preview_image;
        let mut app = app_with(&["a.png"]);
        with_preview(&mut app, Ok(picture(40, 20)));
        let mut images = ImageSlot::default();
        prepare_preview_image(&app, &mut images, Some(&sixel()));
        draw_with(&mut app, 79, 30, &mut images); // too narrow: no pane
        assert!(prepare_preview_image(&app, &mut images, Some(&sixel())));
        assert!(images.preview_protocol.is_none());
    }

    #[test]
    fn pictures_survive_all_sizes() {
        use fds::ui_preview::prepare_preview_image;
        let mut app = app_with(&["a.png"]);
        with_preview(&mut app, Ok(picture(3000, 10)));
        let mut images = ImageSlot::default();
        prepare_preview_image(&app, &mut images, Some(&halfblocks()));
        for (w, h) in [
            (1, 1),
            (40, 10),
            (80, 10),
            (80, 12),
            (80, 24),
            (100, 15),
            (200, 60),
        ] {
            draw_with(&mut app, w, h, &mut images);
        }
    }

    #[test]
    fn the_image_mode_message_is_shown() {
        let mut app = app_with(&["a.txt"]);
        app.set_status(fds::message::Message::ImageMode {
            name: "sixel".into(),
        });
        let screen = text(&draw(&mut app, 100, 30));
        assert!(screen.contains("Image mode: sixel"), "{screen}");
    }

    #[test]
    fn the_help_screen_lists_the_preview_key() {
        let mut app = app_with(&["a.txt"]);
        app.handle(Command::Help);
        let screen = text(&draw(&mut app, 100, 60));
        assert!(screen.contains("preview"), "{screen}");
    }
}

#[test]
fn directories_are_bracketed_and_styled_unlike_a_file_named_with_brackets() {
    let dir = PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" });
    let (mut app, effect) = App::new(dir.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![
            FileEntry::new(dir.join("src"), "src".into(), EntryKind::Directory, false),
            FileEntry::new(dir.join("[a]"), "[a]".into(), EntryKind::File, false),
            FileEntry::new(dir.join("a.txt"), "a.txt".into(), EntryKind::File, false),
        ]),
    );
    app.show_preview = false;
    let buf = draw(&mut app, 60, 20);
    let rendered = text(&buf);
    assert!(!rendered.contains("[DIR]"), "{rendered}");

    let find = |needle: &str| {
        rendered
            .lines()
            .enumerate()
            .find_map(|(y, _)| {
                let cells: Vec<&str> = (0..buf.area.width)
                    .map(|x| buf[(x, y as u16)].symbol())
                    .collect();
                (0..cells.len().saturating_sub(needle.len()))
                    .find(|&x| cells[x..x + needle.len()].concat() == needle)
                    .map(|x| (x as u16, y as u16))
            })
            .unwrap_or_else(|| panic!("{needle} not in {rendered}"))
    };
    // The names line up: `[` sits in the column before them.
    let (dir_x, dir_y) = find("[src]/");
    let (file_x, file_y) = find("[a]");
    let (plain_x, _) = find("a.txt");
    assert_eq!(dir_x + 1, plain_x);
    assert_eq!(file_x, plain_x);

    let dir_cell = &buf[(dir_x + 1, dir_y)];
    assert_eq!(dir_cell.fg, Color::Cyan);
    assert!(dir_cell.modifier.contains(Modifier::BOLD));
    let file_cell = &buf[(file_x + 1, file_y)];
    assert_eq!(file_cell.fg, Color::Reset);
    assert!(!file_cell.modifier.contains(Modifier::BOLD));
}

#[test]
fn title_band_shows_name_version_copyright_and_clock() {
    let version = env!("CARGO_PKG_VERSION");
    let line = title_text(60, "2026/10/31 12:34");
    assert_eq!(line.chars().count(), 60);
    assert!(
        line.starts_with(&format!(" fds {version} © 2026 Qraft Node Kyoto")),
        "{line}"
    );
    assert!(line.ends_with("2026/10/31 12:34 "), "{line}");

    // The narrowest screen drops the copyright and keeps the clock.
    let line = title_text(40, "2026/10/31 12:34");
    assert_eq!(line.chars().count(), 40);
    assert!(line.starts_with(&format!(" fds {version}")), "{line}");
    assert!(!line.contains("Qraft"), "{line}");
    assert!(line.ends_with("2026/10/31 12:34 "), "{line}");

    assert_eq!(
        format_clock(std::time::UNIX_EPOCH, &jiff::tz::TimeZone::UTC),
        "1970/01/01 00:00"
    );
}

#[test]
fn title_band_is_the_top_row_and_gives_way_on_short_screens() {
    let mut app = app_with(&["a.txt"]);
    let buf = draw(&mut app, 80, 24);
    let top = text(&buf);
    let row = top.lines().nth(1).unwrap();
    assert!(row.contains("Qraft Node Kyoto"), "{top}");
    assert!(buf[(1, 1)].modifier.contains(Modifier::REVERSED));
    assert!(buf[(78, 1)].modifier.contains(Modifier::REVERSED));

    let mut app = app_with(&["a.txt"]);
    app.show_frame = false;
    let buf = draw(&mut app, 80, 24);
    assert!(
        text(&buf)
            .lines()
            .next()
            .unwrap()
            .contains("Qraft Node Kyoto")
    );

    // Compact height keeps its rows for the list.
    let mut app = app_with(&["a.txt"]);
    app.show_frame = false;
    let buf = draw(&mut app, 80, 14);
    assert!(!text(&buf).contains("Qraft Node Kyoto"));
}
