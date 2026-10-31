//! Command line (R-111-1 to R-111-3): parsing, the version and help texts, and the binary itself.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use fds::cli::{CliRequest, help_text, parse_args, version_text};
use fds::command::KEY_HELP;
use fds::i18n::{Catalog, builtin, parse_catalog};

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn japanese() -> Catalog {
    let ja = parse_catalog(builtin("ja").unwrap()).unwrap();
    let en = parse_catalog(builtin("en").unwrap()).unwrap();
    Catalog::new("ja", ja, en)
}

#[test]
fn arguments_choose_what_to_do() {
    let start = |p: &str| CliRequest::Start(Some(OsString::from(p)));
    // (arguments, request)
    for (list, request) in [
        (vec![], CliRequest::Start(None)),
        (vec!["--version"], CliRequest::Version),
        (vec!["-V"], CliRequest::Version),
        (vec!["--help"], CliRequest::Help),
        (vec!["-h"], CliRequest::Help),
        (vec!["--version", "dir"], CliRequest::Version),
        (vec!["-h", "--version"], CliRequest::Help),
        // Only the exact spellings are options; everything else is a path, as before.
        (vec!["-v"], start("-v")),
        (vec!["--Version"], start("--Version")),
        (vec!["-H"], start("-H")),
        (vec!["--versions"], start("--versions")),
        (vec!["dir"], start("dir")),
        (vec!["dir", "--version"], start("dir")),
        (vec!["--unknown"], start("--unknown")),
        // "--" ends the options: the next argument is the path.
        (vec!["--", "--version"], start("--version")),
        (vec!["--", "-h"], start("-h")),
        (vec!["--", "dir", "x"], start("dir")),
        (vec!["--"], CliRequest::Start(None)),
    ] {
        assert_eq!(parse_args(&args(&list)), request, "{list:?}");
    }
}

#[test]
fn the_version_text_is_the_package_version() {
    let text = version_text();
    assert_eq!(text, format!("fds {}", env!("CARGO_PKG_VERSION")));
    let version = text.strip_prefix("fds ").unwrap();
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "{version}");
    assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{version}");
}

#[test]
fn the_help_lists_usage_every_key_and_the_config_file() {
    let catalog = Catalog::english();
    let config = Path::new("/home/someone/.config/fds/config.toml");
    let text = help_text(&catalog, Some(config));
    assert!(text.starts_with(&version_text()), "{text}");
    for needle in ["Usage", "fds [path]", "--version", "-V", "--help", "-h"] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
    for (keys, action) in KEY_HELP {
        assert!(text.contains(keys), "{keys}\n{text}");
        assert!(
            text.contains(&catalog.text(action, &[])),
            "{action}\n{text}"
        );
    }
    assert!(
        text.contains("/home/someone/.config/fds/config.toml"),
        "{text}"
    );
    let none = help_text(&catalog, None);
    assert!(none.contains("no config folder"), "{none}");
}

#[test]
fn the_help_follows_the_language() {
    let catalog = japanese();
    let text = help_text(&catalog, None);
    assert!(text.contains("使い方"), "{text}");
    assert!(
        text.contains("fds [path]") && text.contains("--version"),
        "{text}"
    );
    assert!(
        text.contains(&catalog.text("help.cursor_up", &[])),
        "{text}"
    );
}

/// Runs the built binary with its config isolated from the user's.
fn fds(arguments: &[&str], cwd: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fds"))
        .args(arguments)
        .current_dir(cwd)
        .env("APPDATA", cwd)
        .env("XDG_CONFIG_HOME", cwd)
        .output()
        .unwrap()
}

#[test]
fn the_binary_prints_the_version_and_help_and_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    for flag in ["--version", "-V"] {
        let out = fds(&[flag], tmp.path());
        assert!(out.status.success(), "{flag}: {:?}", out.status);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            version_text(),
            "{flag}"
        );
    }
    for flag in ["--help", "-h"] {
        let out = fds(&[flag], tmp.path());
        assert!(out.status.success(), "{flag}: {:?}", out.status);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.starts_with(&version_text()), "{flag}: {stdout}");
        assert!(
            stdout.contains("fds [path]") && stdout.contains("--version"),
            "{flag}"
        );
    }
}

#[test]
fn the_binary_still_rejects_paths_that_are_not_folders() {
    let tmp = tempfile::tempdir().unwrap();
    for list in [vec!["missing-folder"], vec!["--", "--version"], vec!["-v"]] {
        let out = fds(&list, tmp.path());
        assert_eq!(out.status.code(), Some(1), "{list:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("not a directory"), "{list:?}: {stderr}");
    }
}
