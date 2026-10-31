//! Executes Copy / Move / Delete on a worker thread (P3-3 to P3-11).
//!
//! Protocol with the UI:
//! - The worker sends `JobEvent`s. `Finished` is always the last one.
//! - On `JobEvent::Conflict` it blocks on `answers` until an `Answer` arrives (or the channel
//!   closes, which counts as Cancel).
//! - `cancel` is checked between entries and after every chunk while copying.
//!
//! Safety rules (data loss is the worst failure):
//! - Never write a destination file under its final name while it is incomplete: copy into a
//!   temporary sibling `.<name>.fds-partial.<pid>-<job id>` opened with `create_new` (never
//!   overwrites anything), then rename it to the final name. On cancel or error remove the temp.
//! - Never overwrite without a `Resolution::Overwrite` answer (or an earlier "for all").
//! - Never follow symlinks while walking; copy and delete links as links.
//! - Move deletes a source only after that source was fully copied (cross-device fallback).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use super::{Answer, Job, JobEvent, Op, Resolution, Summary};

/// Bytes per read/write chunk; cancellation is checked after each chunk.
pub const CHUNK: usize = 1024 * 1024;

/// Test hooks. Production code uses `Hooks::default()`.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    /// Treat every `fs::rename` in Move as a cross-device failure, to exercise the copy+delete
    /// fallback on a single filesystem (P3-8).
    pub force_cross_device: bool,
    /// Fail the file copy with an I/O error after this many bytes of a file were written, to test
    /// that no partial file remains under the final name (P3-4).
    pub fail_after_bytes: Option<u64>,
    /// Behave as if the destination filesystem cannot create hard links (FAT32, exFAT, some
    /// network shares), to test the fallback in `publish`.
    pub no_hard_links: bool,
    /// Used instead of `trash::move_to_trash` for `Op::Trash`, so tests do not fill the real
    /// trash (and can make single items fail).
    pub trash: Option<fn(&Path) -> io::Result<()>>,
}

