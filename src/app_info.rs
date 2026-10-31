//! The info panel's data flow: `want_info` says what to load (the event loop loads it on a thread),
//! `on_info` stores the answer. Kept out of `App::handle` so no command has to know about it.

use std::path::PathBuf;

use crate::app::{App, Mode};
use crate::info::{Info, MarkSummary};

/// One request for the loader thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoRequest {
    pub id: u64,
    /// The item under the cursor (`None` on `[..]` or in an empty folder).
    pub path: Option<PathBuf>,
    /// The marked items (empty when nothing is marked).
    pub marked: Vec<PathBuf>,
}

impl App {
    /// What to load now, if the target changed since the last request.
    ///
    /// Behavior:
    /// - Only in Normal mode with the panel shown; otherwise `None` and nothing is remembered,
    ///   so coming back re-checks.
    /// - The target is (cursor path, marked paths). The same target is not asked twice.
    /// - Nothing to show (cursor on `[..]`, nothing marked): stale details are cleared and
    ///   `None` is returned.
    /// - A new request gets a new id; answers with an older id are dropped by `on_info`.
    pub fn want_info(&mut self) -> Option<InfoRequest> {
        if self.mode != Mode::Normal || !self.show_info {
            return None;
        }
        let path = self
            .pane
            .cursor_entry()
            .filter(|e| !e.is_parent_link())
            .map(|e| e.path.clone());
        let marked: Vec<PathBuf> = self
            .pane
            .entries
            .iter()
            .filter(|e| e.marked)
            .map(|e| e.path.clone())
            .collect();
        let key = (path.clone(), marked.clone());
        if self.info_key.as_ref() == Some(&key) {
            return None;
        }
        self.info_key = Some(key);
        if path.is_none() && marked.is_empty() {
            self.info = None;
            self.info_marks = None;
            self.pending_info = None;
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.pending_info = Some(id);
        Some(InfoRequest { id, path, marked })
    }

    /// Stores the answer for request `id` (ignored when a newer request was made).
    pub fn on_info(&mut self, id: u64, info: Option<Info>, marks: Option<MarkSummary>) {
        if self.pending_info != Some(id) {
            return;
        }
        self.pending_info = None;
        self.info = info;
        self.info_marks = marks;
    }
}
