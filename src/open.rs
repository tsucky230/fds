//! Opening a file outside FDS (R-110-3, R-110-4): the OS's default application, a directly run
//! program, or the application set in `[open.by_extension]`. Never through a shell.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::editor_cmd::split_command;
use crate::settings::OpenRule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Other
        }
    }
}

/// What opening a file would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// The OS hands it to an application. No question asked.
    Document,
    /// It is a program: asked first, then run in the terminal.
    RunDirectly,
    /// It runs, but only the OS can start it (`.bat`, `.app`, ...): asked first.
    RunThroughOs,
}

const WINDOWS_RUNNABLE: &[&str] = &[
    "bat",
    "cmd",
    "ps1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "ws",
    "msi",
    "msp",
    "mst",
    "scr",
    "pif",
    "hta",
    "cpl",
    "msc",
    "lnk",
    "url",
    "reg",
    "inf",
    "jar",
    "application",
    "appref-ms",
    "gadget",
];
const MACOS_RUNNABLE: &[&str] = &[
    "app", "command", "tool", "pkg", "mpkg", "terminal", "workflow", "action", "jar",
];
const OTHER_RUNNABLE: &[&str] = &["desktop", "appimage", "run", "jar"];
/// Count as programs only with the execute permission.
const SCRIPTS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "csh", "ksh", "py", "pl", "rb", "php", "js", "lua",
];

/// Lowercase extension as the OS sees it. Windows ignores trailing dots and spaces in names.
fn extension(name: &str, os: Os) -> Option<String> {
    let name = if os == Os::Windows {
        name.trim_end_matches(['.', ' '])
    } else {
        name
    };
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// Decides from the name and the execute permission (pure).
///
/// Behavior:
/// - Windows: `.exe` / `.com` -> `RunDirectly`; `WINDOWS_RUNNABLE` -> `RunThroughOs`.
/// - Others: the OS's runnable extensions -> `RunDirectly` with the execute permission, else
///   `RunThroughOs`. With the permission, no extension or a script extension -> `RunDirectly`.
/// - Everything else -> `Document`. A `.pdf` with the execute permission (common on FAT drives)
///   stays a document: the OS opens it by its extension.
pub fn classify_name(name: &str, os: Os, execute_permission: bool) -> Launch {
    let ext = extension(name, os);
    let ext = ext.as_deref();
    let listed = |list: &[&str]| ext.is_some_and(|e| list.contains(&e));
    match os {
        Os::Windows if matches!(ext, Some("exe" | "com")) => Launch::RunDirectly,
        Os::Windows if listed(WINDOWS_RUNNABLE) => Launch::RunThroughOs,
        Os::Windows => Launch::Document,
        Os::MacOs | Os::Other => {
            let runnable = if os == Os::MacOs {
                MACOS_RUNNABLE
            } else {
                OTHER_RUNNABLE
            };
            if execute_permission && (ext.is_none() || listed(runnable) || listed(SCRIPTS)) {
                Launch::RunDirectly
            } else if listed(runnable) {
                Launch::RunThroughOs
            } else {
                Launch::Document
            }
        }
    }
}

/// `classify_name` for a file on disk. Links are judged by what they point at.
pub fn classify(path: &Path) -> Launch {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target
        .file_name()
        .unwrap_or(target.as_os_str())
        .to_string_lossy();
    classify_name(&name, Os::current(), has_execute_permission(&target))
}

#[cfg(unix)]
fn has_execute_permission(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn has_execute_permission(_path: &Path) -> bool {
    false
}

/// An application from `[open.by_extension]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApp {
    pub argv: Vec<String>,
    /// Give it the terminal and come back when it ends. False: start it and carry on.
    pub wait: bool,
}

/// The settings by lowercase extension. Rules without a command are left out.
pub fn apps_from_settings(rules: &HashMap<String, OpenRule>) -> HashMap<String, OpenApp> {
    rules
        .iter()
        .filter_map(|(extension, rule)| {
            let (command, wait) = match rule {
                OpenRule::Command(command) => (command, true),
                OpenRule::Detailed { command, wait } => (command, *wait),
            };
            let argv = split_command(command);
            let extension = extension.trim_start_matches('.').to_ascii_lowercase();
            (!argv.is_empty() && !extension.is_empty())
                .then_some((extension, OpenApp { argv, wait }))
        })
        .collect()
}

/// Runs `command` without a terminal; `Err` when it does not start or fails.
fn finish_detached(mut command: Command, program: &str, check_exit: bool) -> Result<(), String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !check_exit || output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(match stderr.lines().next() {
        Some(line) => format!("{program}: {line}"),
        None => format!("{program}: {}", output.status),
    })
}

/// Opens `path` with the OS's default application. Blocks until the opener ends: call it on a
/// thread. Explorer's exit code says nothing, so only macOS and others check it.
pub fn open_with_os(path: &Path) -> Result<(), String> {
    let program = match Os::current() {
        Os::Windows => "explorer",
        Os::MacOs => "open",
        Os::Other => "xdg-open",
    };
    let mut command = Command::new(program);
    command.arg(path);
    finish_detached(command, program, Os::current() != Os::Windows)
}

/// Runs a `wait = false` application on `path`. Blocks until it ends: call it on a thread.
pub fn run_app_detached(app: &OpenApp, path: &Path) -> Result<(), String> {
    let Some((program, args)) = app.argv.split_first() else {
        return Err("no command".to_string());
    };
    let mut command = Command::new(program);
    command.args(args).arg(path);
    finish_detached(command, program, true)
}
