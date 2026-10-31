//! File operations (design prompt §15, Phase 3 / R-400 to R-406).
//!
//! Pure parts: `target` (what to operate on), `name` (validation, unique names), `check` (self-
//! containment). The destructive work runs in `worker` on a separate thread and talks to the UI
//! through `JobEvent` / `Resolution` channels. `simple` holds the short, synchronous rename/mkdir.

pub mod check;
pub mod name;
pub mod place;
pub mod simple;
pub mod target;
pub mod trash;
pub mod worker;

use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Copy,
    Move,
    /// Permanent deletion (R-402-2).
    Delete,
    /// Move to the platform trash (R-402-3, M1-a). Never deletes permanently.
    Trash,
}

/// What F8 does, from `[ops] delete` (R-402-3, M1-a). The default stays permanent (R-402-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeleteMode {
    #[default]
    Permanent,
    Trash,
}

/// A destructive job for the worker. `dest` is the destination directory (None for Delete).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    pub op: Op,
    pub sources: Vec<PathBuf>,
    pub dest: Option<PathBuf>,
}

/// Messages from the worker to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    /// Pre-scan finished: totals for the progress bar.
    Scanned { files: u64, bytes: u64 },
    Progress {
        done_files: u64,
        done_bytes: u64,
        /// Path currently being processed (display only).
        current: PathBuf,
    },
    /// `dest` already exists. The worker waits for a `Resolution`.
    Conflict {
        source: PathBuf,
        dest: PathBuf,
        dest_is_dir: bool,
    },
    /// One entry failed; the job continues with the next one.
    EntryFailed { path: PathBuf, reason: String },
    /// The job ended (normally or cancelled). Always the last event of a job.
    Finished(Summary),
}

/// Answer to a `JobEvent::Conflict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Overwrite,
    Skip,
    /// Use a free name like "name (2).ext".
    Rename,
    /// Stop the whole job.
    Cancel,
}

/// A resolution plus "apply to all remaining conflicts of this job".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub resolution: Resolution,
    pub for_all: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// Top-level or nested entries completed (files, links and directories).
    pub done: u64,
    pub skipped: u64,
    pub failed: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

/// One finished job, kept as the base for a future history / undo UI (P3-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpRecord {
    pub op: Op,
    pub sources: Vec<PathBuf>,
    pub dest: Option<PathBuf>,
    pub summary: Summary,
    pub finished_at: SystemTime,
}

/// History capacity; older records are dropped first.
pub const HISTORY_LIMIT: usize = 100;
