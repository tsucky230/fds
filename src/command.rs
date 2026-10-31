//! KeyEvent -> Command mapping (design prompt §7, §8 / R-150).
//!
//! No business logic here. Mapping lives in this one place so the keymap can become configurable.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    CursorUp,
    CursorDown,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    /// `o`: open with the OS's default application (R-110-3).
    OpenExternal,
    ParentDirectory,
    ToggleMark,
    Help,
    Refresh,
    ToggleHidden,
    /// Show or hide the info panel.
    ToggleInfo,
    /// Show or hide the preview pane (R-200-9).
    TogglePreview,
    View,
    Edit,
    Copy,
    Move,
    Delete,
    Quit,
    /// Ctrl+C: quits FDS from any screen (q/F10 only leave the viewer).
    ForceQuit,
    /// Esc: leaves the current screen (viewer, help); no-op in the list.
    Back,
    /// Viewer: toggle line wrapping (w).
    ToggleWrap,
    /// Viewer: show the same file with the next viewer (Tab).
    NextViewer,
    /// Viewer: horizontal scroll when wrapping is off (Left / Right).
    ScrollLeft,
    ScrollRight,
    /// Image viewer: p; normal-list preview: Ctrl+P. Try the next image protocol (M17).
    NextImageProtocol,
    /// Text viewer: decode with the next encoding of `encoding::next_choice` (E, M1-b).
    /// Ignored in the list and in Hex / Image viewers.
    NextEncoding,
    /// F2 / r: rename the cursor entry (P3-12).
    Rename,
    /// F7 / n: create a directory (P3-12).
    MakeDir,
    /// p in the normal list: create a ZIP, tar or tar.gz archive from marks (or the cursor entry).
    ArchiveCreate,
    /// u: extract the archive under the cursor to a new sibling directory.
    ArchiveExtract,
    /// /: incremental filename search (P3-14).
    Search,
    /// L / F9: popup with drives, home and mount points (M23 follow-up).
    Places,
    /// !: type a command to run in the listed folder (R-412).
    RunCommand,
    /// x: the same, with the box filled in for the marks or the cursor entry (R-412).
    RunCommandOnTarget,
    /// Ctrl+Z on Unix. In raw mode the terminal does not send SIGTSTP, so FDS suspends itself (R-1300-4).
    Suspend,
}

/// Maps a KeyEvent to a Command with the standard keymap. Unmapped keys return None.
///
/// - Ignores anything but Press (Windows also delivers Release/Repeat).
/// - In raw mode Ctrl+C arrives as a key, not a signal, so it is treated as Quit.
/// - Some terminals send Backspace as Ctrl+H, so Ctrl+H is also Parent.
pub fn from_key(key: KeyEvent) -> Option<Command> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl {
        return match key.code {
            KeyCode::Char('r') => Some(Command::Refresh),
            KeyCode::Char('b') => Some(Command::PageUp),
            KeyCode::Char('f') => Some(Command::PageDown),
            KeyCode::Char('c') => Some(Command::ForceQuit),
            KeyCode::Char('h') => Some(Command::ParentDirectory),
            KeyCode::Char('p') => Some(Command::NextImageProtocol),
            KeyCode::Char('z') if cfg!(unix) => Some(Command::Suspend),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(Command::CursorUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Command::CursorDown),
        KeyCode::PageUp => Some(Command::PageUp),
        KeyCode::PageDown => Some(Command::PageDown),
        KeyCode::Home | KeyCode::Char('g') => Some(Command::Home),
        KeyCode::End | KeyCode::Char('G') => Some(Command::End),
        KeyCode::Enter | KeyCode::Char('l') => Some(Command::Enter),
        KeyCode::Char('o') => Some(Command::OpenExternal),
        KeyCode::Char('!') => Some(Command::RunCommand),
        KeyCode::Char('x') => Some(Command::RunCommandOnTarget),
        KeyCode::Backspace | KeyCode::Char('h') => Some(Command::ParentDirectory),
        KeyCode::Char(' ') => Some(Command::ToggleMark),
        KeyCode::F(1) | KeyCode::Char('?') => Some(Command::Help),
        KeyCode::Char('.') => Some(Command::ToggleHidden),
        KeyCode::Char('i') => Some(Command::ToggleInfo),
        KeyCode::Char('P') => Some(Command::TogglePreview),
        KeyCode::F(3) | KeyCode::Char('v') => Some(Command::View),
        KeyCode::F(4) | KeyCode::Char('e') => Some(Command::Edit),
        KeyCode::F(5) | KeyCode::Char('c') => Some(Command::Copy),
        KeyCode::F(6) | KeyCode::Char('m') => Some(Command::Move),
        KeyCode::F(8) | KeyCode::Char('d') => Some(Command::Delete),
        KeyCode::F(10) | KeyCode::Char('q') => Some(Command::Quit),
        KeyCode::Esc => Some(Command::Back),
        KeyCode::Char('w') => Some(Command::ToggleWrap),
        KeyCode::Tab => Some(Command::NextViewer),
        KeyCode::Char('E') => Some(Command::NextEncoding),
        KeyCode::Left => Some(Command::ScrollLeft),
        KeyCode::Right => Some(Command::ScrollRight),
        KeyCode::Char('p') => Some(Command::ArchiveCreate),
        KeyCode::F(2) | KeyCode::Char('r') => Some(Command::Rename),
        KeyCode::F(7) | KeyCode::Char('n') => Some(Command::MakeDir),
        KeyCode::Char('u') => Some(Command::ArchiveExtract),
        KeyCode::Char('/') => Some(Command::Search),
        KeyCode::F(9) | KeyCode::Char('L') => Some(Command::Places),
        _ => None,
    }
}

