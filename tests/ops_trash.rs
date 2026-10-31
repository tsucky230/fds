//! The real platform trash (R-402-3, M1-a).
//!
//! These tests put files into the real trash of the machine that runs them, so they are
//! `#[ignore]`d: run them on purpose with `cargo test --test ops_trash -- --ignored`.
//! The always-on checks of the Trash operation (injected trash function) are in ops_worker.rs.

use std::fs;

use fds::ops::trash::move_to_trash;

fn unique(name: &str) -> String {
    format!("fds-trash-test-{}-{name}", std::process::id())
}

#[test]
#[ignore = "moves files into the real trash"]
fn files_and_directories_go_to_the_trash() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(unique("file.txt"));
    fs::write(&file, "x").unwrap();
    let dir = tmp.path().join(unique("dir"));
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/y.txt"), "y").unwrap();
    move_to_trash(&file).unwrap();
    move_to_trash(&dir).unwrap();
    assert!(!file.exists());
    assert!(!dir.exists());
}

#[test]
#[ignore = "moves files into the real trash"]
fn a_link_goes_to_the_trash_and_its_target_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join(unique("target"));
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.txt"), "keep").unwrap();
    let link = tmp.path().join(unique("link"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    {
        // A junction needs no privilege, unlike a symlink.
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap()
            .status;
        assert!(status.success());
    }
    move_to_trash(&link).unwrap();
    assert!(fs::symlink_metadata(&link).is_err(), "link moved");
    assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
}

#[test]
fn a_missing_item_is_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join(unique("missing"));
    let error = move_to_trash(&missing).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

/// Windows: on a drive without a Recycle Bin (a subst drive, a USB stick, a network share) the
/// shell would delete permanently. Set `FDS_TEST_NO_RECYCLE_DIR` to a folder on such a drive.
#[test]
#[ignore = "needs FDS_TEST_NO_RECYCLE_DIR on a drive without a Recycle Bin"]
fn an_item_that_cannot_be_recycled_stays() {
    let dir = std::env::var_os("FDS_TEST_NO_RECYCLE_DIR")
        .expect("set FDS_TEST_NO_RECYCLE_DIR to a folder on a drive without a Recycle Bin");
    let file = std::path::Path::new(&dir).join(unique("stay.txt"));
    fs::write(&file, "stay").unwrap();
    let result = move_to_trash(&file);
    let still_there = fs::read_to_string(&file).ok();
    let _ = fs::remove_file(&file);
    assert!(result.is_err(), "must fail instead of deleting permanently");
    assert_eq!(still_there.as_deref(), Some("stay"), "the file must stay");
}
