//! External command providers (R-802, Phase 4, ADR 0006): a pack declares a command; FDS runs it
//! on the file and shows its standard output as text.
//!
//! Safety rules: no shell (the command is an argv list); the file path is passed only as a whole
//! argument and always absolute (so a name such as `-rf` is never read as an option); standard
//! input is closed; output is bounded; the process is killed on timeout or when the user cancels;
//! the text goes through the same decoding and control-character sanitizing as the Text viewer.

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::detect::Sniff;
use super::{Document, TextDoc, ViewError, ViewerKind, ViewerProvider, encoding, text};
use crate::pack::ProviderSpec;

/// The placeholder replaced by the file path. Only an argument that is exactly this is replaced.
pub const PATH_PLACEHOLDER: &str = "{path}";
/// Priority of an external provider for its extensions: above every built-in viewer, so the
/// provider the user installed is the default and Tab still reaches the built-in ones (C31).
pub const EXTERNAL_PRIORITY: u8 = 200;
/// Default and allowed range of `[providers] timeout_secs`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const MIN_TIMEOUT_SECS: u64 = 1;
pub const MAX_TIMEOUT_SECS: u64 = 300;
/// Standard output is read up to this many bytes (the same limit as the Text viewer).
pub const MAX_STDOUT: usize = super::TEXT_LIMIT as usize;
/// Standard error is kept up to this many bytes for the error message.
pub const MAX_STDERR: usize = 4 * 1024;

/// `[providers] timeout_secs`: None -> `DEFAULT_TIMEOUT`; otherwise clamped to
/// `MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS` (0 becomes 1).
pub fn timeout_from_setting(secs: Option<u64>) -> Duration {
    secs.map(|secs| Duration::from_secs(secs.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)))
        .unwrap_or(DEFAULT_TIMEOUT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub timeout: Duration,
    pub max_stdout: usize,
    pub max_stderr: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutput {
    /// At most `max_stdout` bytes.
    pub stdout: Vec<u8>,
    /// True when the output was cut at `max_stdout` (the process is then killed; that is not
    /// a failure).
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The program could not be found or started (the program as written).
    NotFound(String),
    /// Killed after the timeout.
    Timeout(Duration),
    /// Killed because `cancel` was set.
    Cancelled,
    /// Exited with a failure status. `stderr`: the first `max_stderr` bytes, lossy UTF-8, trimmed.
    Failed { code: Option<i32>, stderr: String },
    /// Any other I/O error (OS text).
    Io(String),
}

impl ProviderError {
    /// English text for the status line, e.g. "provider not found: pdftotext",
    /// "provider timed out after 10 s", "provider failed (exit 1): <stderr>".
    pub fn message(&self) -> String {
        match self {
            Self::NotFound(program) => format!("provider not found: {program}"),
            Self::Timeout(duration) => format!("provider timed out after {} s", duration.as_secs()),
            Self::Cancelled => "provider cancelled".to_string(),
            Self::Failed { code, stderr } => {
                let status =
                    code.map_or_else(|| "signal".to_string(), |code| format!("exit {code}"));
                if stderr.is_empty() {
                    format!("provider failed ({status})")
                } else {
                    format!("provider failed ({status}): {stderr}")
                }
            }
            Self::Io(error) => format!("provider I/O error: {error}"),
        }
    }
}

/// The argv for `command` on `path`: every element exactly equal to `PATH_PLACEHOLDER` becomes
/// the absolute path of `path` (`std::path::absolute`); every other element is kept as is
/// (`--in={path}` stays literal). Never involves a shell.
pub fn build_argv(command: &[String], path: &Path) -> Vec<OsString> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|dir| dir.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    });
    command
        .iter()
        .map(|part| {
            if part == PATH_PLACEHOLDER {
                absolute.as_os_str().to_os_string()
            } else {
                OsString::from(part)
            }
        })
        .collect()
}

