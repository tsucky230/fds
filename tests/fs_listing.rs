//! Directory reading against a real temp directory (AC 15, 16, 33, 36 / R-101, R-301).

use std::fs;

use fds::entry::EntryKind;
use fds::fs::read_listing;

fn find<'a>(list: &'a [fds::entry::FileEntry], name: &str) -> &'a fds::entry::FileEntry {
    list.iter()
        .find(|e| e.display_name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn reads_files_dirs_and_hidden_flags() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    fs::create_dir(d.join("sub")).unwrap();
    fs::write(d.join("a.txt"), b"x").unwrap();
    fs::write(d.join(".hidden"), b"x").unwrap();
    fs::write(d.join("日本語 ファイル.md"), b"x").unwrap();

    let list = read_listing(d).unwrap();
    assert_eq!(list.len(), 4, "no [..] from fs");
    assert_eq!(find(&list, "sub").kind, EntryKind::Directory);
    assert_eq!(find(&list, "a.txt").kind, EntryKind::File);
    assert!(find(&list, ".hidden").hidden);
    assert!(!find(&list, "a.txt").hidden);
    let jp = find(&list, "日本語 ファイル.md");
    assert_eq!(jp.path, d.join("日本語 ファイル.md"));
    assert!(list.iter().all(|e| !e.marked));
}

#[test]
fn empty_directory_is_ok() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(read_listing(tmp.path()).unwrap().is_empty());
}

#[test]
fn missing_directory_is_an_error_not_a_panic() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(read_listing(&tmp.path().join("nope")).is_err());
}

#[cfg(windows)]
#[test]
fn windows_hidden_attribute_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("secret.txt");
    fs::write(&p, b"x").unwrap();
    let status = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&p)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(find(&read_listing(tmp.path()).unwrap(), "secret.txt").hidden);
}

#[cfg(unix)]
#[test]
fn symlinks_are_classified_by_target() {
    use fds::entry::LinkTarget;
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    fs::create_dir(d.join("dir")).unwrap();
    fs::write(d.join("file"), b"x").unwrap();
    symlink(d.join("dir"), d.join("to_dir")).unwrap();
    symlink(d.join("file"), d.join("to_file")).unwrap();
    symlink(d.join("nowhere"), d.join("dangling")).unwrap();

    let list = read_listing(d).unwrap();
    assert_eq!(
        find(&list, "to_dir").kind,
        EntryKind::Symlink(LinkTarget::Directory)
    );
    assert_eq!(
        find(&list, "to_file").kind,
        EntryKind::Symlink(LinkTarget::File)
    );
    assert_eq!(
        find(&list, "dangling").kind,
        EntryKind::Symlink(LinkTarget::Broken)
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_names_keep_original_path() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let tmp = tempfile::tempdir().unwrap();
    let raw = OsStr::from_bytes(b"bad\xffname");
    if fs::write(tmp.path().join(raw), b"x").is_err() {
        return; // filesystem refuses non-UTF-8 names (e.g. some macOS volumes)
    }
    let list = read_listing(tmp.path()).unwrap();
    let e = &list[0];
    assert_eq!(e.name, raw);
    assert_eq!(e.path, tmp.path().join(raw));
    assert!(e.display_name.contains('\u{FFFD}'));
    assert!(e.path.is_file(), "original path still opens the file");
}

#[cfg(unix)]
#[test]
fn unreadable_directory_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let result = read_listing(&locked);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    // root ignores permissions; the assertion only makes sense for normal users.
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    if String::from_utf8_lossy(&uid).trim() == "0" {
        return;
    }
    assert!(result.is_err());
}
