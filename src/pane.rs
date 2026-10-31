//! Directory browser state (design prompt §10, §13, §14 / R-101, R-102, R-104, R-107, R-109).
//!
//! Pure logic that never touches the filesystem. The caller reads a directory and passes the
//! result to `set_listing`.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::entry::FileEntry;
use crate::sort::sort_entries;

/// Where to put the cursor when the listing is replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorFocus {
    /// Just entered a directory. First entry after `[..]`; else `[..]`; else 0 (R-102-3).
    First,
    /// Just came back with Parent. The entry with this name; if not found, same as First (R-102-4).
    Name(OsString),
    /// Refresh. Same name as the previous cursor entry; if it is gone, the same index
    /// (clamped to the last entry) (R-109-2).
    Keep,
}

#[derive(Debug, Clone, Default)]
pub struct Pane {
    pub current_dir: PathBuf,
    /// Visible entries (hidden filter applied, sorted, includes `[..]`).
    pub entries: Vec<FileEntry>,
    /// All entries before filtering (sorted, includes `[..]`). Used to rebuild on hidden toggle.
    all_entries: Vec<FileEntry>,
    pub cursor: usize,
    /// Index in `entries` shown at the top of the list area.
    pub scroll: usize,
    pub show_hidden: bool,
    /// Rows in the list area at the last `ensure_visible` call. Used as the page size.
    page_size: usize,
}

impl Pane {
    /// Creates an empty Pane. `show_hidden` defaults to true (R-107-1).
    pub fn new(dir: PathBuf) -> Self {
        Self {
            current_dir: dir,
            show_hidden: true,
            ..Self::default()
        }
    }

    /// Replaces the state with a freshly read listing.
    ///
    /// Behavior:
    /// - `listing` contains real entries only (no `[..]`). If `dir.parent()` is Some, prepend
    ///   `FileEntry::parent_link(parent)` (not at the root, R-101-4).
    /// - Sort with `crate::sort::sort_entries`.
    /// - Marks: only when `focus` is Keep and `dir == self.current_dir`, re-mark entries whose name
    ///   was marked before (R-104-4). Otherwise all marks are cleared (R-104-3).
    /// - Build `entries` from all entries, dropping hidden ones when `self.show_hidden` is false
    ///   (ParentLink is always kept).
    /// - Place the cursor per `focus`. Reset scroll to 0 (keep it for Keep); the caller then
    ///   calls `ensure_visible`.
    /// - If `entries` is empty, cursor = 0 and scroll = 0.
    /// - Set `current_dir` to `dir`.
    pub fn set_listing(&mut self, dir: PathBuf, listing: Vec<FileEntry>, focus: CursorFocus) {
        let keep_marks = matches!(focus, CursorFocus::Keep) && dir == self.current_dir;
        let marked_names: HashSet<OsString> = if keep_marks {
            self.all_entries
                .iter()
                .filter(|e| e.marked)
                .map(|e| e.name.clone())
                .collect()
        } else {
            HashSet::new()
        };

        let old_cursor_entry = if matches!(focus, CursorFocus::Keep) {
            self.cursor_entry().cloned()
        } else {
            None
        };
        let old_cursor = self.cursor;
        let old_scroll = self.scroll;

        let mut all_entries = listing;
        if let Some(parent) = dir.parent() {
            all_entries.insert(0, FileEntry::parent_link(parent.to_path_buf()));
        }
        sort_entries(&mut all_entries);

        if keep_marks {
            for e in &mut all_entries {
                if marked_names.contains(&e.name) {
                    e.marked = true;
                }
            }
        }

        self.all_entries = all_entries;
        let (entries, _) = self.build_visible();
        self.entries = entries;

        if self.entries.is_empty() {
            self.cursor = 0;
            self.scroll = 0;
        } else {
            match focus {
                CursorFocus::First => {
                    self.cursor = first_cursor_position(&self.entries);
                    self.scroll = 0;
                }
                CursorFocus::Name(ref target) => {
                    self.cursor = self
                        .entries
                        .iter()
                        .position(|e| e.name == *target)
                        .unwrap_or_else(|| first_cursor_position(&self.entries));
                    self.scroll = 0;
                }
                CursorFocus::Keep => {
                    if let Some(ref old) = old_cursor_entry {
                        self.cursor = self
                            .entries
                            .iter()
                            .position(|e| e.name == old.name)
                            .unwrap_or_else(|| {
                                let max = self.entries.len() - 1;
                                old_cursor.min(max)
                            });
                    } else {
                        let max = self.entries.len() - 1;
                        self.cursor = old_cursor.min(max);
                    }
                    self.scroll = old_scroll;
                }
            }
        }

        self.current_dir = dir;
    }

