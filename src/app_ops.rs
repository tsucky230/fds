//! File-operation part of `App` (Phase 3: P3-1 to P3-16, §45 I1-I8).
//!
//! Pure state machine like the rest of `App`: every filesystem action is returned as an `Effect`
//! and its result comes back through `on_job_event` / `on_simple_result` / `on_editor_finished`.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::app::{App, Effect, Mode};
use crate::command::{Command, TextKey};
use crate::entry::{EntryKind, LinkTarget};
use crate::line_editor::LineEditor;
use crate::message::Message;
use crate::ops::simple::SimpleError;
use crate::ops::{Answer, DeleteMode, Job, JobEvent, Op, OpRecord, Resolution};
use crate::pane::CursorFocus;
use crate::search::Direction;
use crate::shell_cmd::{self, CommandError};

/// Lines kept in `App::command_history`.
const COMMAND_HISTORY_LIMIT: usize = 100;

/// Copy / Move waiting for a destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOp {
    pub op: Op,
    pub sources: Vec<PathBuf>,
    /// Directory the sources were chosen in.
    pub origin: PathBuf,
}

/// Progress of the running job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobState {
    pub id: u64,
    pub op: Op,
    pub sources: Vec<PathBuf>,
    pub dest: Option<PathBuf>,
    pub total_files: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub done_bytes: u64,
    pub current: Option<PathBuf>,
    /// A cancel was requested; waiting for `Finished`.
    pub cancelling: bool,
}