/// Runs `job` to completion and returns the summary (also sent as `JobEvent::Finished`).
///
/// Behavior:
/// 1. Scan: walk all sources without following symlinks, count files (non-directories) and bytes
///    (regular files only), send `Scanned`. Check `cancel` while walking.
/// 2. For each source in order:
///    - Copy / Move: the destination is `dest.join(source.file_name())`.
///      * If `dest` path equals the source path (same directory copy): use
///        `name::unique_name` for a free name, no conflict question.
///      * If the destination exists: ask (`Conflict`) unless an earlier `for_all` answer applies.
///        Overwrite: file over file -> replace after the temp copy completes; directory over
///        directory -> merge (recurse, asking per nested conflict); file vs directory mismatch ->
///        `EntryFailed` (never delete a directory to put a file there, or vice versa).
///        Skip -> count as skipped. Rename -> `name::unique_name`. Cancel -> stop the job.
///      * Regular file: temp copy in `CHUNK`s, then set permissions and modified time from the
///        source, then rename to the final name. Send `Progress` after each file (and during
///        large files at least every 16 chunks).
///      * Directory: create it (or reuse on merge), then recurse in sorted order.
///      * Symlink: recreate the link with the same target (`read_link`). On Windows use
///        `symlink_file` / `symlink_dir` depending on the target; failure -> `EntryFailed`.
///      * Move: first try `fs::rename(source, destination)` (unless `hooks.force_cross_device`).
///        If it fails with a cross-device error (Unix EXDEV = 18, Windows ERROR_NOT_SAME_DEVICE
///        = 17) or the hook is set, copy as above and remove each source entry only after it was
///        copied completely; if anything inside a source directory failed or was skipped, keep
///        that source directory.
///    - Delete: depth-first, files and links before their directory; links are removed as links.
///      Failures -> `EntryFailed`, continue with the rest.
///    - Trash (M1-a): each source as given (never walked, never canonicalized, so a link is moved
///      as the link) goes to `hooks.trash` or `trash::move_to_trash`. Ok -> counted as done with
///      the scanned counts of that source and a `Progress`. Err -> `EntryFailed` and continue with
///      the next source (C39). The worker itself never removes anything for Trash.
/// 3. Any per-entry I/O error -> `EntryFailed { path, reason }` and continue.
/// 4. On cancel: remove the current temp file, stop, `Summary.cancelled = true`.
///    On cancel or failure inside a directory this job created, remove that directory if it
///    is still empty (a merged, pre-existing directory is never removed).
/// 5. Send `Finished(summary)` and return it.
pub fn run(
    job: &Job,
    events: &Sender<JobEvent>,
    answers: &Receiver<Answer>,
    cancel: &Arc<AtomicBool>,
    hooks: &Hooks,
) -> Summary {
    let mut worker = Worker {
        job,
        events,
        answers,
        cancel,
        hooks,
        summary: Summary::default(),
        for_all: None,
        done_files: 0,
        done_bytes: 0,
        counts: HashMap::new(),
        scan_failures: HashSet::new(),
    };
    let mut total = Counts::default();
    for source in &job.sources {
        if worker.stopped() {
            break;
        }
        total.add(worker.scan(source, 0));
    }
    worker.emit(JobEvent::Scanned {
        files: total.files,
        bytes: total.bytes,
    });
    for source in &job.sources {
        if worker.stopped() {
            break;
        }
        if job.op == Op::Trash {
            let move_to_trash = hooks.trash.unwrap_or(super::trash::move_to_trash);
            match move_to_trash(source) {
                Ok(()) => {
                    let counts = worker.counts.get(source).copied().unwrap_or_else(|| {
                        fs::symlink_metadata(source)
                            .map(|metadata| entry_counts(&metadata))
                            .unwrap_or(Counts {
                                entries: 1,
                                ..Counts::default()
                            })
                    });
                    worker.completed(source, counts);
                }
                Err(error) => worker.fail(source, error),
            }
        } else if job.op == Op::Delete {
            worker.delete(source, 0);
        } else if let Some(dest) = &job.dest {
            let Some(name) = source.file_name() else {
                worker.fail(source, io::Error::other("source has no file name"));
                continue;
            };
            // The UI only checks lexical containment; resolve aliases here before any writes.
            if fs::symlink_metadata(source).is_ok_and(|m| m.is_dir())
                && super::check::source_containing(std::slice::from_ref(source), dest).is_some()
            {
                worker.fail(source, io::Error::other("destination is inside the source"));
                continue;
            }
            let destination = dest.join(name);
            let same = destination == *source
                || fs::canonicalize(source)
                    .ok()
                    .zip(fs::canonicalize(&destination).ok())
                    .is_some_and(|(a, b)| a == b);
            if same && job.op == Op::Move {
                // Moving into the directory it is already in: nothing to do (never rename it).
                worker.summary.skipped = worker.summary.skipped.saturating_add(1);
                continue;
            }
            let destination = if same {
                match free_destination(&destination) {
                    Ok(path) => path,
                    Err(error) => {
                        worker.fail(source, error);
                        continue;
                    }
                }
            } else {
                destination
            };
            worker.transfer(source, &destination, 0);
        } else {
            worker.fail(source, io::Error::other("missing destination directory"));
        }
    }
    worker.stopped();
    worker.emit(JobEvent::Finished(worker.summary.clone()));
    worker.summary
}

#[derive(Clone, Copy, Default)]
struct Counts {
    entries: u64,
    files: u64,
    bytes: u64,
}

impl Counts {
    fn add(&mut self, other: Self) {
        self.entries = self.entries.saturating_add(other.entries);
        self.files = self.files.saturating_add(other.files);
        self.bytes = self.bytes.saturating_add(other.bytes);
    }
}

struct Worker<'a> {
    job: &'a Job,
    events: &'a Sender<JobEvent>,
    answers: &'a Receiver<Answer>,
    cancel: &'a AtomicBool,
    hooks: &'a Hooks,
    summary: Summary,
    for_all: Option<Resolution>,
    done_files: u64,
    done_bytes: u64,
    counts: HashMap<PathBuf, Counts>,
    scan_failures: HashSet<PathBuf>,
}

