//! Command line (R-111-1 to R-111-3): `fds [path]`, `fds --version`, `fds --help`.
//! Pure logic; `main.rs` prints the texts and exits.

use std::ffi::OsString;
use std::path::Path;

use crate::command::KEY_HELP;
use crate::i18n::Catalog;

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliRequest {
    /// Start the filer in the `[path]` argument (None: the current folder).
    Start(Option<OsString>),
    /// `--version` / `-V`: print `version_text` and exit 0.
    Version,
    /// `--help` / `-h`: print `help_text` and exit 0.
    Help,
}

/// Reads the arguments after the program name.
///
/// Behavior:
/// - No arguments -> `Start(None)`.
/// - The first argument exactly "--version" or "-V" -> `Version`; exactly "--help" or "-h" ->
///   `Help` (case-sensitive; later arguments are ignored).
/// - The first argument "--": the next one is the path even if it looks like an option
///   (`fds -- --version` opens a folder named "--version"); nothing after "--" -> `Start(None)`.
/// - Anything else -> `Start(Some(first argument))`; later arguments are ignored, as before.
pub fn parse_args(args: &[OsString]) -> CliRequest {
    let Some(first) = args.first() else {
        return CliRequest::Start(None);
    };
    match first.to_str() {
        Some("--version" | "-V") => CliRequest::Version,
        Some("--help" | "-h") => CliRequest::Help,
        Some("--") => CliRequest::Start(args.get(1).cloned()),
        _ => CliRequest::Start(Some(first.clone())),
    }
}

/// "fds 0.5.0": the package version.
pub fn version_text() -> String {
    format!("fds {}", env!("CARGO_PKG_VERSION"))
}

/// The `--help` text in the catalog's language: the version line, the usage lines, every key of
/// `command::KEY_HELP` with its action, and where the config file is (`config_file`, or a note
/// that no config folder was found).
pub fn help_text(catalog: &Catalog, config_file: Option<&Path>) -> String {
    let t = |key: &str| catalog.text(key, &[]);
    let mut out = format!(
        "{} - {}\n\n{}:\n",
        version_text(),
        t("cli.tagline"),
        t("cli.usage")
    );
    for (usage, key) in [
        ("fds [path]", "cli.start"),
        ("fds --version, -V", "cli.version"),
        ("fds --help, -h", "cli.help"),
    ] {
        out.push_str(&format!("  {usage:<20} {}\n", t(key)));
    }
    out.push_str(&format!("\n{}:\n", t("cli.keys")));
    for (keys, action) in KEY_HELP {
        out.push_str(&format!("  {keys:<22} {}\n", t(action)));
    }
    out.push('\n');
    out.push_str(&match config_file {
        Some(path) => catalog.text("cli.config", &[("path", &path.display().to_string())]),
        None => t("cli.config_none"),
    });
    out.push_str(&format!(
        "\n\n{}\n{}",
        t("legal.notice"),
        t("legal.license")
    ));
    out
}
