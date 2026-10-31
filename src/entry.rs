//! One row of the listing: FileEntry (design prompt §11).

use std::ffi::OsString;
use std::path::PathBuf;

/// Kind of a symlink's target (R-101-3). `Unknown` before/without resolution (R-101-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTarget {
    Directory,
    File,
    Broken,
    Unknown,
}

/// Entry kind. `[..]` is represented as `ParentLink`, distinct from real entries (R-101-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    ParentLink,
    Directory,
    File,
    Symlink(LinkTarget),
    Other,
}

impl EntryKind {
    /// Whether Enter can go into it and it sorts into the directory group (R-302-1).
    pub fn is_dir_like(self) -> bool {
        matches!(
            self,
            EntryKind::ParentLink
                | EntryKind::Directory
                | EntryKind::Symlink(LinkTarget::Directory)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Original path used for filesystem operations. Never rebuilt from display text (R-303-3).
    pub path: PathBuf,
    /// Original file name.
    pub name: OsString,
    /// Display-only text (lossy conversion). Also used for sorting.
    pub display_name: String,
    pub kind: EntryKind,
    /// Dotfile, or has the Windows Hidden attribute (R-107-1).
    pub hidden: bool,
    pub marked: bool,
}

impl FileEntry {
    /// Creates a real entry. `display_name` is a lossy conversion of `name`.
    pub fn new(path: PathBuf, name: OsString, kind: EntryKind, hidden: bool) -> Self {
        let display_name = name.to_string_lossy().into_owned();
        Self {
            path,
            name,
            display_name,
            kind,
            hidden,
            marked: false,
        }
    }

    /// Creates the `[..]` entry. `parent` is the parent directory. Never hidden, never marked.
    pub fn parent_link(parent: PathBuf) -> Self {
        Self {
            path: parent,
            name: OsString::from(".."),
            display_name: "..".to_string(),
            kind: EntryKind::ParentLink,
            hidden: false,
            marked: false,
        }
    }

    pub fn is_parent_link(&self) -> bool {
        self.kind == EntryKind::ParentLink
    }
}
