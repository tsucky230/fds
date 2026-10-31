//! The preview pane's data flow (R-200-9), shaped like the info panel's (`app_info.rs`):
//! `want_preview` says what to load, `on_preview` stores the answer, `stale_preview` says which
//! load to cancel. The event loop runs the loads on threads (`preview::load_preview`).
//!
//! At most one load is in flight, so a slow drive cannot pile up threads; a load that does not
//! come back within `PREVIEW_ABANDON` after its target changed is given up.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::app::{App, Effect, Mode};
use crate::command::Command;
use crate::entry::FileEntry;
use crate::message::Message;
use crate::pane::CursorFocus;
use crate::preview::{PreviewDoc, PreviewError};

/// How long a load whose target is no longer under the cursor may block the next one.
pub const PREVIEW_ABANDON: Duration = Duration::from_secs(2);

/// One request for the loader thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRequest {
    pub id: u64,
    pub path: PathBuf,
    /// Whether a folder preview lists hidden entries (follows the list, `.`).
    pub show_hidden: bool,
    /// Decode images for the pane (setting `[ui] preview_images`, R-200-10).
    pub thumbnails: bool,
}

/// What the pane shows for the item under the cursor. Never another item's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewView<'a> {
    /// The preview is turned off: the list takes the whole width.
    Off,
    /// Nothing to preview (cursor on `[..]`, empty folder): the pane stays, empty.
    Empty,
    /// The cursor item's preview has not arrived yet.
    Loading,
    Ready(&'a PreviewDoc),
    /// Never `PreviewError::Cancelled` (cancelled answers are not stored).
    Failed(&'a PreviewError),
}

/// The read-only folder list in the preview pane (R-200-13): `[..]` and a snapshot of the folder
/// preview, so nothing here is an operation target or a destination (R-001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeekView {
    pub dir_name: String,
    pub entries: Vec<FileEntry>,
    /// The folder has more entries than the preview listed.
    pub more: bool,
    pub cursor: usize,
    pub scroll: usize,
}

impl App {
    /// Tab in the list (R-200-13).
    ///
    /// Behavior:
    /// - The cursor item must be a folder (or a link to one) and no listing may be in flight;
    ///   otherwise nothing happens.
    /// - The pane must be on screen with room to split, else `PeekUnavailable`.
    /// - Its folder preview must have arrived: an empty folder gives `PeekEmpty`, one still loading
    ///   `PeekLoading`, and a failed one leaves its error in the pane.
    /// - Otherwise the listed entries become `peek` and the mode `PreviewBrowse`.
    pub(crate) fn enter_peek(&mut self) {
        let Some(entry) = self
            .pane
            .cursor_entry()
            .filter(|e| !e.is_parent_link() && e.kind.is_dir_like())
        else {
            return;
        };
        if self.is_loading() {
            return;
        }
        if !self.show_preview || !self.preview_shown || !self.peek_fits {
            self.status = Some(Message::PeekUnavailable);
            return;
        }
        let dir_name = entry.display_name.clone();
        match self.preview_view() {
            PreviewView::Ready(PreviewDoc::Folder { entries, more }) if entries.is_empty() => {
                debug_assert!(!more);
                self.status = Some(Message::PeekEmpty);
            }
            PreviewView::Ready(PreviewDoc::Folder { entries, more }) => {
                let mut list = vec![FileEntry::parent_link(self.pane.current_dir.clone())];
                list.extend_from_slice(entries);
                self.peek = Some(PeekView {
                    dir_name,
                    entries: list,
                    more: *more,
                    // The first item, not `[..]`: Enter on `[..]` leaves the list.
                    cursor: 1,
                    scroll: 0,
                });
                self.mode = Mode::PreviewBrowse;
            }
            PreviewView::Loading => self.status = Some(Message::PeekLoading),
            _ => {}
        }
    }

    /// Back to the list on the left, whose cursor has not moved.
    pub fn leave_peek(&mut self) {
        self.peek = None;
        self.mode = Mode::Normal;
    }

    /// Commands in the pane's list. Only moving, opening and leaving: the keys that change files
    /// do nothing here (R-001).
    pub(crate) fn handle_peek(&mut self, cmd: Command) -> Effect {
        self.status = None;
        let Some(peek) = self.peek.as_mut() else {
            self.mode = Mode::Normal;
            return Effect::None;
        };
        let last = peek.entries.len().saturating_sub(1);
        match cmd {
            Command::CursorUp => peek.cursor = peek.cursor.saturating_sub(1),
            Command::CursorDown => peek.cursor = (peek.cursor + 1).min(last),
            Command::PageUp => peek.cursor = peek.cursor.saturating_sub(10),
            Command::PageDown => peek.cursor = (peek.cursor + 10).min(last),
            Command::Home => peek.cursor = 0,
            Command::End => peek.cursor = last,
            Command::Enter | Command::View => {
                let Some(entry) = peek.entries.get(peek.cursor).cloned() else {
                    return Effect::None;
                };
                // `[..]` is the folder of the list on the left.
                if cmd == Command::Enter && entry.is_parent_link() {
                    self.leave_peek();
                    return Effect::None;
                }
                if cmd == Command::Enter
                    && let Some(effect) = self.open_app_effect(&entry)
                {
                    return effect;
                }
                if cmd == Command::Enter && entry.kind.is_dir_like() {
                    self.leave_peek();
                    return self.request_load(entry.path, CursorFocus::First);
                }
                return self.view_entry(&entry);
            }
            Command::OpenExternal => {
                let entry = peek.entries.get(peek.cursor).cloned();
                return self.open_external(entry);
            }
            Command::NextViewer
            | Command::Back
            | Command::ParentDirectory
            | Command::ScrollLeft => self.leave_peek(),
            Command::Help => {
                self.help_return = Mode::PreviewBrowse;
                self.mode = Mode::Help;
            }
            Command::NextImageProtocol => return self.image_protocol_effect(),
            Command::Quit => {
                self.leave_peek();
                self.mode = Mode::ConfirmQuit;
            }
            Command::ForceQuit => return Effect::Quit,
            Command::Suspend => return Effect::Suspend,
            _ => {}
        }
        Effect::None
    }

