//! Pure logic for Phase 3: names, unique names, line editor, editor command, search, targets.

use std::collections::HashMap;
use std::path::PathBuf;

use fds::editor_cmd::{EditorSources, choose, split_command};
use fds::entry::{EntryKind, FileEntry};
use fds::line_editor::LineEditor;
use fds::ops::name::{NameError, is_case_only_change, unique_name, validate};
use fds::ops::target::targets;
use fds::pane::{CursorFocus, Pane};
use fds::search::{Direction, find};

// ---------- name validation (P3-12) ----------

#[test]
fn names_valid_on_all_platforms() {
    for name in ["a.txt", "日本語", ".bashrc", "with space", "a..b"] {
        assert_eq!(validate(name, false), Ok(()), "{name}");
        assert_eq!(validate(name, true), Ok(()), "{name}");
    }
}

#[test]
fn names_rejected_everywhere() {
    assert_eq!(validate("", false), Err(NameError::Empty));
    assert_eq!(validate(".", false), Err(NameError::Dot));
    assert_eq!(validate("..", true), Err(NameError::Dot));
    assert_eq!(validate("a/b", false), Err(NameError::Separator));
    assert_eq!(validate("a\0b", false), Err(NameError::Separator));
    // Backslash is a normal character on Unix, a separator on Windows.
    assert_eq!(validate("a\\b", false), Ok(()));
    assert_eq!(validate("a\\b", true), Err(NameError::Separator));
}

#[test]
fn names_rejected_on_windows_only() {
    for bad in ["a<b", "a>b", "a:b", "a\"b", "a|b", "a?b", "a*b", "a\u{1}b"] {
        assert_eq!(validate(bad, true), Err(NameError::InvalidChar), "{bad:?}");
        // Only '/' and NUL are invalid on Unix; the rest are Windows-only restrictions.
        assert_eq!(validate(bad, false), Ok(()), "{bad:?}");
    }
    assert_eq!(validate("name.", true), Err(NameError::TrailingDotOrSpace));
    assert_eq!(validate("name ", true), Err(NameError::TrailingDotOrSpace));
    for reserved in [
        "CON", "con", "Con.txt", "PRN", "AUX", "NUL", "COM1", "com9.log", "LPT1", "lpt9",
    ] {
        assert_eq!(
            validate(reserved, true),
            Err(NameError::Reserved),
            "{reserved}"
        );
        assert_eq!(validate(reserved, false), Ok(()), "{reserved}");
    }
    assert_eq!(validate("CONSOLE", true), Ok(()));
    assert_eq!(validate("COM10", true), Ok(()));
}

// ---------- unique names (P3-7) ----------

#[test]
fn unique_name_adds_counter_before_extension() {
    let taken = ["a.txt", "a (2).txt", "dir", ".bashrc", "archive.tar.gz"];
    let exists = |n: &str| taken.contains(&n);
    assert_eq!(unique_name("free.txt", &exists), "free.txt");
    assert_eq!(unique_name("a.txt", &exists), "a (3).txt");
    assert_eq!(unique_name("dir", &exists), "dir (2)");
    assert_eq!(unique_name(".bashrc", &exists), ".bashrc (2)");
    assert_eq!(unique_name("archive.tar.gz", &exists), "archive.tar (2).gz");
}

#[test]
fn unique_name_gives_up_instead_of_looping() {
    let always = |_: &str| true;
    assert_eq!(unique_name("x.txt", &always), "x (9999).txt");
}

#[test]
fn case_only_change() {
    assert!(is_case_only_change("Foo.TXT", "foo.txt"));
    assert!(is_case_only_change("ÄBC", "äbc"));
    assert!(!is_case_only_change("a", "a"));
    assert!(!is_case_only_change("a", "b"));
}

// ---------- line editor (P3-12, P3-14) ----------

#[test]
fn line_editor_basic_editing() {
    let mut e = LineEditor::new("abc");
    assert_eq!(e.cursor(), 3);
    e.left();
    e.insert('X');
    assert_eq!(e.text(), "abXc");
    e.backspace();
    assert_eq!(e.text(), "abc");
    e.home();
    e.delete();
    assert_eq!(e.text(), "bc");
    e.backspace(); // at start: no-op
    e.end();
    e.delete(); // at end: no-op
    assert_eq!(e.text(), "bc");
    e.right(); // at end: stays
    assert_eq!(e.cursor(), 2);
    e.insert('\n'); // control chars ignored
    assert_eq!(e.text(), "bc");
}

