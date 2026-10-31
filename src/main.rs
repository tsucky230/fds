//! `fds [path]` - starts in the given directory, or the current working directory (R-111-1).
//! `fds --version` / `fds --help` print and exit (R-111-2, R-111-3).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use fds::cli::{self, CliRequest};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let arg = match cli::parse_args(&args) {
        CliRequest::Version => {
            println!("{}", cli::version_text());
            return ExitCode::SUCCESS;
        }
        CliRequest::Help => {
            print_help();
            return ExitCode::SUCCESS;
        }
        CliRequest::Start(arg) => arg,
    };
    let start_dir = match start_dir(arg) {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("fds: {message}");
            return ExitCode::FAILURE;
        }
    };
    // The terminal is already restored inside run, so this prints to the normal screen.
    match fds::run::run(start_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fds: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Resolves the start directory before touching the terminal, so errors print normally.
fn start_dir(arg: Option<OsString>) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot get current directory: {e}"))?;
    fds::run::resolve_start_dir(arg, &cwd)
}

/// The help in the configured language (the same config and catalog as the filer uses).
fn print_help() {
    let config_dir = fds::settings::config_dir();
    let settings = fds::settings::load_settings(config_dir.as_deref());
    let catalog = fds::settings::load_catalog(&settings, config_dir.as_deref());
    let config_file = config_dir.map(|dir| dir.join("config.toml"));
    println!("{}", cli::help_text(&catalog, config_file.as_deref()));
}