impl Worker<'_> {
    fn emit(&self, event: JobEvent) {
        let _ = self.events.send(event);
    }

    fn stopped(&mut self) -> bool {
        self.summary.cancelled |= self.cancel.load(Ordering::Relaxed);
        self.summary.cancelled
    }

    fn fail(&mut self, path: &Path, error: io::Error) {
        let reason = error.to_string();
        self.summary
            .failed
            .push((path.to_path_buf(), reason.clone()));
        self.emit(JobEvent::EntryFailed {
            path: path.to_path_buf(),
            reason,
        });
    }

    fn progress(&self, path: &Path) {
        self.emit(JobEvent::Progress {
            done_files: self.done_files,
            done_bytes: self.done_bytes,
            current: path.to_path_buf(),
        });
    }

    fn completed(&mut self, path: &Path, counts: Counts) {
        self.summary.done = self.summary.done.saturating_add(counts.entries);
        self.done_files = self.done_files.saturating_add(counts.files);
        self.done_bytes = self.done_bytes.saturating_add(counts.bytes);
        self.progress(path);
    }

    fn scan(&mut self, path: &Path, depth: usize) -> Counts {
        if self.stopped() {
            return Counts::default();
        }
        let result = (|| {
            check_depth(depth)?;
            let metadata = fs::symlink_metadata(path)?;
            let mut counts = entry_counts(&metadata);
            if metadata.is_dir() {
                for child in children(path)? {
                    if self.stopped() {
                        break;
                    }
                    counts.add(self.scan(&child, depth.saturating_add(1)));
                }
            }
            Ok(counts)
        })();
        match result {
            Ok(counts) => {
                self.counts.insert(path.to_path_buf(), counts);
                counts
            }
            Err(error) => {
                self.scan_failures.insert(path.to_path_buf());
                self.fail(path, error);
                Counts::default()
            }
        }
    }

    fn resolve(&mut self, source: &Path, dest: &Path, dest_is_dir: bool) -> Resolution {
        if self.stopped() {
            return Resolution::Cancel;
        }
        if let Some(resolution) = self.for_all {
            return resolution;
        }
        if self
            .events
            .send(JobEvent::Conflict {
                source: source.to_path_buf(),
                dest: dest.to_path_buf(),
                dest_is_dir,
            })
            .is_err()
        {
            self.summary.cancelled = true;
            return Resolution::Cancel;
        }
        loop {
            if self.stopped() {
                return Resolution::Cancel;
            }
            match self.answers.recv_timeout(Duration::from_millis(50)) {
                Ok(answer) => {
                    if answer.for_all {
                        self.for_all = Some(answer.resolution);
                    }
                    if answer.resolution == Resolution::Cancel {
                        self.summary.cancelled = true;
                    }
                    return answer.resolution;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.summary.cancelled = true;
                    return Resolution::Cancel;
                }
            }
        }
    }

    fn transfer(&mut self, source: &Path, dest: &Path, depth: usize) -> bool {
        if self.stopped() || self.scan_failures.contains(source) {
            return false;
        }
        match self.transfer_entry(source, dest, depth) {
            Ok(complete) => complete,
            Err(error) => {
                self.fail(source, error);
                false
            }
        }
    }

    fn transfer_entry(&mut self, source: &Path, dest: &Path, depth: usize) -> io::Result<bool> {
        check_depth(depth)?;
        let metadata = fs::symlink_metadata(source)?;
        let mut destination = dest.to_path_buf();
        let mut existing = metadata_if_present(&destination)?;
        let mut overwrite = false;
        if let Some(target) = &existing {
            match self.resolve(source, &destination, target.is_dir()) {
                Resolution::Cancel => return Ok(false),
                Resolution::Skip => {
                    self.summary.skipped = self.summary.skipped.saturating_add(1);
                    return Ok(false);
                }
                Resolution::Rename => {
                    destination = free_destination(&destination)?;
                    existing = None;
                }
                Resolution::Overwrite => {
                    if metadata.is_dir() != target.is_dir() {
                        return Err(io::Error::other("file/directory conflict"));
                    }
                    overwrite = true;
                }
            }
        }
        if self.stopped() {
            return Ok(false);
        }
        let merging = metadata.is_dir() && existing.is_some();
        let scan_incomplete = self.scan_failures.iter().any(|p| p.starts_with(source));
        if self.job.op == Op::Move && !merging && !self.hooks.force_cross_device && !scan_incomplete
        {
            // Recheck just before rename so a newly appeared destination is never silently replaced.
            ensure_destination(&destination, overwrite, metadata.is_dir())?;
            match fs::rename(source, &destination) {
                Ok(()) => {
                    let counts = self
                        .counts
                        .get(source)
                        .copied()
                        .unwrap_or_else(|| entry_counts(&metadata));
                    self.completed(source, counts);
                    return Ok(true);
                }
                Err(error) if is_cross_device(&error) => {}
                Err(error) => return Err(error),
            }
        }
        if metadata.is_dir() {
            if !merging {
                fs::create_dir(&destination)?;
            }
            let filled = (|| {
                let mut complete = true;
                for child in children(source)? {
                    if self.stopped() {
                        return Ok(false);
                    }
                    let Some(name) = child.file_name() else {
                        return Err(io::Error::other("entry has no file name"));
                    };
                    if !self.transfer(&child, &destination.join(name), depth.saturating_add(1)) {
                        complete = false;
                    }
                }
                Ok(complete && !self.stopped())
            })();
            if !matches!(filled, Ok(true)) {
                // Do not leave behind an empty directory this job created (cancel, failure).
                // remove_dir fails on a non-empty one, so finished entries are kept.
                if !merging {
                    let _ = fs::remove_dir(&destination);
                }
                return filled;
            }
            if self.job.op == Op::Move {
                fs::remove_dir(source)?;
            }
            self.completed(
                source,
                Counts {
                    entries: 1,
                    ..Counts::default()
                },
            );
            return Ok(true);
        }
        let bytes_before = self.done_bytes;
        let copied = if metadata.file_type().is_symlink() {
            self.copy_link(source, &destination, &metadata, overwrite)?
        } else if metadata.is_file() {
            self.copy_file(source, &destination, &metadata, overwrite)?
        } else {
            return Err(io::Error::other("unsupported file type"));
        };
        if !copied {
            return Ok(false);
        }
        if self.job.op == Op::Move {
            remove_leaf(source, &metadata)?;
        }
        let mut counts = entry_counts(&metadata);
        // File bytes were already included in chunk progress.
        counts.bytes = counts
            .bytes
            .saturating_sub(self.done_bytes.saturating_sub(bytes_before));
        self.completed(source, counts);
        Ok(true)
    }

    fn copy_file(
        &mut self,
        source: &Path,
        dest: &Path,
        metadata: &Metadata,
        overwrite: bool,
    ) -> io::Result<bool> {
        let temporary = temporary_path(dest, self.job.id)?;
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        // Only this successfully created file belongs to us; a pre-existing temp is never removed.
        let result = (|| {
            let mut buffer = vec![0_u8; CHUNK];
            let mut written = 0_u64;
            let mut chunks = 0_u8;
            loop {
                if self.stopped() {
                    return Ok(false);
                }
                if self
                    .hooks
                    .fail_after_bytes
                    .is_some_and(|limit| written >= limit)
                {
                    return Err(io::Error::other("injected copy failure"));
                }
                let size = match input.read(&mut buffer) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                if size == 0 {
                    break;
                }
                let chunk = buffer
                    .get(..size)
                    .ok_or_else(|| io::Error::other("invalid read size"))?;
                output.write_all(chunk)?;
                written = written.saturating_add(size as u64);
                self.done_bytes = self.done_bytes.saturating_add(size as u64);
                chunks = chunks.saturating_add(1);
                if chunks >= 16 {
                    self.progress(source);
                    chunks = 0;
                }
                if self.stopped() {
                    return Ok(false);
                }
            }
            output.set_modified(metadata.modified()?)?;
            output.set_permissions(metadata.permissions())?;
            output.sync_all()?;
            Ok(true)
        })();
        drop(output);
        let result = match result {
            Ok(true) if !self.stopped() => {
                publish(&temporary, dest, overwrite, self.hooks.no_hard_links).map(|()| true)
            }
            Ok(_) => Ok(false),
            Err(error) => Err(error),
        };
        finish_temporary(&temporary, result)
    }

    fn copy_link(
        &mut self,
        source: &Path,
        dest: &Path,
        metadata: &Metadata,
        overwrite: bool,
    ) -> io::Result<bool> {
        let target = fs::read_link(source)?;
        if !overwrite {
            if self.stopped() {
                return Ok(false);
            }
            // Link creation is atomic and exclusive; no incomplete link needs staging.
            create_link(&target, dest, metadata)?;
            return Ok(true);
        }
        let temporary = temporary_path(dest, self.job.id)?;
        create_link(&target, &temporary, metadata)?;
        let result = if self.stopped() {
            Ok(false)
        } else {
            ensure_destination(dest, true, false)
                .and_then(|()| fs::rename(&temporary, dest))
                .map(|()| true)
        };
        finish_temporary(&temporary, result)
    }

    fn delete(&mut self, path: &Path, depth: usize) -> bool {
        if self.stopped() || self.scan_failures.contains(path) {
            return false;
        }
        let result = (|| {
            check_depth(depth)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.is_dir() {
                let mut complete = true;
                for child in children(path)? {
                    if self.stopped() {
                        return Ok(false);
                    }
                    if !self.delete(&child, depth.saturating_add(1)) {
                        complete = false;
                    }
                }
                if !complete || self.stopped() {
                    return Ok(false);
                }
                fs::remove_dir(path)?;
            } else {
                if self.stopped() {
                    return Ok(false);
                }
                remove_leaf(path, &metadata)?;
            }
            self.completed(path, entry_counts(&metadata));
            Ok(true)
        })();
        match result {
            Ok(complete) => complete,
            Err(error) => {
                self.fail(path, error);
                false
            }
        }
    }
}

