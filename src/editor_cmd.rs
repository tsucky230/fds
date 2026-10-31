//! Which external editor to run and how to split its command line (P3-15, R-1200). Pure logic.
//! The command is never passed to a shell (design prompt §25, §32).

use std::collections::HashMap;

/// Splits a command line into words without a shell.
///
/// Behavior:
/// - Words are separated by one or more ASCII whitespace characters.
/// - Double quotes group a word and are removed: `code --wait "C:\Program Files\x"` ->
///   ["code", "--wait", "C:\Program Files\x"]. Backslashes are literal (Windows paths).
/// - An unterminated quote extends to the end.
/// - Empty or whitespace-only input -> empty Vec.
pub fn split_command(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        if c == '"' {
            quoted = !quoted;
            started = true;
        } else if c.is_ascii_whitespace() && !quoted {
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(c);
            started = true;
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Inputs that decide the editor. All strings are raw config / environment values.
#[derive(Debug, Clone, Default)]
pub struct EditorSources<'a> {
    /// `[editor] command` from config.
    pub config_default: Option<&'a str>,
    /// `[editor.by_extension]` from config: lowercase extension -> command.
    pub config_by_extension: Option<&'a HashMap<String, String>>,
    pub visual: Option<&'a str>,
    pub editor: Option<&'a str>,
    pub windows: bool,
    /// Whether an `edit` executable is on PATH (Windows only).
    pub edit_on_path: bool,
}

/// Returns the argv to run for `file_extension` (lowercase, without the dot), not including the
/// file path (the caller appends it as the last argument).
///
/// Order (the first non-empty after `split_command` wins):
/// 1. `config_by_extension[file_extension]`
/// 2. `config_default`
/// 3. `visual`
/// 4. `editor`
/// 5. OS default: Unix ["vi"]; Windows ["edit"] if `edit_on_path`, otherwise ["notepad"].
pub fn choose(sources: &EditorSources<'_>, file_extension: Option<&str>) -> Vec<String> {
    let extension =
        file_extension.and_then(|ext| sources.config_by_extension?.get(ext).map(String::as_str));
    for line in [
        extension,
        sources.config_default,
        sources.visual,
        sources.editor,
    ]
    .into_iter()
    .flatten()
    {
        let argv = split_command(line);
        if !argv.is_empty() {
            return argv;
        }
    }
    vec![
        if !sources.windows {
            "vi"
        } else if sources.edit_on_path {
            "edit"
        } else {
            "notepad"
        }
        .to_string(),
    ]
}
