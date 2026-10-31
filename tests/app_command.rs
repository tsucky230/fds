//! Typing a command from the list (R-412): the keys, the input box, the history and what is asked
//! of the event loop.

use std::path::PathBuf;

use fds::app::{App, Effect, InputPurpose, Mode};
use fds::command::{Command, KEY_HELP, TextKey, from_key};
use fds::entry::{EntryKind, FileEntry};
use fds::i18n::Catalog;
use fds::message::Message;
use fds::pane::CursorFocus;
use fds::shell_cmd::{CommandError, ShellKind};
use fds::ui::render;
use fds::ui_viewer::ImageSlot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn root() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" })
}

fn entry(name: &str, kind: EntryKind) -> FileEntry {
    FileEntry::new(root().join(name), name.into(), kind, false)
}

/// A list in `root()` on Posix (so the expected lines do not depend on the OS), the cursor on the
/// first entry.
fn app_with(entries: Vec<FileEntry>) -> App {
    let (mut app, effect) = App::new(root());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(id, Ok(entries));
    app.shell_kind = ShellKind::Posix;
    app
}

fn files(names: &[&str]) -> App {
    app_with(names.iter().map(|n| entry(n, EntryKind::File)).collect())
}

fn cursor_on(app: &mut App, name: &str) {
    app.handle(Command::Home);
    while app.pane.cursor_entry().unwrap().display_name != name {
        app.handle(Command::CursorDown);
    }
}

fn mark(app: &mut App, name: &str) {
    cursor_on(app, name);
    app.handle(Command::ToggleMark);
}

fn typing(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_text(TextKey::Char(c));
    }
}

fn box_text(app: &App) -> (String, usize) {
    let input = app.input.as_ref().expect("input box open");
    (input.editor.text().to_string(), input.editor.cursor())
}

fn run(app: &mut App, typed: &str) -> Effect {
    app.handle(Command::RunCommand);
    typing(app, typed);
    app.handle_text(TextKey::Enter)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn bang_and_x_are_the_keys() {
    assert_eq!(from_key(key(KeyCode::Char('!'))), Some(Command::RunCommand));
    assert_eq!(
        from_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT)),
        Some(Command::RunCommand)
    );
    assert_eq!(
        from_key(key(KeyCode::Char('x'))),
        Some(Command::RunCommandOnTarget)
    );
    assert_eq!(
        from_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)),
        None
    );
    for (keys, action) in [("!", "help.command"), ("x", "help.command_target")] {
        assert!(KEY_HELP.contains(&(keys, action)), "{keys}");
    }
}

#[test]
fn bang_opens_an_empty_box() {
    let mut app = files(&["a"]);
    assert_eq!(app.handle(Command::RunCommand), Effect::None);
    assert_eq!(app.mode, Mode::Input);
    assert!(matches!(
        app.input.as_ref().unwrap().purpose,
        InputPurpose::Command { .. }
    ));
    assert_eq!(box_text(&app), (String::new(), 0));
}

#[test]
fn enter_runs_the_line_in_the_listed_folder() {
    let mut app = files(&["a"]);
    assert_eq!(
        run(&mut app, "git status"),
        Effect::RunShell {
            dir: root(),
            line: "git status".into()
        }
    );
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.input.is_none());
}

#[test]
fn esc_cancels() {
    let mut app = files(&["a"]);
    app.handle(Command::RunCommand);
    typing(&mut app, "rm -rf x");
    assert_eq!(app.handle_text(TextKey::Esc), Effect::None);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status, Some(Message::Cancelled));
    assert!(app.command_history.is_empty());
}

#[test]
fn letters_in_the_box_are_text_not_commands() {
    let mut app = files(&["a"]);
    app.handle(Command::RunCommand);
    typing(&mut app, "q!x");
    assert_eq!(box_text(&app).0, "q!x");
    assert_eq!(app.mode, Mode::Input);
}

