//! Directory reading (design prompt §11, §18 / R-101, R-301). OS-dependent code stays here.

use std::ffi::OsStr;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use crate::app::ListingResult;
use crate::entry::{EntryKind, FileEntry, LinkTarget};

/// Reads the real entries of `dir` (no `[..]`; Pane adds it).
///
/// - Err if `dir` itself cannot be opened (permission denied, missing, ...).
/// - An entry that fails to read is skipped; the listing as a whole does not fail.
/// - Symlink targets are resolved with an extra stat. This runs on the loader thread, so a slow
///   filesystem does not block the UI or key input (R-101-6, R-1400-2).
pub fn read_listing(dir: &Path) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        entries.push(to_entry(&item));
    }
    Ok(entries)
}

/// Builds the entry for one directory item (also used by the preview pane).
pub(crate) fn to_entry(item: &DirEntry) -> FileEntry {
    let name = item.file_name();
    let path = item.path();
    let kind = match item.file_type() {
        Ok(t) if t.is_symlink() => EntryKind::Symlink(link_target(&path)),
        Ok(t) if t.is_dir() => EntryKind::Directory,
        Ok(t) if t.is_file() => EntryKind::File,
        _ => EntryKind::Other,
    };
    // Attributes of the symlink itself, not its target.
    let hidden = is_hidden(&name, fs::symlink_metadata(&path).ok().as_ref());
    FileEntry::new(path, name, kind, hidden)
}

fn link_target(path: &Path) -> LinkTarget {
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => LinkTarget::Directory,
        Ok(_) => LinkTarget::File,
        Err(_) => LinkTarget::Broken,
    }
}

/// Dotfiles are hidden on every OS; on Windows the Hidden attribute also counts (R-107-1).
fn is_hidden(name: &OsStr, metadata: Option<&Metadata>) -> bool {
    let dotfile = name.as_encoded_bytes().first() == Some(&b'.');
    dotfile || has_hidden_attribute(metadata)
}

#[cfg(windows)]
fn has_hidden_attribute(metadata: Option<&Metadata>) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    metadata.is_some_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn has_hidden_attribute(_metadata: Option<&Metadata>) -> bool {
    false
}

/// Reads `dir` on a separate thread and sends `(id, result)`.
///
/// If the receiver is gone (shutting down), the result is dropped.
pub fn spawn_listing(id: u64, dir: std::path::PathBuf, tx: Sender<(u64, ListingResult)>) {
    thread::spawn(move || {
        let result = read_listing(&dir).map_err(|e| e.to_string());
        let _ = tx.send((id, result));
    });
}
