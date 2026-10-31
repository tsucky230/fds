//! Pure App state for the standard archive browser and jobs (R-407).

use std::io;
use std::path::PathBuf;

use crate::app::{App, Effect, Mode};
use crate::app_ops::{InputPurpose, InputState};
use crate::archive::{Action, Member};
use crate::command::Command;
use crate::entry::{EntryKind, LinkTarget};
use crate::line_editor::LineEditor;
use crate::message::Message;
use crate::ops::target;
use crate::pane::CursorFocus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveView {
    pub path: PathBuf,
    pub members: Vec<Member>,
    pub prefix: PathBuf,
    pub cursor: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveJob {
    pub id: u64,
    pub action: Action,
    pub cancelling: bool,
}

impl ArchiveView {
    /// Immediate children of `prefix`, in the stable order returned by `archive::list`.
    pub fn children(&self) -> Vec<&Member> {
        self.members
            .iter()
            .filter(|m| m.path.parent() == Some(self.prefix.as_path()))
            .collect()
    }

    pub fn selected(&self) -> Option<&Member> {
        self.children().get(self.cursor).copied()
    }
}

impl App {
    /// Return the cursor's archive path only for a supported regular file or file symlink.
    pub(crate) fn archive_cursor_path(&self) -> Option<PathBuf> {
        let entry = self.pane.cursor_entry()?;
        if !matches!(
            entry.kind,
            EntryKind::File | EntryKind::Symlink(LinkTarget::File)
        ) {
            return None;
        }
        crate::archive::format(&entry.path).map(|_| entry.path.clone())
    }

    /// Request asynchronous archive listing. Late results are ignored by `on_archive_loaded`.
    pub(crate) fn open_archive(&mut self, path: PathBuf) -> Effect {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.pending_archive = Some((id, path.clone()));
        self.archive = None;
        self.mode = Mode::ArchiveLoading;
        Effect::OpenArchive { id, path }
    }

    /// Prompt for the output archive name; selected entries are snapshotted now.
    pub(crate) fn begin_archive_create(&mut self) {
        let sources = target::targets(&self.pane);
        if sources.is_empty() {
            self.status = Some(Message::NothingSelected);
            return;
        }
        self.input = Some(InputState {
            purpose: InputPurpose::ArchiveCreate {
                sources,
                dir: self.pane.current_dir.clone(),
            },
            editor: LineEditor::new("archive.zip"),
        });
        self.mode = Mode::Input;
    }

    /// Prompt for a new extraction-directory name for the cursor archive.
    pub(crate) fn begin_archive_extract(&mut self) {
        let source = if self.mode == Mode::ArchiveBrowse {
            self.archive.as_ref().map(|view| view.path.clone())
        } else {
            self.archive_cursor_path()
        };
        let Some(source) = source else {
            self.status = Some(Message::NothingSelected);
            return;
        };
        let Some(name) = crate::archive::extract_destination(&source)
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            self.status = Some(Message::OperationFailed {
                name: source.display().to_string(),
                reason: "invalid_name".into(),
            });
            return;
        };
        self.input = Some(InputState {
            purpose: InputPurpose::ArchiveExtract {
                source,
                dir: self.pane.current_dir.clone(),
                return_mode: self.mode,
            },
            editor: LineEditor::new(&name),
        });
        self.mode = Mode::Input;
    }

    /// Handle input while an archive is opening; Esc returns to the filesystem list.
    pub(crate) fn handle_archive_loading(&mut self, cmd: Command) -> Effect {
        if matches!(
            cmd,
            Command::Back | Command::Quit | Command::ParentDirectory
        ) {
            self.pending_archive = None;
            self.mode = Mode::Normal;
        }
        Effect::None
    }

    /// Navigate the read-only archive tree or close it.
    pub(crate) fn handle_archive_browse(&mut self, cmd: Command) -> Effect {
        if cmd == Command::ArchiveExtract {
            self.begin_archive_extract();
            return Effect::None;
        }
        let Some(view) = self.archive.as_mut() else {
            return Effect::None;
        };
        let len = view.children().len();
        match cmd {
            Command::CursorUp => view.cursor = view.cursor.saturating_sub(1),
            Command::CursorDown => view.cursor = (view.cursor + 1).min(len.saturating_sub(1)),
            Command::PageUp => view.cursor = view.cursor.saturating_sub(10),
            Command::PageDown => view.cursor = (view.cursor + 10).min(len.saturating_sub(1)),
            Command::Home => view.cursor = 0,
            Command::End => view.cursor = len.saturating_sub(1),
            Command::Enter => {
                if let Some(path) = view.selected().filter(|m| m.is_dir).map(|m| m.path.clone()) {
                    view.prefix = path;
                    view.cursor = 0;
                    view.scroll = 0;
                }
            }
            Command::ParentDirectory | Command::ScrollLeft => {
                if !view.prefix.pop() {
                    self.archive = None;
                    self.mode = Mode::Normal;
                } else {
                    view.cursor = 0;
                    view.scroll = 0;
                }
            }
            Command::Back | Command::Quit | Command::View => {
                self.archive = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Effect::None
    }

    /// Esc requests cancellation of an active archive job.
    pub(crate) fn handle_archive_running(&mut self, cmd: Command) -> Effect {
        if matches!(cmd, Command::Back | Command::Quit)
            && let Some(job) = self.archive_job.as_mut().filter(|job| !job.cancelling)
        {
            job.cancelling = true;
            return Effect::CancelArchive { id: job.id };
        }
        Effect::None
    }

    /// Accept one completed archive listing if its request id is still current.
    pub fn on_archive_loaded(&mut self, id: u64, result: io::Result<Vec<Member>>) {
        let Some((current_id, path)) = self.pending_archive.take() else {
            return;
        };
        if current_id != id {
            self.pending_archive = Some((current_id, path));
            return;
        }
        if self.mode != Mode::ArchiveLoading {
            return;
        }
        match result {
            Ok(members) => {
                self.archive = Some(ArchiveView {
                    path,
                    members,
                    prefix: PathBuf::new(),
                    cursor: 0,
                    scroll: 0,
                });
                self.mode = Mode::ArchiveBrowse;
            }
            Err(e) => {
                self.status = Some(Message::OperationFailed {
                    name: path.display().to_string(),
                    reason: format!("!{e}"),
                });
                self.mode = Mode::Normal;
            }
        }
    }

    /// Accept one completed archive job, report its result and refresh the filesystem listing.
    pub fn on_archive_finished(&mut self, id: u64, result: io::Result<()>) -> Effect {
        let Some(job) = self.archive_job.take() else {
            return Effect::None;
        };
        if job.id != id {
            self.archive_job = Some(job);
            return Effect::None;
        }
        let dest = job.action.destination().to_path_buf();
        self.archive = None;
        self.mode = Mode::Normal;
        match result {
            Ok(()) => {
                self.status = Some(Message::ArchiveDone {
                    name: dest
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                })
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                self.status = Some(Message::Cancelled)
            }
            Err(e) => {
                self.status = Some(Message::OperationFailed {
                    name: dest.display().to_string(),
                    reason: format!("!{e}"),
                })
            }
        }
        self.request_load(
            self.pane.current_dir.clone(),
            CursorFocus::Name(dest.file_name().unwrap_or_default().to_os_string()),
        )
    }

    /// Start a validated archive operation after its name prompt is submitted.
    pub(crate) fn start_archive_action(&mut self, action: Action) -> Effect {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.archive_job = Some(ArchiveJob {
            id,
            action: action.clone(),
            cancelling: false,
        });
        self.mode = Mode::ArchiveRunning;
        Effect::StartArchive { id, action }
    }
}
