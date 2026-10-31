//! Copy / Move / Delete on real files (P3-3 to P3-11). Every destructive path has a success,
//! a failure and a boundary case (§36, §40).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use fds::ops::worker::{Hooks, is_cross_device, run, temp_name};
use fds::ops::{Answer, Job, JobEvent, Op, Resolution, Summary};

/// Runs a job, auto-answering every conflict with `answer`. Returns (summary, events).
fn run_job(job: &Job, answer: Answer, hooks: Hooks) -> (Summary, Vec<JobEvent>) {
    let (event_tx, event_rx): (Sender<JobEvent>, Receiver<JobEvent>) = mpsc::channel();
    let (answer_tx, answer_rx) = mpsc::channel::<Answer>();
    let cancel = Arc::new(AtomicBool::new(false));
    let job2 = job.clone();
    let handle = thread::spawn(move || run(&job2, &event_tx, &answer_rx, &cancel, &hooks));
    let mut events = Vec::new();
    while let Ok(event) = event_rx.recv() {
        let conflict = matches!(event, JobEvent::Conflict { .. });
        let finished = matches!(event, JobEvent::Finished(_));
        events.push(event);
        if conflict {
            let _ = answer_tx.send(answer);
        }
        if finished {
            break;
        }
    }
    (handle.join().unwrap(), events)
}

fn job(id: u64, op: Op, sources: Vec<PathBuf>, dest: Option<PathBuf>) -> Job {
    Job {
        id,
        op,
        sources,
        dest,
    }
}

fn overwrite() -> Answer {
    Answer {
        resolution: Resolution::Overwrite,
        for_all: true,
    }
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap()
}

// ---------- Copy ----------

#[test]
fn copy_file_directory_tree_and_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("a.txt"), "hello").unwrap();
    fs::write(src.join("sub/b.txt"), "world").unwrap();

    let (summary, _) = run_job(
        &job(1, Op::Copy, vec![src.clone()], Some(dst.clone())),
        overwrite(),
        Hooks::default(),
    );
    assert!(summary.failed.is_empty() && !summary.cancelled);
    assert_eq!(read(&dst.join("src/a.txt")), "hello");
    assert_eq!(read(&dst.join("src/sub/b.txt")), "world");
    assert!(fs::metadata(src.join("a.txt")).is_ok(), "source is kept");
}

#[cfg(unix)]
#[test]
fn copy_preserves_permissions_and_links_are_links() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("run.sh"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(src.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink("run.sh", src.join("link")).unwrap();

    run_job(
        &job(1, Op::Copy, vec![src.clone()], Some(dst.clone())),
        overwrite(),
        Hooks::default(),
    );
    let mode = fs::metadata(dst.join("src/run.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755);
    let link = dst.join("src/link");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), PathBuf::from("run.sh"));
}

#[test]
fn copy_failure_leaves_no_partial_file_under_the_final_name() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("big.bin");
    let dst = tmp.path().join("out");
    fs::create_dir(&dst).unwrap();
    fs::write(&src, vec![7u8; 4 * 1024 * 1024]).unwrap();
    let hooks = Hooks {
        fail_after_bytes: Some(1024 * 1024),
        ..Hooks::default()
    };

    let (summary, _) = run_job(
        &job(1, Op::Copy, vec![src], Some(dst.clone())),
        overwrite(),
        hooks,
    );
    assert_eq!(summary.failed.len(), 1);
    assert!(
        !dst.join("big.bin").exists(),
        "no file under the final name"
    );
    let leftovers: Vec<_> = fs::read_dir(&dst).unwrap().filter_map(Result::ok).collect();
    assert!(leftovers.is_empty(), "temp file removed: {leftovers:?}");
}

#[test]
fn copy_into_same_directory_auto_renames() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("note.txt"), "x").unwrap();
    let (summary, _) = run_job(
        &job(
            1,
            Op::Copy,
            vec![dir.join("note.txt")],
            Some(dir.to_path_buf()),
        ),
        overwrite(),
        Hooks::default(),
    );
    assert!(summary.failed.is_empty());
    assert!(dir.join("note (2).txt").exists());
    assert!(dir.join("note.txt").exists());
}

#[test]
fn temp_name_includes_pid_and_job() {
    let name = temp_name("a.txt", 7);
    assert!(name.starts_with(".a.txt.fds-partial."));
    assert!(name.ends_with("-7"));
    assert!(name.contains(&std::process::id().to_string()));
}

// ---------- conflicts (P3-6) ----------

