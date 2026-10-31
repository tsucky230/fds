//! Keymap (AC 4-10, 28-30 / R-150).

use fds::command::{Command, KEY_HELP, from_key};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn standard_keys_and_alternatives() {
    let cases = [
        (KeyCode::Up, Command::CursorUp),
        (KeyCode::Char('k'), Command::CursorUp),
        (KeyCode::Down, Command::CursorDown),
        (KeyCode::Char('j'), Command::CursorDown),
        (KeyCode::Enter, Command::Enter),
        (KeyCode::Char('l'), Command::Enter),
        (KeyCode::Backspace, Command::ParentDirectory),
        (KeyCode::Char('h'), Command::ParentDirectory),
        (KeyCode::Char(' '), Command::ToggleMark),
        (KeyCode::F(1), Command::Help),
        (KeyCode::Char('?'), Command::Help),
        (KeyCode::Char('.'), Command::ToggleHidden),
        (KeyCode::F(3), Command::View),
        (KeyCode::Char('v'), Command::View),
        (KeyCode::F(4), Command::Edit),
        (KeyCode::Char('e'), Command::Edit),
        (KeyCode::F(5), Command::Copy),
        (KeyCode::Char('c'), Command::Copy),
        (KeyCode::F(6), Command::Move),
        (KeyCode::Char('m'), Command::Move),
        (KeyCode::F(8), Command::Delete),
        (KeyCode::Char('d'), Command::Delete),
        (KeyCode::F(10), Command::Quit),
        (KeyCode::PageUp, Command::PageUp),
        (KeyCode::PageDown, Command::PageDown),
        (KeyCode::Home, Command::Home),
        (KeyCode::Char('g'), Command::Home),
        (KeyCode::End, Command::End),
        (KeyCode::Char('p'), Command::ArchiveCreate),
        (KeyCode::Char('q'), Command::Quit),
    ];
    for (code, cmd) in cases {
        assert_eq!(from_key(press(code)), Some(cmd), "{code:?}");
    }
}

#[test]
fn control_keys() {
    assert_eq!(from_key(ctrl('r')), Some(Command::Refresh));
    assert_eq!(from_key(ctrl('b')), Some(Command::PageUp));
    assert_eq!(from_key(ctrl('f')), Some(Command::PageDown));
    // Shift+g arrives as 'G' (some terminals also set SHIFT).
    assert_eq!(from_key(press(KeyCode::Char('G'))), Some(Command::End));
    assert_eq!(
        from_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
        Some(Command::End)
    );
    assert_eq!(from_key(ctrl('c')), Some(Command::ForceQuit));
    assert_eq!(from_key(ctrl('h')), Some(Command::ParentDirectory));
    assert_eq!(from_key(ctrl('p')), Some(Command::NextImageProtocol));
    let expected = if cfg!(unix) {
        Some(Command::Suspend)
    } else {
        None
    };
    assert_eq!(from_key(ctrl('z')), expected);
    assert_eq!(from_key(ctrl('x')), None);
}

#[test]
fn release_and_repeat_are_ignored() {
    for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
        let mut key = press(KeyCode::Char('j'));
        key.kind = kind;
        assert_eq!(from_key(key), None);
    }
}

#[test]
fn unmapped_keys_return_none() {
    assert_eq!(from_key(press(KeyCode::Char('z'))), None);
    assert_eq!(from_key(press(KeyCode::F(11))), None);
}

#[test]
fn every_function_key_has_an_alternative_in_help() {
    // R-150-2: all operations work without function keys.
    for (keys, _) in KEY_HELP {
        if keys.starts_with('F') {
            assert!(keys.contains(" / "), "{keys} has no alternative");
        }
    }
}

#[test]
fn shift_e_switches_the_encoding_and_e_still_edits() {
    assert_eq!(
        from_key(press(KeyCode::Char('E'))),
        Some(Command::NextEncoding)
    );
    assert_eq!(from_key(press(KeyCode::Char('e'))), Some(Command::Edit));
    assert!(
        fds::command::VIEWER_KEY_HELP
            .iter()
            .any(|(keys, action)| *keys == "E" && *action == "vhelp.encoding"),
        "the viewer help lists E"
    );
}

