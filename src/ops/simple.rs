//! Rename and mkdir: short, synchronous operations (P3-12).

use std::path::{Path, PathBuf};

use super::name::NameError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleError {
    InvalidName(NameError),
    /// An entry with that name already exists (never overwritten).
    Exists(PathBuf),
    Io(String),
}

/// Renames `dir/old` to `dir/new` (P3-12).
///
/// Behavior:
/// - `new` is validated with `name::validate(new, cfg!(windows))`; invalid -> InvalidName.
/// - `new == old` -> Ok(dir/old) without touching the filesystem.
/// - If `dir/new` exists:
///   - and `name::is_case_only_change(old, new)`: list `dir` and look for an entry whose name is
///     byte-for-byte equal to `new`. If one exists, it is a different entry (case-sensitive FS)
///     -> Exists. Otherwise the FS is case-insensitive and `dir/new` is `dir/old` itself: rename
///     through a temporary name (`dir/.<old>.fds-rename.<pid>`) so the case change sticks.
///   - otherwise -> Exists (never overwrite).
/// - Other failures -> Io with the OS error text.
/// - Returns the new path.
pub fn rename(dir: &Path, old: &str, new: &str) -> Result<PathBuf, SimpleError> {
    super::name::validate(new, cfg!(windows)).map_err(SimpleError::InvalidName)?;
    let source = dir.join(old);
    let destination = dir.join(new);
    if old == new {
        return Ok(source);
    }
    if exists(&destination)? {
        if !super::name::is_case_only_change(old, new) {
            return Err(SimpleError::Exists(destination));
        }
        for entry in std::fs::read_dir(dir).map_err(io_error)? {
            if entry.map_err(io_error)?.file_name() == std::ffi::OsStr::new(new) {
                return Err(SimpleError::Exists(destination));
            }
        }
        let temporary = dir.join(format!(".{old}.fds-rename.{}", std::process::id()));
        if exists(&temporary)? {
            return Err(SimpleError::Exists(temporary));
        }
        std::fs::rename(&source, &temporary).map_err(io_error)?;
        if let Err(error) = std::fs::rename(&temporary, &destination) {
            if let Err(rollback) = std::fs::rename(&temporary, &source) {
                return Err(SimpleError::Io(format!(
                    "{error}; restoring {} failed: {rollback}; entry remains at {}",
                    source.display(),
                    temporary.display()
                )));
            }
            return Err(io_error(error));
        }
    } else {
        std::fs::rename(&source, &destination).map_err(io_error)?;
    }
    Ok(destination)
}

/// Creates `dir/name` (P3-12). Validates like `rename`; an existing entry -> Exists.
/// Returns the new path.
pub fn make_dir(dir: &Path, name: &str) -> Result<PathBuf, SimpleError> {
    super::name::validate(name, cfg!(windows)).map_err(SimpleError::InvalidName)?;
    let path = dir.join(name);
    match std::fs::create_dir(&path) {
        Ok(()) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(SimpleError::Exists(path))
        }
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: std::io::Error) -> SimpleError {
    SimpleError::Io(error.to_string())
}

fn exists(path: &Path) -> Result<bool, SimpleError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}