#[test]
fn conflict_skip_keeps_the_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(dst.join("src")).unwrap();
    fs::write(src.join("a.txt"), "new").unwrap();
    fs::write(dst.join("src/a.txt"), "old").unwrap();

    let skip = Answer {
        resolution: Resolution::Skip,
        for_all: true,
    };
    let (summary, events) = run_job(
        &job(1, Op::Copy, vec![src], Some(dst.clone())),
        skip,
        Hooks::default(),
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, JobEvent::Conflict { .. }))
    );
    assert_eq!(read(&dst.join("src/a.txt")), "old");
    assert!(summary.skipped >= 1);
}

#[test]
fn conflict_overwrite_replaces_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path().join("dst");
    fs::create_dir(&dst).unwrap();
    fs::write(tmp.path().join("a.txt"), "new").unwrap();
    fs::write(dst.join("a.txt"), "old").unwrap();
    run_job(
        &job(
            1,
            Op::Copy,
            vec![tmp.path().join("a.txt")],
            Some(dst.clone()),
        ),
        overwrite(),
        Hooks::default(),
    );
    assert_eq!(read(&dst.join("a.txt")), "new");
}

// ---------- Move (P3-8) ----------

#[test]
fn move_same_filesystem_uses_rename() {
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path().join("dst");
    fs::create_dir(&dst).unwrap();
    fs::write(tmp.path().join("a.txt"), "data").unwrap();
    let (summary, _) = run_job(
        &job(
            1,
            Op::Move,
            vec![tmp.path().join("a.txt")],
            Some(dst.clone()),
        ),
        overwrite(),
        Hooks::default(),
    );
    assert!(summary.failed.is_empty());
    assert_eq!(read(&dst.join("a.txt")), "data");
    assert!(!tmp.path().join("a.txt").exists(), "source removed");
}

#[test]
fn move_cross_device_copies_then_removes_source() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("sub/x.txt"), "data").unwrap();
    let hooks = Hooks {
        force_cross_device: true,
        ..Hooks::default()
    };
    let (summary, _) = run_job(
        &job(1, Op::Move, vec![src.clone()], Some(dst.clone())),
        overwrite(),
        hooks,
    );
    assert!(summary.failed.is_empty());
    assert_eq!(read(&dst.join("tree/sub/x.txt")), "data");
    assert!(!src.exists(), "source tree removed after full copy");
}

#[test]
fn cross_device_error_is_detected() {
    let unix_exdev = std::io::Error::from_raw_os_error(18);
    let win_notsame = std::io::Error::from_raw_os_error(17);
    if cfg!(unix) {
        assert!(is_cross_device(&unix_exdev));
    }
    if cfg!(windows) {
        assert!(is_cross_device(&win_notsame));
    }
    assert!(!is_cross_device(&std::io::Error::from_raw_os_error(2)));
}

// ---------- Delete (P3-9, P3-10) ----------

#[test]
fn delete_removes_files_and_trees() {
    let tmp = tempfile::tempdir().unwrap();
    let tree = tmp.path().join("tree");
    fs::create_dir_all(tree.join("sub")).unwrap();
    fs::write(tree.join("sub/x.txt"), "x").unwrap();
    let file = tmp.path().join("f.txt");
    fs::write(&file, "y").unwrap();
    let (summary, _) = run_job(
        &job(1, Op::Delete, vec![tree.clone(), file.clone()], None),
        overwrite(),
        Hooks::default(),
    );
    assert!(summary.failed.is_empty());
    assert!(!tree.exists() && !file.exists());
}

#[cfg(unix)]
#[test]
fn delete_removes_the_link_not_the_target() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.txt"), "keep").unwrap();
    let link = tmp.path().join("link");
    symlink(&target, &link).unwrap();
    run_job(
        &job(1, Op::Delete, vec![link.clone()], None),
        overwrite(),
        Hooks::default(),
    );
    assert!(!link.exists());
    assert!(target.join("keep.txt").exists(), "target kept");
}

// ---------- cancel (P3-11) ----------

