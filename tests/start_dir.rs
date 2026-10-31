//! Start directory resolution (AC 35 / R-111-1).

use std::ffi::OsString;
use std::fs;

use fds::run::resolve_start_dir;

#[test]
fn no_argument_uses_current_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = resolve_start_dir(None, tmp.path()).unwrap();
    assert_eq!(dir, tmp.path());
}

#[test]
fn relative_argument_becomes_absolute() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    let dir = resolve_start_dir(Some(OsString::from("sub")), tmp.path()).unwrap();
    assert!(dir.is_absolute());
    assert_eq!(dir, tmp.path().join("sub"));
}

#[test]
fn absolute_argument_is_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let dir = resolve_start_dir(Some(other.path().as_os_str().to_owned()), tmp.path()).unwrap();
    assert_eq!(dir, other.path());
}

#[test]
fn missing_path_or_file_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), b"x").unwrap();
    assert!(resolve_start_dir(Some(OsString::from("nope")), tmp.path()).is_err());
    assert!(resolve_start_dir(Some(OsString::from("file.txt")), tmp.path()).is_err());
}