/// Maps a key while the viewer is shown (R-500-6).
///
/// Behavior:
/// - A Press of lowercase `p` switches image protocol in the viewer; in the normal list it
///   creates an archive.
/// - `vi_keys` true: a Press of 'h' -> `ScrollLeft` and 'l' -> `ScrollRight` when Ctrl is not held
///   (only the lowercase letters). Every other key is `from_key`, so Backspace, Ctrl+H, q, Esc,
///   F3 and v still close the viewer.
pub fn viewer_key(key: KeyEvent, vi_keys: bool) -> Option<Command> {
    if key.kind == KeyEventKind::Press
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('p')
    {
        return Some(Command::NextImageProtocol);
    }
    if vi_keys && key.kind == KeyEventKind::Press && !key.modifiers.contains(KeyModifiers::CONTROL)
    {
        match key.code {
            KeyCode::Char('h') => return Some(Command::ScrollLeft),
            KeyCode::Char('l') => return Some(Command::ScrollRight),
            _ => {}
        }
    }
    from_key(key)
}

/// The viewer's Help list for the keymap in use (`viewer_key`).
///
/// Behavior: `vi_keys` false -> `VIEWER_KEY_HELP`. True -> the same actions in the same order,
/// with h left out of the close keys and h / l added to the sideways scrolling keys.
pub fn viewer_key_help(vi_keys: bool) -> &'static [(&'static str, &'static str)] {
    if vi_keys {
        VIEWER_KEY_HELP_VI
    } else {
        VIEWER_KEY_HELP
    }
}

/// Key list shown on the viewer's Help screen: (keys, catalog key of the action).
pub const VIEWER_KEY_HELP: &[(&str, &str)] = &[
    ("Up / k", "vhelp.up"),
    ("Down / j", "vhelp.down"),
    ("PgUp / Ctrl+B", "help.page_up"),
    ("PgDn / Ctrl+F", "help.page_down"),
    ("Home / g", "vhelp.top"),
    ("End / G", "vhelp.bottom"),
    ("Left / Right", "vhelp.hscroll"),
    ("w", "vhelp.wrap"),
    ("Tab", "vhelp.next_viewer"),
    ("E", "vhelp.encoding"),
    ("p", "vhelp.protocol"),
    ("q / Esc / F3 / v / h / Backspace", "vhelp.close"),
    ("Ctrl+C", "vhelp.quit_app"),
];