fn check_depth(depth: usize) -> io::Result<()> {
    // Bound stack use for arbitrarily deep or concurrently changing trees.
    if depth > 256 {
        Err(io::Error::other("directory nesting is too deep"))
    } else {
        Ok(())
    }
}

fn entry_counts(metadata: &Metadata) -> Counts {
    Counts {
        entries: 1,
        files: u64::from(!metadata.is_dir()),
        bytes: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
    }
}

fn children(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(path)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

fn metadata_if_present(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn free_destination(path: &Path) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("destination has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("destination has no file name"))?
        .to_string_lossy();
    let unique = super::name::unique_name(
        &name,
        &|candidate| !matches!(fs::symlink_metadata(parent.join(candidate)), Err(error) if error.kind() == io::ErrorKind::NotFound),
    );
    let destination = parent.join(unique);
    ensure_destination(&destination, false, false)?;
    Ok(destination)
}

fn temporary_path(dest: &Path, job_id: u64) -> io::Result<PathBuf> {
    let name = dest
        .file_name()
        .ok_or_else(|| io::Error::other("destination has no file name"))?;
    Ok(dest.with_file_name(temp_name(&name.to_string_lossy(), job_id)))
}

fn ensure_destination(dest: &Path, overwrite: bool, is_dir: bool) -> io::Result<()> {
    if let Some(metadata) = metadata_if_present(dest)? {
        if !overwrite {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination already exists",
            ));
        }
        if metadata.is_dir() != is_dir {
            return Err(io::Error::other("file/directory conflict"));
        }
    }
    Ok(())
}