    /// What to load now, if anything.
    ///
    /// Behavior:
    /// - Only with the preview on, drawn by the last frame (`preview_shown`) and in Normal or
    ///   Input mode (the list is on screen); otherwise `None` and nothing changes, so coming back
    ///   re-checks.
    /// - The target is the cursor item (not `[..]`) with `pane.show_hidden`: in the pane's list
    ///   while `PreviewBrowse`, else in the list.
    /// - A load in flight for the same path: `None`. For another path: `None` until
    ///   `PREVIEW_ABANDON` has passed since that load started (`now - started >= PREVIEW_ABANDON`);
    ///   then it is given up (its answer will be ignored) and the target is checked as below.
    /// - The same target as the last request (since the last listing, toggle or cancelled answer):
    ///   `None`.
    /// - No target (`[..]`, empty folder): the stored preview is dropped, `None`.
    /// - Otherwise a new request with a new id, remembered as in flight since `now`.
    pub fn want_preview(&mut self, now: Instant) -> Option<PreviewRequest> {
        if !self.preview_active() {
            return None;
        }
        let target = self.preview_target().cloned();
        if let Some((_, path, started)) = &self.pending_preview {
            if target.as_ref() == Some(path) || now.duration_since(*started) < PREVIEW_ABANDON {
                return None;
            }
            // Stuck on an item that is no longer wanted: give it up; its answer is ignored.
            self.pending_preview = None;
        }
        let key = (target.clone(), self.pane.show_hidden);
        if self.preview_key.as_ref() == Some(&key) {
            return None;
        }
        self.preview_key = Some(key);
        let Some(path) = target else {
            self.preview = None;
            return None;
        };
        self.next_id += 1;
        let id = self.next_id;
        self.pending_preview = Some((id, path.clone(), now));
        Some(PreviewRequest {
            id,
            path,
            show_hidden: self.pane.show_hidden,
            thumbnails: self.preview_images,
        })
    }

    /// Stores the answer for request `id`.
    ///
    /// Behavior:
    /// - An id that is not the load in flight (older, or given up) is ignored.
    /// - `Err(Cancelled)`: nothing is stored and the target is forgotten, so `want_preview` asks
    ///   again when that item is (still or again) under the cursor.
    /// - Otherwise the answer is stored for the request's path, even when the cursor has moved
    ///   meanwhile (`preview_view` decides whether it is shown).
    pub fn on_preview(&mut self, id: u64, result: Result<PreviewDoc, PreviewError>) {
        let Some((pending, path, started)) = self.pending_preview.take() else {
            return;
        };
        if pending != id {
            self.pending_preview = Some((pending, path, started));
            return;
        }
        if result == Err(PreviewError::Cancelled) {
            self.preview_key = None;
            return;
        }
        self.preview = Some((path, result));
    }

    /// The id of the load in flight when its target is no longer wanted (the cursor moved to
    /// another item, the preview was turned off, or the pane left the screen), so the event loop
    /// can cancel it. `None` when nothing is in flight or the load is still wanted.
    pub fn stale_preview(&self) -> Option<u64> {
        let (id, path, _) = self.pending_preview.as_ref()?;
        let wanted = self.preview_active() && self.preview_target() == Some(path);
        (!wanted).then_some(*id)
    }

    /// What the pane shows now.
    ///
    /// Behavior:
    /// - Preview off -> `Off`.
    /// - No target (`[..]`, empty folder) -> `Empty`.
    /// - A stored answer for exactly the cursor item's path -> `Ready` / `Failed`.
    /// - Otherwise -> `Loading` (an answer for another item is never shown).
    pub fn preview_view(&self) -> PreviewView<'_> {
        if !self.show_preview {
            return PreviewView::Off;
        }
        let Some(target) = self.preview_target() else {
            return PreviewView::Empty;
        };
        match &self.preview {
            Some((path, Ok(doc))) if path == target => PreviewView::Ready(doc),
            Some((path, Err(error))) if path == target => PreviewView::Failed(error),
            _ => PreviewView::Loading,
        }
    }

    /// The item to preview: the cursor item, unless it is `[..]` (or the folder is empty).
    pub(crate) fn preview_entry(&self) -> Option<&FileEntry> {
        if let Some(peek) = self.peek.as_ref() {
            return peek
                .entries
                .get(peek.cursor)
                .filter(|e| !e.is_parent_link());
        }
        self.pane.cursor_entry().filter(|e| !e.is_parent_link())
    }

    pub(crate) fn preview_target(&self) -> Option<&PathBuf> {
        self.preview_entry().map(|e| &e.path)
    }

    /// The pane is on screen: turned on, drawn by the last frame, and the list is showing.
    fn preview_active(&self) -> bool {
        self.show_preview
            && self.preview_shown
            && matches!(
                self.mode,
                Mode::Normal
                    | Mode::Input
                    | Mode::PreviewBrowse
                    | Mode::ConfirmQuit
                    | Mode::ConfirmRun
            )
    }
}