    /// Toggles showing hidden files (R-107-2).
    ///
    /// Behavior:
    /// - Flip `show_hidden` and rebuild `entries` from `all_entries`. Marks are kept.
    /// - Cursor goes to the entry with the same name as before. If that entry became hidden, go to
    ///   the nearest visible entry before it (or 0 if none).
    pub fn toggle_hidden(&mut self) {
        let old_entry = self.cursor_entry().cloned();
        let old_all_index = old_entry.as_ref().and_then(|e| {
            self.all_entries
                .iter()
                .position(|a| a.name == e.name && a.path == e.path)
        });

        self.show_hidden = !self.show_hidden;
        let (entries, all_indices) = self.build_visible();
        self.entries = entries;

        if self.entries.is_empty() {
            self.cursor = 0;
            self.scroll = 0;
            return;
        }

        if let Some(ref old) = old_entry {
            if let Some(pos) = self
                .entries
                .iter()
                .position(|e| e.name == old.name && e.path == old.path)
            {
                self.cursor = pos;
            } else if let Some(idx) = old_all_index {
                let mut new_pos = 0;
                for (pos, &all_idx) in all_indices.iter().enumerate() {
                    if all_idx < idx {
                        new_pos = pos;
                    } else {
                        break;
                    }
                }
                self.cursor = new_pos;
            } else {
                let max = self.entries.len() - 1;
                self.cursor = self.cursor.min(max);
            }
        } else {
            let max = self.entries.len() - 1;
            self.cursor = self.cursor.min(max);
        }
    }

    /// Moves the cursor up by one. Does not move at the top (no wrap).
    pub fn cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Moves the cursor down by one. Does not move at the bottom. Never panics on empty entries.
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }

    /// Moves the cursor up by one page (the list height; 1 before the first render). Clamps at 0.
    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(self.page_size.max(1));
    }

    /// Moves the cursor down by one page. Clamps at the last entry.
    pub fn page_down(&mut self) {
        let last = self.entries.len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add(self.page_size.max(1)).min(last);
    }

    /// Moves the cursor to the first entry.
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the last entry (0 when empty).
    pub fn cursor_end(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
    }

    /// The entry under the cursor. None if entries is empty.
    pub fn cursor_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.cursor)
    }

    /// Toggles the mark of the entry under the cursor (R-104-1). The cursor does not move.
    ///
    /// Returns true if toggled; false if entries is empty or the cursor is on `[..]` (R-104-5).
    /// The same entry in `all_entries` is updated too (so marks survive a hidden toggle).
    pub fn toggle_mark(&mut self) -> bool {
        let entry = match self.cursor_entry() {
            Some(e) => e,
            None => return false,
        };
        if entry.is_parent_link() {
            return false;
        }

        let path = entry.path.clone();
        let new_marked = !entry.marked;

        for e in &mut self.entries {
            if e.path == path {
                e.marked = new_marked;
            }
        }
        for e in &mut self.all_entries {
            if e.path == path {
                e.marked = new_marked;
            }
        }

        true
    }

    /// Marked entries (visible ones).
    pub fn marked_entries(&self) -> Vec<&FileEntry> {
        self.entries.iter().filter(|e| e.marked).collect()
    }

    /// Adjusts scroll so the cursor is visible within `height` rows.
    ///
    /// Behavior:
    /// - If `height == 0`, do nothing.
    /// - If cursor < scroll, scroll = cursor.
    /// - If cursor >= scroll + height, scroll = cursor + 1 - height.
    /// - Then clamp scroll to at most `entries.len().saturating_sub(height)`
    ///   (no blank rows at the bottom after the terminal grows).
    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        self.page_size = height;
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        if self.cursor >= self.scroll.saturating_add(height) {
            self.scroll = self.cursor + 1 - height;
        }
        let max_scroll = self.entries.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Name of the current directory itself (for CursorFocus::Name on Parent). None at the root.
    pub fn current_dir_name(&self) -> Option<OsString> {
        self.current_dir.file_name().map(|n| n.to_os_string())
    }

    /// Build the visible entry list and the corresponding indices in `all_entries`.
    fn build_visible(&self) -> (Vec<FileEntry>, Vec<usize>) {
        let mut entries = Vec::new();
        let mut indices = Vec::new();
        for (i, e) in self.all_entries.iter().enumerate() {
            if e.is_parent_link() || self.show_hidden || !e.hidden {
                entries.push(e.clone());
                indices.push(i);
            }
        }
        (entries, indices)
    }
}

/// Cursor position for `CursorFocus::First`.
fn first_cursor_position(entries: &[FileEntry]) -> usize {
    if entries.is_empty() {
        0
    } else if entries[0].is_parent_link() && entries.len() > 1 {
        1
    } else {
        0
    }
}
