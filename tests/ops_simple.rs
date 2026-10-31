//! Rename and mkdir on real files (P3-12).

use std::fs;

use fds::ops::name::NameError;
use fds::ops::simple::{SimpleError, make_dir, rename};

#[test]
fn rename_moves_the_entry() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("old.txt"), "x").unwrap();
    let new = rename(tmp.path(), "old.txt", "new.txt").unwrap();
    assert_eq!(new, tmp.path().join("new.txt"));
    assert!(!tmp.path().join("old.txt").exists());
    assert!(tmp.path().join("new.txt").exists());
}

#[test]
fn rename_rejects_invalid_names_and_existing_targets() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.txt"), "a").unwrap();
    fs::write(tmp.path().join("b.txt"), "b").unwrap();
    assert_eq!(
        rename(tmp.path(), "a.txt", ""),
        Err(SimpleError::InvalidName(NameError::Empty))
    );
    assert_eq!(
        rename(tmp.path(), "a.txt", "x/y"),
        Err(SimpleError::InvalidName(NameError::Separator))
    );
    assert_eq!(
        rename(tmp.path(), "a.txt", "b.txt"),
        Err(SimpleError::Exists(tmp.path().join("b.txt")))
    );
    assert_eq!(read(tmp.path(), "a.txt"), "a", "nothing changed on error");
    assert_eq!(read(tmp.path(), "b.txt"), "b");
}

#[test]
fn rename_to_same_name_is_a_noop() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.txt"), "x").unwrap();
    assert_eq!(
        rename(tmp.path(), "a.txt", "a.txt").unwrap(),
        tmp.path().join("a.txt")
    );
    assert_eq!(read(tmp.path(), "a.txt"), "x");
}

#[test]
fn case_only_rename_works_but_a_real_clash_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("File.txt"), "x").unwrap();
    // Case change of the same entry: allowed on any filesystem.
    let new = rename(tmp.path(), "File.txt", "FILE.TXT").unwrap();
    assert_eq!(new, tmp.path().join("FILE.TXT"));
    let names: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["FILE.TXT"]);

    // A different entry that only differs in case (needs a case-sensitive FS).
    if fs::write(tmp.path().join("other.txt"), "o").is_ok()
        && fs::write(tmp.path().join("OTHER.txt"), "O").is_ok()
    {
        let listed = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .count();
        if listed == 3 {
            // case-sensitive FS: renaming other.txt -> OTHER.txt must not clobber the other file.
            assert!(matches!(
                rename(tmp.path(), "other.txt", "OTHER.txt"),
                Err(SimpleError::Exists(_))
            ));
            assert_eq!(read(tmp.path(), "OTHER.txt"), "O");
        }
    }
}

#[test]
fn mkdir_creates_and_rejects_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    let made = make_dir(tmp.path(), "new").unwrap();
    assert!(made.is_dir());
    assert!(matches!(
        make_dir(tmp.path(), "new"),
        Err(SimpleError::Exists(_))
    ));
    assert_eq!(
        make_dir(tmp.path(), ".."),
        Err(SimpleError::InvalidName(NameError::Dot))
    );
}

fn read(dir: &std::path::Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}
