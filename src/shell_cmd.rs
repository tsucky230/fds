//! Turning what the user typed into a shell command line (R-412). Pure, apart from building the
//! `std::process::Command` at the end.
//!
//! The line goes through a shell because the user asked for pipes, wildcards and built-in
//! commands. What is typed is the user's own; only file names must never be read as shell syntax.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::open::Launch;

/// `cmd.exe` accepts 8191 characters; this leaves room for `cmd.exe /S /C ""`.
pub const CMD_LINE_LIMIT: usize = 8100;

/// The shell, which decides how names are quoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    /// `/bin/sh -c <line>`. `$SHELL` is not used: fish and csh quote differently.
    Posix,
    /// `%COMSPEC% /S /C "<line>"`.
    Cmd,
}

impl ShellKind {
    /// Cmd on Windows, Posix elsewhere.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Cmd
        } else {
            Self::Posix
        }
    }

    /// What a name starting with `-` gets in front of it, and a program in this folder too.
    fn here(self) -> &'static str {
        match self {
            Self::Posix => "./",
            Self::Cmd => ".\\",
        }
    }

    fn quote(self, text: &str) -> String {
        match self {
            Self::Posix => format!("'{}'", text.replace('\'', r"'\''")),
            Self::Cmd => format!("\"{text}\""),
        }
    }
}

/// Why a typed line was not run. The line stays in the input box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Nothing to run: empty, or the first word is `{file}` / `{files}`. Without this, Enter on
    /// the line `x` prefills would run the first marked file as a program.
    NoCommand,
    /// `{file}` / `{files}` inside quotes the user typed: expanding it there would break out of
    /// them.
    QuotedPlaceholder,
    /// `{files}` is several words, so it cannot be part of a word (`{files}.bak`).
    FilesNotAWord,
    /// A placeholder with nothing for it to stand for.
    NoTarget,
    /// The name is not valid UTF-8, so it cannot be put in a command line.
    NonUtf8 { name: String },
    /// `cmd.exe` would read part of this name as syntax even inside quotes (`%`), or it cannot be
    /// quoted at all (`"`, line breaks).
    UnsafeName { name: String },
    /// Over `CMD_LINE_LIMIT`. Never shortened: that would silently drop files.
    TooLong,
    /// `cmd.exe` cannot run in a UNC folder: it warns and moves to the Windows folder, so the
    /// command would run somewhere else.
    UncFolder,
}

/// Replaces the placeholders in `template`.
///
/// Behavior:
/// - `{file}` -> `cursor`, `{files}` -> every one of `targets` (R-400), space separated. Exactly
///   these two spellings. Any other brace (`awk '{print $1}'`, `{File}`) and the rest of the line
///   stay byte for byte. A line without placeholders is returned as it is, and nothing is added.
/// - Names are relative to the listed folder. A name starting with `-` becomes `./-a` (`.\-a` on
///   Cmd) so that it is not read as an option. Each name is quoted: Posix `'a'` (a `'` becomes
///   `'\''`), Cmd `"a"`.
/// - Quotes are tracked per shell: Posix `'`, `"` and `\` (outside single quotes), Cmd `"` and `^`
///   (outside quotes). An unclosed quote counts as open to the end of the line.
/// - `{files}` must be a word of its own: start or end of line, white space or one of `|&;()<>`
///   on both sides. `{file}` may be part of a word (`{file}.bak`).
///
/// Errors, in this order: `NoCommand`, `QuotedPlaceholder`, `FilesNotAWord`, `NoTarget`,
/// `NonUtf8`, `UnsafeName`, `TooLong`. Names that no placeholder uses are not looked at.
/// `TooLong` is for Cmd only and counts UTF-16 units of the result. Posix has no limit here; a
/// line the OS refuses comes back as a failure to start the shell.
pub fn expand(
    template: &str,
    cursor: Option<&OsStr>,
    targets: &[OsString],
    kind: ShellKind,
) -> Result<String, CommandError> {
    let start = template.trim_start();
    if start.is_empty() || start.starts_with(FILE) || start.starts_with(FILES) {
        return Err(CommandError::NoCommand);
    }
    let found = find_placeholders(template, kind);
    if found.iter().any(|p| p.quoted) {
        return Err(CommandError::QuotedPlaceholder);
    }
    if found.iter().any(|p| p.files && !is_a_word(template, p)) {
        return Err(CommandError::FilesNotAWord);
    }
    let uses_files = found.iter().any(|p| p.files);
    let uses_file = found.iter().any(|p| !p.files);
    let cursor = if uses_file {
        Some(cursor.ok_or(CommandError::NoTarget)?)
    } else {
        None
    };
    if uses_files && targets.is_empty() {
        return Err(CommandError::NoTarget);
    }
    let files: &[OsString] = if uses_files { targets } else { &[] };
    for name in cursor
        .into_iter()
        .chain(files.iter().map(OsString::as_os_str))
    {
        utf8(name)?;
    }
    let cursor_word = cursor.map(|name| word(name, kind)).transpose()?;
    let files_word = files
        .iter()
        .map(|name| word(name, kind))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");

    let mut line = String::with_capacity(template.len());
    let mut copied = 0;
    for placeholder in &found {
        line.push_str(&template[copied..placeholder.start]);
        line.push_str(if placeholder.files {
            &files_word
        } else {
            cursor_word.as_deref().unwrap_or_default()
        });
        copied = placeholder.end;
    }
    line.push_str(&template[copied..]);
    if kind == ShellKind::Cmd && line.encode_utf16().count() > CMD_LINE_LIMIT {
        return Err(CommandError::TooLong);
    }
    Ok(line)
}