fn publish(temporary: &Path, dest: &Path, overwrite: bool, no_hard_links: bool) -> io::Result<()> {
    ensure_destination(dest, overwrite, false)?;
    if overwrite {
        fs::rename(temporary, dest)
    } else {
        // Claim the final name with the completed inode, never an empty placeholder.
        // hard_link is exclusive, unlike rename on Unix, so a late arrival is preserved.
        if let Err(error) = link_exclusive(temporary, dest, no_hard_links) {
            if error.kind() == io::ErrorKind::AlreadyExists {
                return Err(error);
            }
            // No hard links here (FAT32, exFAT, some network shares). Fall back to a rename
            // after re-checking that the name is still free; a tiny race window remains only on
            // such filesystems.
            ensure_destination(dest, false, false)?;
            return fs::rename(temporary, dest);
        }
        #[cfg(windows)]
        let permissions = {
            let permissions = fs::symlink_metadata(temporary)?.permissions();
            // Both links belong to this copy; restore this attribute on the final link below.
            if permissions.readonly() {
                let mut writable = permissions.clone();
                #[allow(clippy::permissions_set_readonly_false)]
                writable.set_readonly(false);
                fs::set_permissions(temporary, writable)?;
            }
            permissions
        };
        match fs::rename(temporary, dest) {
            Ok(()) => {
                // Renaming two links to the same inode can be a successful no-op.
                if let Some(metadata) = metadata_if_present(temporary)? {
                    remove_leaf(temporary, &metadata)?;
                }
                #[cfg(windows)]
                fs::set_permissions(dest, permissions)?;
                Ok(())
            }
            Err(error) => {
                fs::remove_file(dest).map_err(|cleanup| {
                    io::Error::other(format!("{error}; cleanup failed: {cleanup}"))
                })?;
                Err(error)
            }
        }
    }
}