#[test]
fn x_with_marks_leaves_room_for_the_command() {
    let mut app = files(&["a", "b", "c"]);
    mark(&mut app, "a");
    mark(&mut app, "c");
    assert_eq!(app.handle(Command::RunCommandOnTarget), Effect::None);
    assert_eq!(app.mode, Mode::Input);
    assert_eq!(box_text(&app), (" {files}".to_string(), 0));

    typing(&mut app, "tar cf x.tar");
    assert_eq!(
        app.handle_text(TextKey::Enter),
        Effect::RunShell {
            dir: root(),
            line: "tar cf x.tar 'a' 'c'".into()
        }
    );
}

#[test]
fn marks_win_over_a_program_under_the_cursor() {
    // The marks were chosen on purpose; the cursor may just happen to be on a program.
    let mut app = files(&["a", "build.sh"]);
    mark(&mut app, "a");
    cursor_on(&mut app, "build.sh");
    assert_eq!(app.handle(Command::RunCommandOnTarget), Effect::None);
    assert_eq!(box_text(&app), (" {files}".to_string(), 0));
}

#[test]
fn x_on_a_file_asks_the_event_loop_to_look_at_it() {
    let mut app = files(&["build.sh"]);
    assert_eq!(
        app.handle(Command::RunCommandOnTarget),
        Effect::PrefillCommand {
            path: root().join("build.sh")
        }
    );
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn x_on_a_program_is_ready_for_arguments() {
    let mut app = files(&["build.sh"]);
    app.on_command_classified(root().join("build.sh"), true);
    assert_eq!(app.mode, Mode::Input);
    let text = "./build.sh ".to_string();
    let end = text.len();
    assert_eq!(box_text(&app), (text, end));

    typing(&mut app, "--release");
    assert_eq!(
        app.handle_text(TextKey::Enter),
        Effect::RunShell {
            dir: root(),
            line: "./build.sh --release".into()
        }
    );
}

#[test]
fn x_on_a_program_with_spaces_quotes_it() {
    let mut app = files(&["my tool.sh"]);
    app.on_command_classified(root().join("my tool.sh"), true);
    assert_eq!(box_text(&app).0, "'./my tool.sh' ");
    typing(&mut app, "-v");
    assert_eq!(
        app.handle_text(TextKey::Enter),
        Effect::RunShell {
            dir: root(),
            line: "'./my tool.sh' -v".into()
        }
    );
}

#[test]
fn x_on_a_document_leaves_room_for_the_command() {
    let mut app = files(&["a b.txt"]);
    app.on_command_classified(root().join("a b.txt"), false);
    assert_eq!(box_text(&app), (" {file}".to_string(), 0));
    typing(&mut app, "wc -l");
    assert_eq!(
        app.handle_text(TextKey::Enter),
        Effect::RunShell {
            dir: root(),
            line: "wc -l 'a b.txt'".into()
        }
    );
}

#[test]
fn x_on_a_program_cmd_cannot_quote_opens_no_box() {
    let mut app = files(&["a%b.exe"]);
    app.shell_kind = ShellKind::Cmd;
    app.on_command_classified(root().join("a%b.exe"), true);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.status,
        Some(Message::CommandRejected(CommandError::UnsafeName {
            name: "a%b.exe".into()
        }))
    );
}

#[test]
fn x_on_a_folder_needs_no_look_at_the_disk() {
    let mut app = app_with(vec![entry("sub", EntryKind::Directory)]);
    assert_eq!(app.handle(Command::RunCommandOnTarget), Effect::None);
    assert_eq!(box_text(&app), (" {file}".to_string(), 0));
}