#[test]
fn cancel_stops_a_delete_and_reports_it() {
    let tmp = tempfile::tempdir().unwrap();
    let mut sources = Vec::new();
    for i in 0..200 {
        let p = tmp.path().join(format!("f{i:03}"));
        fs::write(&p, "x").unwrap();
        sources.push(p);
    }
    let (event_tx, event_rx) = mpsc::channel::<JobEvent>();
    let (_answer_tx, answer_rx) = mpsc::channel::<Answer>();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = Arc::clone(&cancel);
    let j = job(1, Op::Delete, sources.clone(), None);
    let handle = thread::spawn(move || run(&j, &event_tx, &answer_rx, &cancel2, &Hooks::default()));
    // Cancel as soon as the first progress arrives.
    for event in &event_rx {
        if matches!(event, JobEvent::Progress { .. }) {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if matches!(event, JobEvent::Finished(_)) {
            break;
        }
    }
    let summary = handle.join().unwrap();
    assert!(summary.cancelled);
    let remaining = fs::read_dir(tmp.path()).unwrap().count();
    assert!(remaining > 0, "cancel left some files");
}

// ---------- directories left by an unfinished copy ----------

#[test]
fn failed_copy_leaves_no_empty_directory_it_created() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("sub/x.txt"), "data").unwrap();
    let hooks = Hooks {
        fail_after_bytes: Some(0),
        ..Hooks::default()
    };
    let (summary, _) = run_job(
        &job(1, Op::Copy, vec![src.clone()], Some(dst.clone())),
        overwrite(),
        hooks,
    );
    assert!(!summary.failed.is_empty());
    assert!(!dst.join("tree").exists(), "empty tree removed");
    assert!(src.join("sub/x.txt").exists());
}

#[test]
fn failed_copy_keeps_directories_that_have_content_or_existed() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("new")).unwrap();
    fs::create_dir_all(dst.join("tree/old")).unwrap();
    fs::write(src.join("new/x.txt"), "data").unwrap();
    // Merge into an existing tree: tree/ and tree/old/ were there before and must stay.
    let hooks = Hooks {
        fail_after_bytes: Some(0),
        ..Hooks::default()
    };
    let (summary, _) = run_job(
        &job(1, Op::Copy, vec![src.clone()], Some(dst.clone())),
        overwrite(),
        hooks,
    );
    assert!(!summary.failed.is_empty());
    assert!(dst.join("tree/old").is_dir(), "existing directories kept");
    assert!(
        !dst.join("tree/new").exists(),
        "created and empty -> removed"
    );
}

#[test]
fn cancelled_cross_device_move_keeps_the_source_and_no_empty_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    // Larger than 16 chunks, so progress arrives while the file is still being copied.
    let big = vec![7_u8; 20 * fds::ops::worker::CHUNK];
    fs::write(src.join("big.bin"), &big).unwrap();
    let (event_tx, event_rx) = mpsc::channel::<JobEvent>();
    let (_answer_tx, answer_rx) = mpsc::channel::<Answer>();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = Arc::clone(&cancel);
    let j = job(1, Op::Move, vec![src.clone()], Some(dst.clone()));
    let hooks = Hooks {
        force_cross_device: true,
        ..Hooks::default()
    };
    let handle = thread::spawn(move || run(&j, &event_tx, &answer_rx, &cancel2, &hooks));
    for event in &event_rx {
        if matches!(event, JobEvent::Progress { .. }) {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if matches!(event, JobEvent::Finished(_)) {
            break;
        }
    }
    let summary = handle.join().unwrap();
    assert!(summary.cancelled);
    assert_eq!(
        fs::read(src.join("big.bin")).unwrap().len(),
        big.len(),
        "source kept"
    );
    assert!(!dst.join("tree").exists(), "no empty directory left behind");
}

// ---------- Gemini §38 review findings ----------

#[test]
fn copy_works_when_hard_links_are_unavailable() {
    // FAT32 / exFAT (USB drives) cannot hard-link; the copy must still succeed.
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path().join("dst");
    fs::create_dir(&dst).unwrap();
    fs::write(tmp.path().join("a.txt"), "data").unwrap();
    let hooks = Hooks {
        no_hard_links: true,
        ..Hooks::default()
    };
    let (summary, _) = run_job(
        &job(
            1,
            Op::Copy,
            vec![tmp.path().join("a.txt")],
            Some(dst.clone()),
        ),
        overwrite(),
        hooks,
    );
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert_eq!(read(&dst.join("a.txt")), "data");
    let names: Vec<_> = fs::read_dir(&dst).unwrap().filter_map(Result::ok).collect();
    assert_eq!(names.len(), 1, "no temp file left");
}

#[test]
fn move_into_the_same_directory_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.txt"), "x").unwrap();
    let (summary, _) = run_job(
        &job(
            1,
            Op::Move,
            vec![tmp.path().join("a.txt")],
            Some(tmp.path().to_path_buf()),
        ),
        overwrite(),
        Hooks::default(),
    );
    assert!(summary.failed.is_empty());
    assert_eq!(summary.skipped, 1);
    assert_eq!(read(&tmp.path().join("a.txt")), "x", "original untouched");
    assert!(!tmp.path().join("a (2).txt").exists(), "no renamed copy");
}