/// Creates `dest` as a hard link to `temporary`. Fails with AlreadyExists if `dest` exists.
fn link_exclusive(temporary: &Path, dest: &Path, no_hard_links: bool) -> io::Result<()> {
    if no_hard_links {
        return Err(io::Error::from(io::ErrorKind::Unsupported));
    }
    fs::hard_link(temporary, dest)
}

fn finish_temporary(temporary: &Path, result: io::Result<bool>) -> io::Result<bool> {
    if matches!(result, Ok(true)) {
        return result;
    }
    if let Some(metadata) = metadata_if_present(temporary)? {
        #[cfg(windows)]
        if metadata.is_file() && metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            // This branch only clears the Windows read-only attribute on our own temp.
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(temporary, permissions)?;
        }
        if let Err(cleanup) = remove_leaf(temporary, &metadata) {
            let reason = match result {
                Err(error) => format!("{error}; "),
                _ => String::new(),
            };
            return Err(io::Error::other(format!(
                "{reason}temporary cleanup failed: {cleanup}"
            )));
        }
    }
    result
}

fn remove_leaf(path: &Path, _metadata: &Metadata) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if _metadata.file_type().is_symlink_dir() {
            return fs::remove_dir(path);
        }
    }
    fs::remove_file(path)
}

fn create_link(target: &Path, link: &Path, _metadata: &Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{FileTypeExt, symlink_dir, symlink_file};
        if _metadata.file_type().is_symlink_dir() {
            symlink_dir(target, link)
        } else {
            symlink_file(target, link)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symlinks are unsupported",
        ))
    }
}

/// Temporary file name for copying into `dest_dir` (used by `run`; public for tests).
/// Format: `.<name>.fds-partial.<pid>-<job_id>`.
pub fn temp_name(name: &str, job_id: u64) -> String {
    format!(".{name}.fds-partial.{}-{job_id}", std::process::id())
}

/// True for the OS error meaning "rename across filesystems".
pub fn is_cross_device(error: &std::io::Error) -> bool {
    match error.raw_os_error() {
        Some(code) if cfg!(unix) => code == 18,
        Some(code) if cfg!(windows) => code == 17,
        _ => false,
    }
}

/// Size in bytes of a regular file without following links (0 for others). Used by the scan.
pub fn entry_bytes(path: &Path) -> u64 {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .unwrap_or(0)
}