/// Runs `command` on `path` (see `build_argv`) and collects its standard output.
///
/// - Standard input is null; stdout and stderr are piped and read on their own threads.
/// - Stops reading stdout at `limits.max_stdout` bytes, kills the process and returns
///   `Ok(truncated: true)`.
/// - Polls about every 20 ms: `cancel` set -> kill, `Err(Cancelled)`; `limits.timeout` passed ->
///   kill, `Err(Timeout)`. A killed process is always waited for (no zombies).
/// - The program cannot be started (not found, not executable) -> `Err(NotFound(program))`.
/// - Exit status not success (and not truncated) -> `Err(Failed { code, stderr })`.
pub fn run_provider(
    command: &[String],
    path: &Path,
    limits: &RunLimits,
    cancel: &AtomicBool,
) -> Result<ProviderOutput, ProviderError> {
    let argv = build_argv(command, path);
    let Some(program) = command.first() else {
        return Err(ProviderError::NotFound(String::new()));
    };
    let Some((program_arg, args)) = argv.split_first() else {
        return Err(ProviderError::NotFound(program.clone()));
    };
    let mut child = Command::new(program_arg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
                ProviderError::NotFound(program.clone())
            }
            _ => ProviderError::Io(error.to_string()),
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProviderError::Io("stdout pipe unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ProviderError::Io("stderr pipe unavailable".into()))?;
    let truncated = Arc::new(AtomicBool::new(false));
    let stdout_limit = limits.max_stdout;
    let stdout_truncated = Arc::clone(&truncated);
    let stdout_reader = std::thread::spawn(move || {
        // Read one byte past the limit: output of exactly `limit` bytes is not cut.
        let mut bytes = Vec::new();
        let result = stdout
            .take((stdout_limit as u64).saturating_add(1))
            .read_to_end(&mut bytes);
        if result.is_ok() && bytes.len() > stdout_limit {
            bytes.truncate(stdout_limit);
            stdout_truncated.store(true, Ordering::Relaxed);
        }
        result.map(|_| bytes)
    });
    let stderr_limit = limits.max_stderr;
    let stderr_reader = std::thread::spawn(move || {
        let mut reader = stderr;
        let mut kept = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            let remaining = stderr_limit.saturating_sub(kept.len());
            kept.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        Ok::<_, std::io::Error>(kept)
    });

    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProviderError::Cancelled);
        }
        if truncated.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        if started.elapsed() >= limits.timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProviderError::Timeout(limits.timeout));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProviderError::Io(error.to_string()));
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| ProviderError::Io("stdout reader panicked".into()))?
        .map_err(|error| ProviderError::Io(error.to_string()))?;
    if truncated.load(Ordering::Relaxed) {
        return Ok(ProviderOutput {
            stdout,
            truncated: true,
        });
    }
    let stderr = stderr_reader
        .join()
        .map_err(|_| ProviderError::Io("stderr reader panicked".into()))?
        .map_err(|error| ProviderError::Io(error.to_string()))?;
    if let Some(status) = status
        && !status.success()
    {
        return Err(ProviderError::Failed {
            code: status.code(),
            stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
        });
    }
    Ok(ProviderOutput {
        stdout,
        truncated: false,
    })
}

/// A `ViewerProvider` backed by a pack's command.
#[derive(Debug)]
pub struct ExternalProvider {
    spec: ProviderSpec,
    kind: ViewerKind,
    limits: RunLimits,
}

impl ExternalProvider {
    /// `kind` is `ViewerKind::Custom(label)`; the label is leaked once to get `&'static str`
    /// (a handful of providers, loaded once at startup).
    pub fn new(spec: ProviderSpec, timeout: Duration) -> Self {
        let kind = ViewerKind::Custom(Box::leak(spec.label.clone().into_boxed_str()));
        Self {
            spec,
            kind,
            limits: RunLimits {
                timeout,
                max_stdout: MAX_STDOUT,
                max_stderr: MAX_STDERR,
            },
        }
    }
}

impl ViewerProvider for ExternalProvider {
    fn kind(&self) -> ViewerKind {
        self.kind
    }

    /// `EXTERNAL_PRIORITY` when the file's extension (lowercase) is one of the spec's.
    fn accepts(&self, sniff: &Sniff) -> Option<u8> {
        sniff
            .extension
            .as_ref()
            .filter(|extension| self.spec.extensions.contains(extension))
            .map(|_| EXTERNAL_PRIORITY)
    }

    fn load(&self, path: &Path, sniff: &Sniff) -> Result<Document, ViewError> {
        self.load_cancellable(path, sniff, None, &AtomicBool::new(false))
    }

    fn load_with_encoding(
        &self,
        path: &Path,
        sniff: &Sniff,
        encoding: Option<&'static encoding_rs::Encoding>,
    ) -> Result<Document, ViewError> {
        self.load_cancellable(path, sniff, encoding, &AtomicBool::new(false))
    }

    /// Runs the command; the output becomes `Document::Lines` through
    /// `encoding::decode_with(stdout, encoding)` and `text::to_lines` (control characters are
    /// made harmless there, C29). `truncated` = the output was cut; `file_len` = `sniff.len`;
    /// `encoding` = the decoded encoding's name. Errors -> `ViewError::Io(error.message())`.
    fn load_cancellable(
        &self,
        path: &Path,
        sniff: &Sniff,
        encoding: Option<&'static encoding_rs::Encoding>,
        cancel: &AtomicBool,
    ) -> Result<Document, ViewError> {
        let output = run_provider(&self.spec.command, path, &self.limits, cancel)
            .map_err(|error| ViewError::Io(error.message()))?;
        let (decoded, name) = encoding::decode_with(&output.stdout, encoding);
        Ok(Document::Lines(TextDoc {
            lines: text::to_lines(&decoded),
            encoding: name.to_string(),
            truncated: output.truncated,
            file_len: sniff.len,
        }))
    }
}