#[test]
fn line_editor_works_on_graphemes() {
    let mut e = LineEditor::new("日本e\u{301}");
    assert_eq!(e.cursor(), 3);
    e.backspace(); // removes "e + combining acute" as one grapheme
    assert_eq!(e.text(), "日本");
    e.left();
    e.insert('語');
    assert_eq!(e.text(), "日語本");
}

#[test]
fn rename_starts_before_the_extension() {
    let mut e = LineEditor::new("report.final.pdf");
    e.cursor_before_extension();
    assert_eq!(e.cursor(), "report.final".chars().count());
    let mut e = LineEditor::new(".bashrc");
    e.cursor_before_extension();
    assert_eq!(e.cursor(), 7);
    let mut e = LineEditor::new("Makefile");
    e.cursor_before_extension();
    assert_eq!(e.cursor(), 8);
}

// ---------- editor command (P3-15) ----------

#[test]
fn split_without_a_shell() {
    assert_eq!(split_command("vim"), ["vim"]);
    assert_eq!(split_command("  code   --wait "), ["code", "--wait"]);
    assert_eq!(
        split_command(r#""C:\Program Files\Editor\ed.exe" -n"#),
        [r"C:\Program Files\Editor\ed.exe", "-n"]
    );
    assert_eq!(
        split_command(r#"ed "unterminated arg"#),
        ["ed", "unterminated arg"]
    );
    assert!(split_command("   ").is_empty());
    // Shell syntax is not interpreted.
    assert_eq!(split_command("vim; rm -rf x"), ["vim;", "rm", "-rf", "x"]);
}

#[test]
fn editor_choice_order() {
    let by_ext = HashMap::from([("md".to_string(), "code --wait".to_string())]);
    let mut s = EditorSources {
        config_default: Some("nvim"),
        config_by_extension: Some(&by_ext),
        visual: Some("emacs -nw"),
        editor: Some("nano"),
        windows: false,
        edit_on_path: false,
    };
    assert_eq!(choose(&s, Some("md")), ["code", "--wait"]);
    assert_eq!(choose(&s, Some("rs")), ["nvim"]);
    s.config_default = Some("  ");
    assert_eq!(choose(&s, Some("rs")), ["emacs", "-nw"]);
    s.visual = None;
    assert_eq!(choose(&s, None), ["nano"]);
    s.editor = None;
    assert_eq!(choose(&s, None), ["vi"]);
    s.windows = true;
    assert_eq!(choose(&s, None), ["notepad"]);
    s.edit_on_path = true;
    assert_eq!(choose(&s, None), ["edit"]);
}

// ---------- search (P3-14) ----------

#[test]
fn search_wraps_and_ignores_case() {
    let names = ["..", "Alpha", "beta", "ALPHABET", "gamma"];
    assert_eq!(find(&names, 0, "alpha", Direction::Forward), Some(1));
    assert_eq!(
        find(&names, 1, "alpha", Direction::Forward),
        Some(1),
        "current match stays"
    );
    assert_eq!(find(&names, 2, "alpha", Direction::Forward), Some(3));
    assert_eq!(
        find(&names, 4, "alpha", Direction::Forward),
        Some(1),
        "wraps"
    );
    assert_eq!(find(&names, 2, "alpha", Direction::Backward), Some(1));
    assert_eq!(
        find(&names, 0, "alpha", Direction::Backward),
        Some(3),
        "wraps backward"
    );
    assert_eq!(find(&names, 0, "zzz", Direction::Forward), None);
    assert_eq!(find(&names, 0, "", Direction::Forward), None);
    assert_eq!(find(&[], 0, "a", Direction::Forward), None);
    assert_eq!(find(&names, 99, "gam", Direction::Forward), Some(4));
}

// ---------- targets (P3-1) ----------

fn pane_with(names: &[&str]) -> Pane {
    let dir = PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" });
    let listing = names
        .iter()
        .map(|n| FileEntry::new(dir.join(n), (*n).into(), EntryKind::File, false))
        .collect();
    let mut p = Pane::new(dir.clone());
    p.set_listing(dir, listing, CursorFocus::First);
    p
}

#[test]
fn targets_are_marks_or_cursor_never_parent() {
    let mut p = pane_with(&["a", "b", "c"]);
    assert_eq!(targets(&p), [p.current_dir.join("a")]);
    p.cursor_down();
    p.toggle_mark();
    p.cursor_down();
    p.toggle_mark();
    assert_eq!(
        targets(&p),
        [p.current_dir.join("b"), p.current_dir.join("c")]
    );
    let mut p = pane_with(&["a"]);
    p.cursor_home(); // [..]
    assert!(targets(&p).is_empty());
}