/// The conflict question shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictInfo {
    pub job_id: u64,
    pub source: PathBuf,
    pub dest: PathBuf,
    pub dest_is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputPurpose {
    /// Rename `dir/old`.
    Rename {
        dir: PathBuf,
        old: String,
    },
    Mkdir,
    ArchiveCreate {
        sources: Vec<PathBuf>,
        dir: PathBuf,
    },
    ArchiveExtract {
        source: PathBuf,
        dir: PathBuf,
        return_mode: Mode,
    },
    /// Incremental search. `origin_cursor` is restored on Esc; `return_mode` is the mode to go
    /// back to (Normal or SelectDestination).
    Search {
        origin_cursor: usize,
        return_mode: Mode,
    },
    /// A command to run in the listed folder (R-412). `history_pos` is the line of
    /// `App::command_history` on show; `draft` is what was typed before Up was pressed.
    Command {
        history_pos: Option<usize>,
        draft: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputState {
    pub purpose: InputPurpose,
    pub editor: LineEditor,
}

/// What a Rename / mkdir result belongs to (for the status message and cursor focus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleKind {
    Rename { old: String },
    Mkdir,
}

impl App {
    /// True when keys must be delivered as `TextKey` through `handle_text`
    /// (Input, ConfirmDelete, ConfirmQuit, Conflict).
    pub fn wants_text(&self) -> bool {
        matches!(
            self.mode,
            Mode::Input
                | Mode::ConfirmDelete
                | Mode::ConfirmQuit
                | Mode::ConfirmRun
                | Mode::Conflict
        )
    }

    /// F5/c, F6/m in Normal (P3-2). Targets from `ops::target::targets`; none ->
    /// status `NothingSelected`. Otherwise store `PendingOp` (origin = current dir) and enter
    /// `Mode::SelectDestination`.
    pub(crate) fn begin_select(&mut self, op: Op) {
        let sources = crate::ops::target::targets(&self.pane);
        if sources.is_empty() {
            self.status = Some(Message::NothingSelected);
            return;
        }
        self.pending_op = Some(PendingOp {
            op,
            sources,
            origin: self.pane.current_dir.clone(),
        });
        self.mode = Mode::SelectDestination;
        self.dest_on_here = true;
    }

    /// L / F9 (Normal or SelectDestination): ask for the places list. The list is built on a
    /// loader thread (a disconnected network drive can be slow); the answer arrives in
    /// `on_places`, which opens the popup.
    pub(crate) fn request_places(&mut self) -> Effect {
        self.next_id += 1;
        let id = self.next_id;
        self.pending_places = Some(id);
        self.places_return = self.mode;
        Effect::ListPlaces { id }
    }

    /// The places list arrived. Ignored if it is not the request in flight. While choosing a
    /// destination, the origin folder is offered first ("go back where I came from").
    pub fn on_places(&mut self, id: u64, mut places: Vec<crate::ops::place::Place>) {
        if self.pending_places != Some(id) {
            return;
        }
        self.pending_places = None;
        if self.places_return == Mode::SelectDestination
            && let Some(pending) = &self.pending_op
        {
            places.insert(
                0,
                crate::ops::place::Place {
                    kind: crate::ops::place::PlaceKind::Origin,
                    label: pending.origin.to_string_lossy().into_owned(),
                    path: pending.origin.clone(),
                },
            );
        }
        self.places = places;
        self.places_cursor = 0;
        self.mode = Mode::Places;
    }

    /// Commands in Mode::Places: move the cursor, Enter goes there (back to the mode the popup was
    /// opened from), Esc / q / L close it.
    pub(crate) fn handle_places(&mut self, cmd: Command) -> Effect {
        let last = self.places.len().saturating_sub(1);
        match cmd {
            Command::CursorUp => self.places_cursor = self.places_cursor.saturating_sub(1),
            Command::CursorDown => self.places_cursor = (self.places_cursor + 1).min(last),
            Command::Home => self.places_cursor = 0,
            Command::End => self.places_cursor = last,
            Command::Enter => {
                let target = self.places.get(self.places_cursor).map(|p| p.path.clone());
                self.mode = self.places_return;
                self.places.clear();
                if let Some(path) = target {
                    return self.request_load(path, CursorFocus::First);
                }
            }
            Command::Back | Command::Quit | Command::Places => {
                self.mode = self.places_return;
                self.places.clear();
            }
            Command::Suspend => return Effect::Suspend,
            _ => {}
        }
        Effect::None
    }

    /// F8/d in Normal (P3-9). Targets as above; none -> `NothingSelected`. Otherwise
    /// `delete_targets` = targets and `Mode::ConfirmDelete`. Nothing is deleted yet (§45 I1).
    pub(crate) fn begin_delete(&mut self) {
        self.delete_targets = crate::ops::target::targets(&self.pane);
        if self.delete_targets.is_empty() {
            self.status = Some(Message::NothingSelected);
        } else {
            self.mode = Mode::ConfirmDelete;
        }
    }

    /// F2/r in Normal (P3-12). The cursor entry (not `[..]`) -> Input(Rename) prefilled with its
    /// name, cursor before the extension. No entry -> `NothingSelected`.
    pub(crate) fn begin_rename(&mut self) {
        let Some(entry) = self.pane.cursor_entry().filter(|e| !e.is_parent_link()) else {
            self.status = Some(Message::NothingSelected);
            return;
        };
        // The Rename effect carries the old name as text, so a name that is not valid UTF-8
        // could not be found again on disk. Refuse instead of failing with NotFound.
        let Some(old) = entry.name.to_str().map(str::to_string) else {
            self.status = Some(Message::OperationFailed {
                name: entry.display_name.clone(),
                reason: "non_utf8".to_string(),
            });
            return;
        };
        let mut editor = LineEditor::new(&old);
        editor.cursor_before_extension();
        self.input = Some(InputState {
            purpose: InputPurpose::Rename {
                dir: self.pane.current_dir.clone(),
                old,
            },
            editor,
        });
        self.mode = Mode::Input;
    }

    /// F7/n in Normal: Input with an empty editor for `purpose`.
    pub(crate) fn begin_input(&mut self, purpose: InputPurpose) {
        self.input = Some(InputState {
            purpose,
            editor: LineEditor::default(),
        });
        self.mode = Mode::Input;
    }

    /// / in Normal or SelectDestination (P3-14): Input(Search) remembering the cursor and the
    /// mode to return to.
    pub(crate) fn begin_search(&mut self, return_mode: Mode) {
        self.begin_input(InputPurpose::Search {
            origin_cursor: self.pane.cursor,
            return_mode,
        });
    }

    /// F4/e in Normal (P3-15). A regular file or a link to a file -> `Effect::OpenEditor`;
    /// a broken link -> `BrokenSymlink`; anything else -> `NotAFile`.
    pub(crate) fn edit_cursor_entry(&mut self) -> Effect {
        let Some(entry) = self.pane.cursor_entry() else {
            self.status = Some(Message::NotAFile {
                name: String::new(),
            });
            return Effect::None;
        };
        let name = entry.display_name.clone();
        match entry.kind {
            EntryKind::File | EntryKind::Symlink(LinkTarget::File) => Effect::OpenEditor {
                path: entry.path.clone(),
            },
            EntryKind::Symlink(LinkTarget::Broken) => {
                self.status = Some(Message::BrokenSymlink { name });
                Effect::None
            }
            _ => {
                self.status = Some(Message::NotAFile { name });
                Effect::None
            }
        }
    }

    /// Commands in Mode::SelectDestination (P3-2, §45 I3, I8).
    ///
    /// - Navigation (cursor keys, page, home/end, Enter into directories, Parent), ToggleHidden,
    ///   Refresh, ToggleMark are handled like Normal. Entering a file does not open the viewer.
    /// - Help: opens Help and returns here.
    /// - Search: `begin_search(Mode::SelectDestination)`.
    /// - The confirming key: Copy for a pending Copy, Move for a pending Move. The destination is
    ///   the current directory. If the destination is one of the sources or lexically inside one
    ///   (`dest.starts_with(source)`) -> status `InsideSource`, stay here. Otherwise start the
    ///   job (`Effect::StartJob`, new id, `JobState`, Mode::Running) and clear `pending_op`.
    /// - Back (Esc) or Quit (q): cancel the selection -> Normal, status `Cancelled`.
    /// - Delete, Rename, MakeDir, Edit, View, the other of Copy/Move: ignored with status
    ///   `FinishSelectionFirst` (I8).
    /// - Suspend -> Effect::Suspend.
    pub(crate) fn handle_select_destination(&mut self, cmd: Command) -> Effect {
        self.status = None;
        // The first row of the list is "copy/move here" (dest_on_here). Enter on it runs the
        // operation; moving down leaves it, moving up from the top of the list returns to it.
        let cmd = match cmd {
            Command::Enter if self.dest_on_here => match self.pending_op.as_ref().map(|p| p.op) {
                Some(Op::Move) => Command::Move,
                _ => Command::Copy,
            },
            Command::CursorDown | Command::PageDown | Command::End if self.dest_on_here => {
                self.dest_on_here = false;
                return Effect::None;
            }
            Command::CursorUp | Command::PageUp | Command::Home
                if !self.dest_on_here && self.pane.cursor == 0 =>
            {
                self.dest_on_here = true;
                return Effect::None;
            }
            Command::Home => {
                self.dest_on_here = true;
                return Effect::None;
            }
            // Left is Parent in the list, as the guide says ("Left/h: up").
            Command::ScrollLeft => Command::ParentDirectory,
            other => other,
        };
        match cmd {
            Command::Places => return self.request_places(),
            Command::Copy | Command::Move => {
                let op = if cmd == Command::Copy {
                    Op::Copy
                } else {
                    Op::Move
                };
                let Some(pending) = self.pending_op.as_ref() else {
                    return Effect::None;
                };
                if pending.op != op {
                    self.status = Some(Message::FinishSelectionFirst);
                    return Effect::None;
                }
                let dest = self.pane.current_dir.clone();
                if op == Op::Move && dest == pending.origin {
                    // Moving into the folder the entries are already in changes nothing.
                    self.status = Some(Message::SameFolder);
                    return Effect::None;
                }
                if let Some(source) = pending.sources.iter().find(|s| dest.starts_with(s)) {
                    self.status = Some(Message::InsideSource {
                        name: source
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    });
                    return Effect::None;
                }
                if let Some(pending) = self.pending_op.take() {
                    return self.start_operation(op, pending.sources, Some(dest));
                }
            }
            Command::Back | Command::Quit => {
                self.pending_op = None;
                self.mode = Mode::Normal;
                self.status = Some(Message::Cancelled);
            }
            Command::Delete
            | Command::Rename
            | Command::MakeDir
            | Command::Edit
            | Command::View => {
                self.status = Some(Message::FinishSelectionFirst);
            }
            Command::Help => {
                self.help_return = Mode::SelectDestination;
                self.mode = Mode::Help;
            }
            Command::Search => self.begin_search(Mode::SelectDestination),
            Command::Enter
                if !self
                    .pane
                    .cursor_entry()
                    .is_some_and(|e| e.kind.is_dir_like()) => {}
            Command::CursorUp
            | Command::CursorDown
            | Command::PageUp
            | Command::PageDown
            | Command::Home
            | Command::End
            | Command::Enter
            | Command::ParentDirectory
            | Command::ToggleHidden
            | Command::Refresh
            | Command::ToggleMark => {
                self.mode = Mode::Normal;
                let effect = self.handle(cmd);
                self.mode = Mode::SelectDestination;
                return effect;
            }
            Command::Suspend => return Effect::Suspend,
            Command::ForceQuit => return Effect::Quit,
            _ => {}
        }
        Effect::None
    }

    /// Commands in Mode::Running (P3-11): Back or Quit -> `Effect::CancelJob` once and
    /// `cancelling = true` (a second press does nothing). Suspend -> Suspend. Others ignored.
    pub(crate) fn handle_running(&mut self, cmd: Command) -> Effect {
        match cmd {
            Command::Back | Command::Quit => {
                if let Some(job) = self.job.as_mut().filter(|job| !job.cancelling) {
                    job.cancelling = true;
                    return Effect::CancelJob { job_id: job.id };
                }
            }
            Command::Suspend => return Effect::Suspend,
            _ => {}
        }
        Effect::None
    }

    /// Keys for Input / ConfirmDelete / Conflict.
    ///
    /// - ForceQuit in any of them -> Effect::Quit.
    /// - ConfirmDelete (P3-9): 'y' / 'Y' -> StartJob(Delete or Trash, by `delete_mode`, of
    ///   `delete_targets`), Mode::Running.
    ///   'n', 'N', 'q', Esc -> Normal, status `Cancelled`, targets cleared. Anything else ignored.
    /// - Conflict (P3-6): 'o' Overwrite, 's' Skip, 'r' Rename, 'c' or Esc Cancel; uppercase
    ///   'O' / 'S' / 'R' set `for_all`. -> Effect::AnswerConflict for `conflict.job_id`,
    ///   back to Mode::Running, `conflict` cleared. Other keys ignored.
    /// - Input:
    ///   - Char / Backspace / Delete / Left / Right / Home / End edit the line.
    ///   - Search: after every edit, move the cursor to `search::find` from the current cursor
    ///     (Forward). Down -> find from cursor + 1 Forward; Up -> from cursor - 1 Backward
    ///     (wrapping). Enter -> keep the cursor, return to `return_mode`. Esc -> restore
    ///     `origin_cursor`, return to `return_mode`.
    ///   - Rename: Enter -> Effect::Rename { dir, old, new: text } and back to Normal
    ///     (an unchanged name just closes). Esc -> Normal, `Cancelled`.
    ///   - Mkdir: Enter -> Effect::MakeDir { dir: current dir, name: text }, Normal.
    ///     An empty text just closes. Esc -> Normal, `Cancelled`.
    pub fn handle_text(&mut self, key: TextKey) -> Effect {
        if key == TextKey::ForceQuit {
            return Effect::Quit;
        }
        match self.mode {
            Mode::ConfirmDelete => match key {
                TextKey::Char('y' | 'Y') => {
                    let sources = std::mem::take(&mut self.delete_targets);
                    let op = if self.delete_mode == DeleteMode::Trash {
                        Op::Trash
                    } else {
                        Op::Delete
                    };
                    self.start_operation(op, sources, None)
                }
                TextKey::Char('n' | 'N' | 'q') | TextKey::Esc => {
                    self.delete_targets.clear();
                    self.mode = Mode::Normal;
                    self.status = Some(Message::Cancelled);
                    Effect::None
                }
                _ => Effect::None,
            },
            Mode::ConfirmQuit => match key {
                TextKey::Char('y' | 'Y') | TextKey::Enter => Effect::Quit,
                TextKey::Char('n' | 'N') | TextKey::Esc => {
                    self.mode = Mode::Normal;
                    Effect::None
                }
                _ => Effect::None,
            },
            // Only y runs it: Enter and every other key answer no (R-110-3).
            Mode::ConfirmRun => {
                let target = self.run_target.take();
                self.mode = self.list_mode();
                match (key, target) {
                    (TextKey::Char('y' | 'Y'), Some(path)) => Effect::RunExecutable { path },
                    _ => {
                        self.status = Some(Message::Cancelled);
                        Effect::None
                    }
                }
            }
            Mode::Conflict => {
                let resolution = match key {
                    TextKey::Char('o' | 'O') => Resolution::Overwrite,
                    TextKey::Char('s' | 'S') => Resolution::Skip,
                    TextKey::Char('r' | 'R') => Resolution::Rename,
                    TextKey::Char('c') | TextKey::Esc => Resolution::Cancel,
                    _ => return Effect::None,
                };
                let Some(conflict) = self.conflict.take() else {
                    return Effect::None;
                };
                self.mode = Mode::Running;
                Effect::AnswerConflict {
                    job_id: conflict.job_id,
                    answer: Answer {
                        resolution,
                        for_all: matches!(key, TextKey::Char('O' | 'S' | 'R')),
                    },
                }
            }
            Mode::Input => self.edit_input(key),
            _ => Effect::None,
        }
    }

    /// Worker events (P3-6, P3-11, §45 I6, I7). Events whose `job_id` is not the running job are
    /// ignored and return Effect::None.
    ///
    /// - Scanned / Progress: update `JobState`.
    /// - Conflict: store `ConflictInfo`, Mode::Conflict.
    /// - EntryFailed: append to `failures`.
    /// - Finished: record an `OpRecord` in `history` (drop the oldest beyond
    ///   `ops::HISTORY_LIMIT`), clear `job` / `conflict`, status `JobDone`, mode ErrorList when
    ///   `failures` is non-empty else Normal, and return a refresh `Effect::Load` of the current
    ///   directory (CursorFocus::Keep).
    pub fn on_job_event(&mut self, job_id: u64, event: JobEvent) -> Effect {
        let Some(job) = self.job.as_mut().filter(|job| job.id == job_id) else {
            return Effect::None;
        };
        match event {
            JobEvent::Scanned { files, bytes } => {
                job.total_files = files;
                job.total_bytes = bytes;
            }
            JobEvent::Progress {
                done_files,
                done_bytes,
                current,
            } => {
                job.done_files = done_files;
                job.done_bytes = done_bytes;
                job.current = Some(current);
            }
            JobEvent::Conflict {
                source,
                dest,
                dest_is_dir,
            } => {
                self.conflict = Some(ConflictInfo {
                    job_id,
                    source,
                    dest,
                    dest_is_dir,
                });
                self.mode = Mode::Conflict;
            }
            JobEvent::EntryFailed { path, reason } => self.failures.push((path, reason)),
            JobEvent::Finished(summary) => {
                self.status = Some(Message::JobDone {
                    op: match job.op {
                        Op::Copy => "copy",
                        Op::Move => "move",
                        Op::Delete => "delete",
                        Op::Trash => "trash",
                    },
                    done: summary.done,
                    skipped: summary.skipped,
                    failed: summary.failed.len() as u64,
                    cancelled: summary.cancelled,
                });
                self.history.push(OpRecord {
                    op: job.op,
                    sources: job.sources.clone(),
                    dest: job.dest.clone(),
                    summary,
                    finished_at: std::time::SystemTime::now(),
                });
                let excess = self.history.len().saturating_sub(crate::ops::HISTORY_LIMIT);
                self.history.drain(..excess);
                self.job = None;
                self.conflict = None;
                self.mode = if self.failures.is_empty() {
                    Mode::Normal
                } else {
                    Mode::ErrorList
                };
                return self.request_load(self.pane.current_dir.clone(), CursorFocus::Keep);
            }
        }
        Effect::None
    }

    /// Result of Effect::Rename / MakeDir (P3-12, P3-13). Ok(path) -> reload the current
    /// directory with the cursor on the new name. Err -> status `OperationFailed` with reason
    /// "invalid_name" (InvalidName), "exists" (Exists) or "!<os text>" (Io); no reload.
    pub fn on_simple_result(
        &mut self,
        kind: SimpleKind,
        result: Result<PathBuf, SimpleError>,
    ) -> Effect {
        match result {
            Ok(path) => {
                let focus = path
                    .file_name()
                    .map(|name| CursorFocus::Name(name.to_os_string()))
                    .unwrap_or(CursorFocus::Keep);
                self.request_load(self.pane.current_dir.clone(), focus)
            }
            Err(error) => {
                let name = match kind {
                    SimpleKind::Rename { old } => old,
                    SimpleKind::Mkdir => match &error {
                        SimpleError::Exists(path) => path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        _ => String::new(),
                    },
                };
                let reason = match error {
                    SimpleError::InvalidName(_) => "invalid_name".into(),
                    SimpleError::Exists(_) => "exists".into(),
                    SimpleError::Io(reason) => format!("!{reason}"),
                };
                self.status = Some(Message::OperationFailed { name, reason });
                Effect::None
            }
        }
    }

    /// `x` on a file: `is_program` says whether to offer it as a program with arguments to fill in
    /// (cursor at the end), or to pass it to a command to type (` {file}`, cursor at the start).
    /// A name `shell_cmd::program_prefill` refuses goes to status and no box opens (R-412).
    pub fn on_command_classified(&mut self, path: PathBuf, is_program: bool) {
        if !is_program {
            self.open_command_box(" {file}", true);
            return;
        }
        let name = path.file_name().unwrap_or(path.as_os_str());
        match shell_cmd::program_prefill(name, self.shell_kind) {
            Ok(text) => self.open_command_box(&text, false),
            Err(reason) => self.status = Some(Message::CommandRejected(reason)),
        }
    }

    /// The shell ended, or could not start (Err). A failing command is not an error here: its
    /// status was already shown in the terminal. Always reloads (the command may have changed the
    /// folder), CursorFocus::Keep, so marks on files that still exist stay (R-412).
    pub fn on_command_finished(&mut self, result: Result<(), String>) -> Effect {
        if let Err(reason) = result {
            self.status = Some(Message::CommandFailed { reason });
        }
        self.request_load(self.pane.current_dir.clone(), CursorFocus::Keep)
    }

    /// How many entries `{files}` stands for now (R-400), for the title of the input box.
    pub fn command_target_count(&self) -> usize {
        crate::ops::target::targets(&self.pane).len()
    }

    /// x in Normal (R-412). Marks win over whatever the cursor is on: they were chosen on purpose.
    /// A file goes to the event loop, which can look at the disk; `[..]` or nothing -> status.
    pub(crate) fn begin_command_on_target(&mut self) -> Effect {
        if !self.pane.marked_entries().is_empty() {
            self.open_command_box(" {files}", true);
            return Effect::None;
        }
        let Some(entry) = self.pane.cursor_entry().filter(|e| !e.is_parent_link()) else {
            self.status = Some(Message::NothingSelected);
            return Effect::None;
        };
        if matches!(
            entry.kind,
            EntryKind::File | EntryKind::Symlink(LinkTarget::File | LinkTarget::Unknown)
        ) {
            return Effect::PrefillCommand {
                path: entry.path.clone(),
            };
        }
        self.open_command_box(" {file}", true);
        Effect::None
    }

    fn open_command_box(&mut self, text: &str, cursor_at_start: bool) {
        let mut editor = LineEditor::new(text);
        if cursor_at_start {
            editor.home();
        }
        self.input = Some(InputState {
            purpose: InputPurpose::Command {
                history_pos: None,
                draft: String::new(),
            },
            editor,
        });
        self.mode = Mode::Input;
    }

    /// Enter in the command box. A refused line stays in the box as it was, with the reason in
    /// status. `cd` moves the list, which a child process could not do.
    fn submit_command(&mut self, input: InputState) -> Effect {
        let kind = self.shell_kind;
        let dir = self.pane.current_dir.clone();
        let template = input.editor.text().to_owned();
        let cursor = self
            .pane
            .cursor_entry()
            .filter(|e| !e.is_parent_link())
            .map(|e| e.name.clone());
        let targets: Vec<OsString> = crate::ops::target::targets(&self.pane)
            .iter()
            .filter_map(|path| path.file_name().map(OsString::from))
            .collect();
        let refuse = |app: &mut Self, input: InputState, reason: CommandError| {
            app.input = Some(input);
            app.status = Some(Message::CommandRejected(reason));
            Effect::None
        };
        let line = match shell_cmd::expand(&template, cursor.as_deref(), &targets, kind) {
            Ok(line) => line,
            Err(reason) => return refuse(self, input, reason),
        };
        let cd = shell_cmd::parse_cd(&line, kind);
        if cd.is_none()
            && let Err(reason) = shell_cmd::check_folder(&dir, kind)
        {
            return refuse(self, input, reason);
        }
        if self.command_history.last() != Some(&template) {
            self.command_history.push(template);
            if self.command_history.len() > COMMAND_HISTORY_LIMIT {
                self.command_history.remove(0);
            }
        }
        self.mode = Mode::Normal;
        match cd {
            Some(path) => self.request_load(shell_cmd::resolve_cd(&dir, &path), CursorFocus::First),
            None => Effect::RunShell { dir, line },
        }
    }

    /// Up / Down in the command box: older / newer lines of this session, then back to what was
    /// being typed.
    fn walk_command_history(&self, input: &mut InputState, key: TextKey) {
        let InputPurpose::Command { history_pos, draft } = &mut input.purpose else {
            return;
        };
        let len = self.command_history.len();
        let next = match (key, *history_pos) {
            (TextKey::Up, None) if len > 0 => {
                *draft = input.editor.text().to_owned();
                Some(len - 1)
            }
            (TextKey::Up, Some(pos)) => Some(pos.saturating_sub(1)),
            (TextKey::Down, Some(pos)) if pos + 1 < len => Some(pos + 1),
            (TextKey::Down, Some(_)) => None,
            _ => return,
        };
        *history_pos = next;
        let text = next.map_or(draft.as_str(), |pos| self.command_history[pos].as_str());
        input.editor = LineEditor::new(text);
    }

    /// The editor exited (P3-15). Err -> status `EditorFailed`. Always reloads the current
    /// directory (the file may have changed), CursorFocus::Keep.
    pub fn on_editor_finished(&mut self, result: Result<(), String>) -> Effect {
        if let Err(reason) = result {
            self.status = Some(Message::EditorFailed { reason });
        }
        self.request_load(self.pane.current_dir.clone(), CursorFocus::Keep)
    }

    fn start_operation(&mut self, op: Op, sources: Vec<PathBuf>, dest: Option<PathBuf>) -> Effect {
        if sources.is_empty() {
            self.mode = Mode::Normal;
            self.status = Some(Message::NothingSelected);
            return Effect::None;
        }
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.job = Some(JobState {
            id,
            op,
            sources: sources.clone(),
            dest: dest.clone(),
            total_files: 0,
            total_bytes: 0,
            done_files: 0,
            done_bytes: 0,
            current: None,
            cancelling: false,
        });
        self.failures.clear();
        self.conflict = None;
        self.status = None;
        self.mode = Mode::Running;
        Effect::StartJob(Job {
            id,
            op,
            sources,
            dest,
        })
    }

    fn edit_input(&mut self, key: TextKey) -> Effect {
        let Some(mut input) = self.input.take() else {
            return Effect::None;
        };
        if matches!(input.purpose, InputPurpose::Command { .. }) {
            match key {
                TextKey::Enter => return self.submit_command(input),
                TextKey::Esc => {}
                // A refusal is about the line as it was.
                _ => self.status = None,
            }
        }
        if matches!(key, TextKey::Enter | TextKey::Esc) {
            self.mode = Mode::Normal;
            match input.purpose {
                InputPurpose::Search {
                    origin_cursor,
                    return_mode,
                } => {
                    self.mode = return_mode;
                    if key == TextKey::Esc {
                        self.pane.cursor = origin_cursor;
                    }
                }
                InputPurpose::ArchiveExtract { return_mode, .. } if key == TextKey::Esc => {
                    self.mode = return_mode;
                    self.status = Some(Message::Cancelled);
                }
                _ if key == TextKey::Esc => self.status = Some(Message::Cancelled),
                InputPurpose::Rename { dir, old } => {
                    let new = input.editor.text().to_owned();
                    if new != old {
                        return Effect::Rename { dir, old, new };
                    }
                }
                InputPurpose::Mkdir => {
                    let name = input.editor.text().to_owned();
                    if !name.is_empty() {
                        return Effect::MakeDir {
                            dir: self.pane.current_dir.clone(),
                            name,
                        };
                    }
                }
                InputPurpose::ArchiveCreate { sources, dir } => {
                    let name = input.editor.text();
                    if crate::ops::name::validate(name, cfg!(windows)).is_err()
                        || crate::archive::format(std::path::Path::new(name)).is_none()
                    {
                        self.status = Some(Message::OperationFailed {
                            name: name.to_string(),
                            reason: "invalid_name".into(),
                        });
                    } else {
                        return self.start_archive_action(crate::archive::Action::Create {
                            destination: dir.join(name),
                            sources,
                        });
                    }
                }
                // Enter was handled above.
                InputPurpose::Command { .. } => {}
                InputPurpose::ArchiveExtract { source, dir, .. } => {
                    let name = input.editor.text();
                    if crate::ops::name::validate(name, cfg!(windows)).is_err() {
                        self.status = Some(Message::OperationFailed {
                            name: name.to_string(),
                            reason: "invalid_name".into(),
                        });
                    } else {
                        return self.start_archive_action(crate::archive::Action::Extract {
                            source,
                            destination: dir.join(name),
                        });
                    }
                }
            }
            return Effect::None;
        }
        match key {
            TextKey::Char(c) => input.editor.insert(c),
            TextKey::Backspace => input.editor.backspace(),
            TextKey::Delete => input.editor.delete(),
            TextKey::Left => input.editor.left(),
            TextKey::Right => input.editor.right(),
            TextKey::Home => input.editor.home(),
            TextKey::End => input.editor.end(),
            TextKey::Up | TextKey::Down => self.walk_command_history(&mut input, key),
            _ => {}
        }
        if matches!(input.purpose, InputPurpose::Search { .. }) {
            let names: Vec<&str> = self
                .pane
                .entries
                .iter()
                .map(|e| e.display_name.as_str())
                .collect();
            let (start, direction) = match key {
                TextKey::Down => (self.pane.cursor.saturating_add(1), Direction::Forward),
                TextKey::Up => (
                    self.pane
                        .cursor
                        .checked_sub(1)
                        .unwrap_or_else(|| names.len().saturating_sub(1)),
                    Direction::Backward,
                ),
                _ => (self.pane.cursor, Direction::Forward),
            };
            if let Some(index) = crate::search::find(&names, start, input.editor.text(), direction)
            {
                self.pane.cursor = index;
            }
        }
        self.input = Some(input);
        Effect::None
    }
}