#[test]
fn p_creates_an_archive_in_the_list_and_still_cycles_protocols_in_the_viewer() {
    for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
        assert_eq!(
            from_key(KeyEvent::new(KeyCode::Char('P'), modifiers)),
            Some(Command::TogglePreview),
            "{modifiers:?}"
        );
    }
    assert_eq!(
        from_key(press(KeyCode::Char('p'))),
        Some(Command::ArchiveCreate)
    );
    assert_eq!(from_key(press(KeyCode::Char('a'))), None);
    assert_eq!(
        fds::command::viewer_key(press(KeyCode::Char('p')), false),
        Some(Command::NextImageProtocol)
    );
    assert_eq!(
        fds::command::viewer_key(press(KeyCode::Char('p')), true),
        Some(Command::NextImageProtocol)
    );
    assert!(
        KEY_HELP
            .iter()
            .any(|(keys, action)| *keys == "P" && *action == "help.preview"),
        "the help lists P"
    );
    assert!(
        KEY_HELP
            .iter()
            .any(|(keys, action)| *keys == "p" && *action == "help.archive_create")
    );
    assert!(
        KEY_HELP
            .iter()
            .any(|(keys, action)| *keys == "Ctrl+P" && *action == "help.preview_protocol")
    );
}

#[test]
fn vi_keys_turn_h_and_l_into_sideways_scrolling_in_the_viewer() {
    use Command::*;
    use fds::command::viewer_key;
    let shift = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
    // (key, vi_keys off, vi_keys on)
    for (key, off, on) in [
        (
            press(KeyCode::Char('h')),
            Some(ParentDirectory),
            Some(ScrollLeft),
        ),
        (press(KeyCode::Char('l')), Some(Enter), Some(ScrollRight)),
        (
            press(KeyCode::Backspace),
            Some(ParentDirectory),
            Some(ParentDirectory),
        ),
        (ctrl('h'), Some(ParentDirectory), Some(ParentDirectory)),
        (press(KeyCode::Left), Some(ScrollLeft), Some(ScrollLeft)),
        (press(KeyCode::Right), Some(ScrollRight), Some(ScrollRight)),
        (press(KeyCode::Char('q')), Some(Quit), Some(Quit)),
        (press(KeyCode::Esc), Some(Back), Some(Back)),
        (press(KeyCode::Char('v')), Some(View), Some(View)),
        (press(KeyCode::F(3)), Some(View), Some(View)),
        (
            press(KeyCode::Char('j')),
            Some(CursorDown),
            Some(CursorDown),
        ),
        (shift('L'), Some(Places), Some(Places)),
        (press(KeyCode::Char('H')), None, None),
    ] {
        assert_eq!(viewer_key(key, false), off, "{key:?} off");
        assert_eq!(viewer_key(key, true), on, "{key:?} on");
        assert_eq!(
            viewer_key(key, false),
            from_key(key),
            "{key:?}: off is from_key"
        );
    }
    for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
        let mut key = press(KeyCode::Char('h'));
        key.kind = kind;
        assert_eq!(viewer_key(key, true), None, "{kind:?}");
    }
}

#[test]
fn the_viewer_help_follows_vi_keys() {
    use fds::command::{VIEWER_KEY_HELP, viewer_key_help};
    assert_eq!(viewer_key_help(false), VIEWER_KEY_HELP);
    let vi = viewer_key_help(true);
    let actions =
        |list: &[(&str, &str)]| list.iter().map(|(_, a)| a.to_string()).collect::<Vec<_>>();
    assert_eq!(
        actions(vi),
        actions(VIEWER_KEY_HELP),
        "same actions, same order"
    );
    let keys_of = |action: &str| -> Vec<&str> {
        let (keys, _) = vi.iter().find(|(_, a)| *a == action).unwrap();
        keys.split(" / ").collect()
    };
    let close = keys_of("vhelp.close");
    assert!(!close.contains(&"h"), "{close:?}");
    for k in ["q", "Esc", "F3", "v", "Backspace"] {
        assert!(close.contains(&k), "{k} still closes: {close:?}");
    }
    let sideways = keys_of("vhelp.hscroll");
    for k in ["Left", "Right", "h", "l"] {
        assert!(sideways.contains(&k), "{k}: {sideways:?}");
    }
}