const FILE: &str = "{file}";
const FILES: &str = "{files}";

struct Placeholder {
    start: usize,
    end: usize,
    files: bool,
    quoted: bool,
}

/// Every placeholder, with whether a quote the user typed is open around it.
fn find_placeholders(template: &str, kind: ShellKind) -> Vec<Placeholder> {
    #[derive(PartialEq)]
    enum Quote {
        None,
        Single,
        Double,
    }
    let bytes = template.as_bytes();
    let mut quote = Quote::None;
    let mut found = Vec::new();
    let mut i = 0;
    // Only ASCII is compared, and no UTF-8 continuation byte is ASCII, so stepping by bytes is safe.
    while i < bytes.len() {
        let rest = &template.as_bytes()[i..];
        let spelled = [(FILE, false), (FILES, true)]
            .into_iter()
            .find(|(text, _)| rest.starts_with(text.as_bytes()));
        if let Some((text, files)) = spelled {
            found.push(Placeholder {
                start: i,
                end: i + text.len(),
                files,
                quoted: quote != Quote::None,
            });
            i += text.len();
            continue;
        }
        match (kind, &quote, bytes[i]) {
            (ShellKind::Posix, Quote::None | Quote::Double, b'\\')
            | (ShellKind::Cmd, Quote::None, b'^') => i += 1,
            (ShellKind::Posix, Quote::None, b'\'') => quote = Quote::Single,
            (ShellKind::Posix, Quote::Single, b'\'') => quote = Quote::None,
            (_, Quote::None, b'"') => quote = Quote::Double,
            (_, Quote::Double, b'"') => quote = Quote::None,
            _ => {}
        }
        i += 1;
    }
    found
}

fn is_a_word(template: &str, placeholder: &Placeholder) -> bool {
    let boundary = |c: Option<char>| c.is_none_or(|c| c.is_whitespace() || "|&;()<>".contains(c));
    boundary(template[..placeholder.start].chars().next_back())
        && boundary(template[placeholder.end..].chars().next())
}

fn utf8(name: &OsStr) -> Result<&str, CommandError> {
    name.to_str().ok_or_else(|| CommandError::NonUtf8 {
        name: name.to_string_lossy().into_owned(),
    })
}

/// `cmd.exe` expands `%` inside quotes too, and a `"` or a line break ends the quoting.
fn check_name(name: &str, kind: ShellKind) -> Result<(), CommandError> {
    if kind == ShellKind::Cmd && name.contains(['%', '"', '\n', '\r']) {
        return Err(CommandError::UnsafeName { name: name.into() });
    }
    Ok(())
}

/// One name as one word of the line.
fn word(name: &OsStr, kind: ShellKind) -> Result<String, CommandError> {
    let name = utf8(name)?;
    check_name(name, kind)?;
    Ok(if name.starts_with('-') {
        kind.quote(&format!("{}{name}", kind.here()))
    } else {
        kind.quote(name)
    })
}

/// What `x` puts in the box for a program under the cursor: its path from the listed folder,
/// quoted only when it needs it, and a space to type the arguments after.
///
/// `build.sh` -> `./build.sh `, `my tool.sh` -> `'./my tool.sh' `; on Cmd `.\build.bat ` and
/// `".\my tool.exe" `. Letters, digits and `._-+:@` need no quotes, nor do `/=` on Posix (`=` is a
/// separator for `cmd.exe`) and `\` on Cmd (an escape for `sh`). Errors: `NonUtf8`, `UnsafeName` as
/// in `expand`.
pub fn program_prefill(name: &OsStr, kind: ShellKind) -> Result<String, CommandError> {
    let name = utf8(name)?;
    check_name(name, kind)?;
    let path = format!("{}{name}", kind.here());
    let extra = match kind {
        ShellKind::Posix => "/=",
        ShellKind::Cmd => "\\",
    };
    let plain = path
        .chars()
        .all(|c| c.is_alphanumeric() || "._-+:@".contains(c) || extra.contains(c));
    Ok(if plain {
        format!("{path} ")
    } else {
        format!("{} ", kind.quote(&path))
    })
}