#[test]
fn x_with_nothing_to_act_on_opens_no_box() {
    // No entries at all, and the cursor on [..].
    let mut empty = app_with(vec![]);
    assert_eq!(empty.handle(Command::RunCommandOnTarget), Effect::None);
    assert_eq!(empty.mode, Mode::Normal);
    assert_eq!(empty.status, Some(Message::NothingSelected));

    let mut parent = app_with(vec![
        FileEntry::parent_link(PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" })),
        entry("a", EntryKind::File),
    ]);
    parent.handle(Command::Home);
    assert!(parent.pane.cursor_entry().unwrap().is_parent_link());
    assert_eq!(parent.handle(Command::RunCommandOnTarget), Effect::None);
    assert_eq!(parent.mode, Mode::Normal);
    assert_eq!(parent.status, Some(Message::NothingSelected));
}

#[test]
fn enter_on_the_untouched_prefill_runs_nothing() {
    // Otherwise the first marked file would run as a program.
    let mut app = files(&["a", "b"]);
    mark(&mut app, "a");
    app.handle(Command::RunCommandOnTarget);
    assert_eq!(app.handle_text(TextKey::Enter), Effect::None);
    assert_eq!(app.mode, Mode::Input);
    assert_eq!(
        app.status,
        Some(Message::CommandRejected(CommandError::NoCommand))
    );
}

#[test]
fn a_refused_line_stays_in_the_box_as_it_was() {
    let mut app = files(&["a"]);
    app.handle(Command::RunCommand);
    typing(&mut app, "echo \"{file}\"");
    app.handle_text(TextKey::Left);
    let before = box_text(&app);
    assert_eq!(app.handle_text(TextKey::Enter), Effect::None);
    assert_eq!(app.mode, Mode::Input);
    assert_eq!(box_text(&app), before);
    assert_eq!(
        app.status,
        Some(Message::CommandRejected(CommandError::QuotedPlaceholder))
    );
    assert!(app.command_history.is_empty());

    // The message goes away as soon as the line is edited.
    app.handle_text(TextKey::Char('!'));
    assert_eq!(app.status, None);
}

#[test]
fn files_without_marks_is_the_cursor() {
    let mut app = files(&["a", "b"]);
    cursor_on(&mut app, "b");
    assert_eq!(
        run(&mut app, "ls {files}"),
        Effect::RunShell {
            dir: root(),
            line: "ls 'b'".into()
        }
    );
}

#[test]
fn placeholders_on_the_parent_link_are_refused() {
    for template in ["ls {file}", "ls {files}"] {
        let mut app = app_with(vec![FileEntry::parent_link(PathBuf::from(
            if cfg!(windows) { r"C:\" } else { "/" },
        ))]);
        assert_eq!(run(&mut app, template), Effect::None);
        assert_eq!(
            app.status,
            Some(Message::CommandRejected(CommandError::NoTarget)),
            "{template}"
        );
    }
}

#[test]
fn the_target_count_follows_the_marks() {
    let mut app = files(&["a", "b", "c"]);
    assert_eq!(app.command_target_count(), 1);
    mark(&mut app, "a");
    mark(&mut app, "b");
    assert_eq!(app.command_target_count(), 2);
    mark(&mut app, "c");
    assert_eq!(app.command_target_count(), 3);

    let parent = app_with(vec![FileEntry::parent_link(PathBuf::from(
        if cfg!(windows) { r"C:\" } else { "/" },
    ))]);
    assert_eq!(parent.command_target_count(), 0);
}

#[test]
fn cmd_refuses_a_unc_folder_and_keeps_the_line() {
    let dir = PathBuf::from(r"\\server\share");
    let (mut app, effect) = App::new(dir.clone());
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(
        id,
        Ok(vec![FileEntry::new(
            dir.join("a"),
            "a".into(),
            EntryKind::File,
            false,
        )]),
    );
    app.shell_kind = ShellKind::Cmd;
    assert_eq!(run(&mut app, "dir"), Effect::None);
    assert_eq!(app.mode, Mode::Input);
    assert_eq!(
        app.status,
        Some(Message::CommandRejected(CommandError::UncFolder))
    );
}

#[test]
fn up_and_down_walk_the_history_and_keep_the_draft() {
    let mut app = files(&["a"]);
    for line in ["one", "two", "three"] {
        run(&mut app, line);
    }
    app.handle(Command::RunCommand);
    typing(&mut app, "draft");

    for expected in ["three", "two", "one", "one"] {
        app.handle_text(TextKey::Up);
        assert_eq!(box_text(&app).0, expected);
        assert_eq!(box_text(&app).1, expected.len(), "cursor at the end");
    }
    for expected in ["two", "three", "draft", "draft"] {
        app.handle_text(TextKey::Down);
        assert_eq!(box_text(&app).0, expected);
    }
}

#[test]
fn the_history_holds_the_line_as_typed() {
    let mut app = files(&["a", "b"]);
    mark(&mut app, "a");
    run(&mut app, "ls {files}");
    assert_eq!(app.command_history, ["ls {files}"]);

    // The same line again, on other marks.
    mark(&mut app, "a");
    mark(&mut app, "b");
    app.handle(Command::RunCommand);
    app.handle_text(TextKey::Up);
    assert_eq!(
        app.handle_text(TextKey::Enter),
        Effect::RunShell {
            dir: root(),
            line: "ls 'b'".into()
        }
    );
}

#[test]
fn the_history_skips_repeats_and_is_bounded() {
    let mut app = files(&["a"]);
    run(&mut app, "same");
    run(&mut app, "same");
    assert_eq!(app.command_history, ["same"]);
    run(&mut app, "other");
    run(&mut app, "same");
    assert_eq!(app.command_history, ["same", "other", "same"]);

    for i in 0..150 {
        run(&mut app, &format!("line{i}"));
    }
    assert_eq!(app.command_history.len(), 100);
    assert_eq!(app.command_history.first().unwrap(), "line50");
    assert_eq!(app.command_history.last().unwrap(), "line149");
}

#[test]
fn up_with_no_history_does_nothing() {
    let mut app = files(&["a"]);
    app.handle(Command::RunCommand);
    typing(&mut app, "x");
    app.handle_text(TextKey::Up);
    assert_eq!(box_text(&app).0, "x");
    app.handle_text(TextKey::Down);
    assert_eq!(box_text(&app).0, "x");
}

#[test]
fn the_other_input_boxes_keep_their_keys() {
    // Up / Down in the rename box are not the command history.
    let mut app = files(&["a"]);
    run(&mut app, "one");
    assert_eq!(app.command_history, ["one"]);
    app.handle(Command::MakeDir);
    typing(&mut app, "x");
    app.handle_text(TextKey::Up);
    assert_eq!(box_text(&app).0, "x");
}

#[test]
fn a_cd_line_moves_the_list() {
    let mut app = files(&["a"]);
    let Effect::Load { dir, focus, .. } = run(&mut app, "cd sub") else {
        panic!()
    };
    assert_eq!(dir, root().join("sub"));
    assert_eq!(focus, CursorFocus::First);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.command_history, ["cd sub"]);
}

#[test]
fn cd_works_on_the_folder_under_the_cursor() {
    let mut app = app_with(vec![entry("my folder", EntryKind::Directory)]);
    let Effect::Load { dir, .. } = run(&mut app, "cd {file}") else {
        panic!()
    };
    assert_eq!(dir, root().join("my folder"));

    let mut app = app_with(vec![entry("it's", EntryKind::Directory)]);
    // Not a plain path any more, so the shell gets it (and FDS stays where it is).
    assert!(matches!(
        run(&mut app, "cd {file}"),
        Effect::RunShell { .. }
    ));
}

#[test]
fn cd_to_a_missing_folder_stays_put_and_says_so() {
    let mut app = files(&["a"]);
    let Effect::Load { id, .. } = run(&mut app, "cd nope") else {
        panic!()
    };
    app.on_listing(id, Err("not found".into()));
    assert_eq!(app.pane.current_dir, root());
    assert!(matches!(app.status, Some(Message::CannotOpen { .. })));
}

#[test]
fn other_cd_lines_go_to_the_shell() {
    let mut app = files(&["a"]);
    assert!(matches!(
        run(&mut app, "cd sub && make"),
        Effect::RunShell { .. }
    ));
}

#[test]
fn after_the_command_the_list_is_read_again_and_marks_stay() {
    let mut app = files(&["a", "b", "c"]);
    mark(&mut app, "a");
    mark(&mut app, "b");
    cursor_on(&mut app, "c");
    let Effect::Load { id, dir, focus } = app.on_command_finished(Ok(())) else {
        panic!()
    };
    assert_eq!(dir, root());
    assert_eq!(focus, CursorFocus::Keep);
    assert_eq!(app.status, None);

    // `rm b; touch new` ran: the mark on b goes with it, the mark on a stays.
    app.on_listing(
        id,
        Ok(vec![
            entry("a", EntryKind::File),
            entry("c", EntryKind::File),
            entry("new", EntryKind::File),
        ]),
    );
    let marked: Vec<&str> = app
        .pane
        .marked_entries()
        .iter()
        .map(|e| e.display_name.as_str())
        .collect();
    assert_eq!(marked, ["a"]);
    assert_eq!(app.pane.cursor_entry().unwrap().display_name, "c");
}

#[test]
fn a_shell_that_cannot_start_is_reported_and_the_list_still_reloads() {
    let mut app = files(&["a"]);
    let effect = app.on_command_finished(Err("no shell".into()));
    assert!(matches!(effect, Effect::Load { .. }));
    assert_eq!(
        app.status,
        Some(Message::CommandFailed {
            reason: "no shell".into()
        })
    );
}

#[test]
fn only_the_list_takes_these_keys() {
    for command in [Command::RunCommand, Command::RunCommandOnTarget] {
        let mut app = app_with(vec![entry("sub", EntryKind::Directory)]);
        app.handle(command);
        assert_eq!(app.mode, Mode::Input, "the keys work in the list");
    }
    for mode in [
        Mode::Viewer,
        Mode::ViewerLoading,
        Mode::PreviewBrowse,
        Mode::ArchiveBrowse,
        Mode::SelectDestination,
        Mode::Places,
        Mode::Running,
        Mode::ConfirmDelete,
        Mode::ConfirmQuit,
    ] {
        for command in [Command::RunCommand, Command::RunCommandOnTarget] {
            let mut app = files(&["a"]);
            app.mode = mode;
            app.handle(command);
            assert!(app.input.is_none(), "{mode:?}");
            assert_ne!(app.mode, Mode::Input, "{mode:?}");
        }
    }
}

fn screen(app: &mut App, catalog: &Catalog) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|f| render(f, app, catalog, &mut ImageSlot::default()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_box_shows_how_many_entries_are_targets_and_what_the_placeholders_are() {
    let mut app = files(&["a", "b", "c"]);
    mark(&mut app, "a");
    mark(&mut app, "b");
    app.handle(Command::RunCommand);
    let shown = screen(&mut app, &Catalog::english());
    assert!(shown.contains("Command (targets: 2)"), "{shown}");
    assert!(shown.contains("{files}"), "{shown}");
}

#[test]
fn a_refusal_is_on_screen_while_the_box_is_open() {
    let mut app = files(&["a"]);
    run(&mut app, "echo \"{file}\"");
    let shown = screen(&mut app, &Catalog::english());
    assert!(shown.contains("inside quotes"), "{shown}");
    assert_eq!(app.mode, Mode::Input);
}

#[test]
fn every_refusal_has_its_own_text() {
    let catalog = Catalog::english();
    let reasons = [
        CommandError::NoCommand,
        CommandError::QuotedPlaceholder,
        CommandError::FilesNotAWord,
        CommandError::NoTarget,
        CommandError::NonUtf8 { name: "n1".into() },
        CommandError::UnsafeName { name: "n2".into() },
        CommandError::TooLong,
        CommandError::UncFolder,
    ];
    let texts: Vec<String> = reasons
        .into_iter()
        .map(|r| Message::CommandRejected(r).render(&catalog))
        .collect();
    for (i, text) in texts.iter().enumerate() {
        assert!(!text.is_empty());
        assert!(!text.contains("command."), "key shown as text: {text}");
        assert!(!texts[..i].contains(text), "{text}");
    }
    assert!(texts[4].contains("n1"));
    assert!(texts[5].contains("n2"));

    let failed = Message::CommandFailed {
        reason: "boom\x1b[31m".into(),
    }
    .render(&catalog);
    assert!(failed.contains("boom"));
    assert!(!failed.contains('\x1b'), "control characters are cleaned");
}

#[cfg(unix)]
#[test]
fn ctrl_c_is_for_the_running_program() {
    use fds::terminal::signal_ends_fds;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    assert!(signal_ends_fds(SIGINT, false));
    assert!(!signal_ends_fds(SIGINT, true));
    for signal in [SIGTERM, SIGHUP] {
        assert!(signal_ends_fds(signal, false));
        assert!(signal_ends_fds(signal, true));
    }
}