// ---------- Trash (R-402-3, M1-a, C39) ----------

use std::sync::Mutex;

/// Paths passed to the injected trash functions (all tests share it; filter by temp dir).
static TRASHED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn record_trash(path: &Path) -> std::io::Result<()> {
    TRASHED.lock().unwrap().push(path.to_path_buf());
    Ok(())
}

fn record_but_fail_b(path: &Path) -> std::io::Result<()> {
    TRASHED.lock().unwrap().push(path.to_path_buf());
    if path.file_name().is_some_and(|n| n == "b.txt") {
        return Err(std::io::Error::other("cannot move to the trash: test"));
    }
    Ok(())
}

fn trashed_under(dir: &Path) -> Vec<PathBuf> {
    TRASHED
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.starts_with(dir))
        .cloned()
        .collect()
}

fn trash_hooks(f: fn(&Path) -> std::io::Result<()>) -> Hooks {
    Hooks {
        trash: Some(f),
        ..Hooks::default()
    }
}

#[test]
fn trash_passes_each_source_as_given_and_never_walks_or_removes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("tree");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/x.txt"), "x").unwrap();
    let file = tmp.path().join("a.txt");
    fs::write(&file, "a").unwrap();
    let sources = vec![file.clone(), dir.clone()];
    let (summary, events) = run_job(
        &job(1, Op::Trash, sources.clone(), None),
        overwrite(),
        trash_hooks(record_trash),
    );
    assert_eq!(
        trashed_under(tmp.path()),
        sources,
        "top-level items only, in order"
    );
    assert!(summary.failed.is_empty() && !summary.cancelled);
    assert!(summary.done >= 2, "{summary:?}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, JobEvent::Progress { .. }))
    );
    // The injected function moved nothing, so everything must still be there: the worker
    // itself never deletes for Trash.
    assert!(file.exists() && dir.join("sub/x.txt").exists());
}

#[test]
fn a_failure_is_reported_and_the_rest_continue() {
    let tmp = tempfile::tempdir().unwrap();
    let names = ["a.txt", "b.txt", "c.txt", "d.txt"];
    let sources: Vec<PathBuf> = names.iter().map(|n| tmp.path().join(n)).collect();
    for s in &sources {
        fs::write(s, "x").unwrap();
    }
    let (summary, events) = run_job(
        &job(1, Op::Trash, sources.clone(), None),
        overwrite(),
        trash_hooks(record_but_fail_b),
    );
    assert_eq!(
        trashed_under(tmp.path()),
        sources,
        "c and d are still tried after b fails"
    );
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].0, tmp.path().join("b.txt"));
    assert!(summary.failed[0].1.contains("cannot move to the trash"));
    assert!(events.iter().any(
        |e| matches!(e, JobEvent::EntryFailed { path, .. } if *path == tmp.path().join("b.txt"))
    ));
    assert!(!summary.cancelled);
}

#[cfg(unix)]
#[test]
fn a_link_is_trashed_as_the_link() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    fs::create_dir(&target).unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    run_job(
        &job(1, Op::Trash, vec![link.clone()], None),
        overwrite(),
        trash_hooks(record_trash),
    );
    assert_eq!(
        trashed_under(tmp.path()),
        vec![link],
        "not the resolved target"
    );
}

#[test]
fn cancel_stops_before_the_next_item() {
    let tmp = tempfile::tempdir().unwrap();
    let sources: Vec<PathBuf> = (0..50)
        .map(|i| tmp.path().join(format!("f{i:02}")))
        .collect();
    for s in &sources {
        fs::write(s, "x").unwrap();
    }
    let (event_tx, event_rx) = mpsc::channel::<JobEvent>();
    let (_answer_tx, answer_rx) = mpsc::channel::<Answer>();
    let cancel = Arc::new(AtomicBool::new(true));
    let j = job(1, Op::Trash, sources, None);
    let hooks = trash_hooks(record_trash);
    let handle = thread::spawn(move || run(&j, &event_tx, &answer_rx, &cancel, &hooks));
    for event in &event_rx {
        if matches!(event, JobEvent::Finished(_)) {
            break;
        }
    }
    let summary = handle.join().unwrap();
    assert!(summary.cancelled);
    assert!(
        trashed_under(tmp.path()).is_empty(),
        "cancelled before starting"
    );
}