/// `VIEWER_KEY_HELP` for `[viewer] vi_keys`: h / l scroll sideways, so h no longer closes.
const VIEWER_KEY_HELP_VI: &[(&str, &str)] = &[
    ("Up / k", "vhelp.up"),
    ("Down / j", "vhelp.down"),
    ("PgUp / Ctrl+B", "help.page_up"),
    ("PgDn / Ctrl+F", "help.page_down"),
    ("Home / g", "vhelp.top"),
    ("End / G", "vhelp.bottom"),
    ("Left / Right / h / l", "vhelp.hscroll"),
    ("w", "vhelp.wrap"),
    ("Tab", "vhelp.next_viewer"),
    ("E", "vhelp.encoding"),
    ("p", "vhelp.protocol"),
    ("q / Esc / F3 / v / Backspace", "vhelp.close"),
    ("Ctrl+C", "vhelp.quit_app"),
];

/// Key list shown on the Help screen (R-108-1): (keys, catalog key of the action).
/// Kept next to the keymap so they do not drift. Key names are not translated.
pub const KEY_HELP: &[(&str, &str)] = &[
    ("Up / k", "help.cursor_up"),
    ("Down / j", "help.cursor_down"),
    ("PgUp / Ctrl+B", "help.page_up"),
    ("PgDn / Ctrl+F", "help.page_down"),
    ("Home / g", "help.home"),
    ("End / G", "help.end"),
    ("Enter / l", "help.enter"),
    ("Backspace / h / Left", "help.parent"),
    ("Space", "help.mark"),
    ("F1 / ?", "help.help"),
    ("Ctrl+R", "help.refresh"),
    (".", "help.toggle_hidden"),
    ("i", "help.info"),
    ("P", "help.preview"),
    ("Tab", "help.peek"),
    ("Ctrl+P", "help.preview_protocol"),
    ("F3 / v", "help.view"),
    ("o", "help.open_external"),
    ("!", "help.command"),
    ("x", "help.command_target"),
    ("Ctrl+C", "help.force_quit"),
    ("F4 / e", "help.edit"),
    ("F5 / c", "help.copy"),
    ("F6 / m", "help.move"),
    ("F8 / d", "help.delete"),
    ("F2 / r", "help.rename"),
    ("F7 / n", "help.mkdir"),
    ("p", "help.archive_create"),
    ("u", "help.archive_extract"),
    ("/", "help.search"),
    ("L / F9", "help.places"),
    ("F10 / q / Esc", "help.quit"),
];

/// Keys for text-driven screens (one-line input and y/n / o/s/r/c dialogs, P3-9, P3-12, P3-14).
/// Letters must reach these screens as characters, not as Commands ("q" is text, not Quit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKey {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    /// Up / Ctrl+P: previous search match.
    Up,
    /// Down / Ctrl+N: next search match.
    Down,
    Home,
    End,
    Enter,
    Esc,
    /// Ctrl+C: quit FDS even from a text screen.
    ForceQuit,
}

/// Maps a KeyEvent for a text screen. Press events only.
///
/// - Printable characters (with or without Shift) -> Char.
/// - Ctrl+C -> ForceQuit, Ctrl+N -> Down, Ctrl+P -> Up, Ctrl+H -> Backspace.
/// - Other Ctrl / Alt combinations and function keys -> None.
pub fn text_key(key: KeyEvent) -> Option<TextKey> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if ctrl {
        return match key.code {
            KeyCode::Char('c') => Some(TextKey::ForceQuit),
            KeyCode::Char('n') => Some(TextKey::Down),
            KeyCode::Char('p') => Some(TextKey::Up),
            KeyCode::Char('h') => Some(TextKey::Backspace),
            _ => None,
        };
    }
    if alt {
        return None;
    }
    match key.code {
        KeyCode::Char(c) => Some(TextKey::Char(c)),
        KeyCode::Backspace => Some(TextKey::Backspace),
        KeyCode::Delete => Some(TextKey::Delete),
        KeyCode::Left => Some(TextKey::Left),
        KeyCode::Right => Some(TextKey::Right),
        KeyCode::Up => Some(TextKey::Up),
        KeyCode::Down => Some(TextKey::Down),
        KeyCode::Home => Some(TextKey::Home),
        KeyCode::End => Some(TextKey::End),
        KeyCode::Enter => Some(TextKey::Enter),
        KeyCode::Esc => Some(TextKey::Esc),
        _ => None,
    }
}
