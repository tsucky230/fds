//! Pre-flight checks before a job starts (P3-5, §45 I4).

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the first source such that `dest` is that source itself or inside it.
///
/// Both sides are resolved with `fs::canonicalize` first, so a destination reached through a
/// symlink or junction is still detected. A source that cannot be resolved (missing) is skipped
/// here; the worker reports it later. If `dest` cannot be resolved, it is compared as given.
pub fn source_containing(sources: &[PathBuf], dest: &Path) -> Option<PathBuf> {
    let dest_real = fs::canonicalize(dest).unwrap_or_else(|_| dest.to_path_buf());
    sources
        .iter()
        .find(|source| {
            fs::canonicalize(source)
                .map(|real| dest_real.starts_with(&real))
                .unwrap_or(false)
        })
        .cloned()
}