/// The folder of a line that does nothing but change folder, which a child process could not do
/// for FDS. `None`: not such a line, so it goes to the shell.
///
/// Behavior:
/// - The line trimmed is `cd`, white space and a path. Case matters on Posix, not on Cmd.
/// - One pair of quotes around the whole path is removed: `'` and `"` on Posix, `"` on Cmd. On
///   Cmd a leading `/d` is skipped.
/// - Anything the shell would have to interpret gives `None`:
///   - Posix: `& | ; < > ( ) $ backtick * ? ~ \` and white space outside quotes, `$ backtick \`
///     inside `"`. Inside `'` every character is literal. The path `-` also gives `None`.
///   - Cmd: `& | < > ^ %` anywhere, white space is part of the path.
/// - `cd` with no path, and the empty path, give `None`.
pub fn parse_cd(line: &str, kind: ShellKind) -> Option<String> {
    let (word, rest) = line.trim().split_once(char::is_whitespace)?;
    let path = rest.trim_start();
    match kind {
        ShellKind::Posix => (word == "cd").then(|| posix_cd_path(path))?,
        ShellKind::Cmd => word.eq_ignore_ascii_case("cd").then(|| cmd_cd_path(path))?,
    }
    .map(str::to_string)
}

/// The inside of `quote...quote`, if the whole text is that and the inside is not empty.
fn inside_quotes(text: &str, quote: char) -> Option<&str> {
    let inside = text.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inside.is_empty() && !inside.contains(quote)).then_some(inside)
}

fn posix_cd_path(path: &str) -> Option<&str> {
    match path.chars().next()? {
        '\'' => inside_quotes(path, '\''),
        '"' => inside_quotes(path, '"').filter(|inside| !inside.contains(['$', '`', '\\'])),
        _ => {
            let shell_syntax = |c: char| c.is_whitespace() || "&|;<>()$`*?~\\'\"".contains(c);
            (path != "-" && !path.contains(shell_syntax)).then_some(path)
        }
    }
}

fn cmd_cd_path(path: &str) -> Option<&str> {
    let path = match path.split_once(char::is_whitespace) {
        Some((switch, rest)) if switch.eq_ignore_ascii_case("/d") => rest.trim_start(),
        _ => path,
    };
    if path.is_empty() || path.contains(['&', '|', '<', '>', '^', '%']) {
        return None;
    }
    if path.starts_with('"') {
        inside_quotes(path, '"')
    } else {
        (!path.contains('"')).then_some(path)
    }
}

/// `path` from `parse_cd` on top of `current`, with `.` and `..` removed without touching the disk.
/// `..` at the root stays at the root.
pub fn resolve_cd(current: &Path, path: &str) -> PathBuf {
    let mut resolved = PathBuf::new();
    for part in current.join(path).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    resolved
}

/// `UncFolder` for a folder starting with `\\` on Cmd (`\\server\share`, `\\?\C:\x`), else Ok.
pub fn check_folder(dir: &Path, kind: ShellKind) -> Result<(), CommandError> {
    if kind == ShellKind::Cmd && dir.to_string_lossy().starts_with(r"\\") {
        return Err(CommandError::UncFolder);
    }
    Ok(())
}

/// Whether `x` should offer `name` as a program. `RunDirectly` always; on Cmd also `.bat` and
/// `.cmd`, which `open::classify` leaves to the OS. Other `RunThroughOs` files (`.lnk`,
/// `.desktop`) and documents are not programs the shell can start.
pub fn runnable_in_shell(name: &str, launch: Launch, kind: ShellKind) -> bool {
    let batch = || {
        Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bat") || e.eq_ignore_ascii_case("cmd"))
    };
    match launch {
        Launch::RunDirectly => true,
        Launch::RunThroughOs => kind == ShellKind::Cmd && batch(),
        Launch::Document => false,
    }
}

/// The process for `line`. The caller sets the folder and how the output is connected.
///
/// On Windows the line is passed raw: `cmd.exe /S /C "..."` has its own quoting rules, which the
/// usual escaping of arguments would break.
pub fn shell_command(line: &str, kind: ShellKind) -> Command {
    match kind {
        ShellKind::Posix => {
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(line);
            command
        }
        ShellKind::Cmd => cmd_command(line),
    }
}

#[cfg(windows)]
fn cmd_command(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(std::env::var_os("COMSPEC").unwrap_or("cmd.exe".into()));
    command.raw_arg(format!("/S /C \"{line}\""));
    command
}

/// Never used in practice: `ShellKind::current` is Posix here.
#[cfg(not(windows))]
fn cmd_command(line: &str) -> Command {
    let mut command = Command::new("cmd.exe");
    command.args(["/S", "/C", line]);
    command
}
