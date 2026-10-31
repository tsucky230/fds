//! Item details (info panel data). Pure formatters plus `read_info` against a temp folder.

use std::fs;
use std::time::{Duration, UNIX_EPOCH};

use fds::info::{
    DIR_COUNT_CAP, InfoKind, MarkSummary, format_mode, format_size, format_time, format_win_attrs,
    read_info, summarize,
};
use jiff::tz::{self, TimeZone};

#[test]
fn mode_shows_octal_and_symbolic() {
    assert_eq!(format_mode(0o755), "755 rwxr-xr-x");
    assert_eq!(format_mode(0o644), "644 rw-r--r--");
    assert_eq!(format_mode(0o000), "000 ---------");
    assert_eq!(format_mode(0o777), "777 rwxrwxrwx");
}

#[test]
fn mode_shows_special_bits() {
    assert_eq!(format_mode(0o4755), "4755 rwsr-xr-x");
    assert_eq!(format_mode(0o4644), "4644 rwSr--r--");
    assert_eq!(format_mode(0o2755), "2755 rwxr-sr-x");
    assert_eq!(format_mode(0o1777), "1777 rwxrwxrwt");
    assert_eq!(format_mode(0o1776), "1776 rwxrwxrwT");
}

#[test]
fn mode_ignores_file_type_bits() {
    assert_eq!(format_mode(0o100644), "644 rw-r--r--");
}

#[test]
fn size_uses_binary_units() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_size(1024), "1.0 KiB");
    assert_eq!(format_size(12_595), "12.2 KiB"); // 12.299 KiB, rounded down
    assert_eq!(format_size(12_600), "12.3 KiB");
    assert_eq!(format_size(1024 * 1024), "1.0 MiB");
    assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
    assert_eq!(format_size(3 * 1024 * 1024 * 1024 * 1024), "3.0 TiB");
    assert!(!format_size(u64::MAX).is_empty());
}

#[test]
fn windows_attributes_are_fixed_order_letters() {
    assert_eq!(format_win_attrs(0), "----");
    assert_eq!(format_win_attrs(0x1), "R---");
    assert_eq!(format_win_attrs(0x1 | 0x20), "R--A");
    assert_eq!(format_win_attrs(0x2 | 0x4), "-HS-");
    assert_eq!(format_win_attrs(0x1 | 0x2 | 0x4 | 0x20), "RHSA");
    // Other bits (directory 0x10, normal 0x80) are not shown.
    assert_eq!(format_win_attrs(0x10 | 0x80), "----");
}

#[test]
fn time_is_formatted_in_the_given_zone() {
    // 1_700_000_000 s = 2023-11-14 22:13:20 UTC
    let t = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(format_time(t, &TimeZone::UTC), "2023-11-14 22:13");
    let jst = TimeZone::fixed(tz::offset(9));
    assert_eq!(format_time(t, &jst), "2023-11-15 07:13");
}

#[test]
fn time_out_of_range_is_a_dash() {
    let before = UNIX_EPOCH - Duration::from_secs(10);
    assert_eq!(format_time(before, &TimeZone::UTC), "-");
    // About year 10,681: past jiff's last year (9999). checked_add: some platforms cannot hold it.
    if let Some(far) = UNIX_EPOCH.checked_add(Duration::from_secs(1 << 38)) {
        assert_eq!(format_time(far, &TimeZone::UTC), "-");
    }
}

#[test]
fn read_info_of_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, b"hello").unwrap();
    let info = read_info(&file).unwrap();
    assert_eq!(info.kind, InfoKind::File);
    assert_eq!(info.size, Some(5));
    assert!(info.modified.is_some());
    assert!(!info.readonly);
    assert_eq!(info.dir_items, None);
    assert_eq!(info.link_target, None);
    #[cfg(unix)]
    {
        assert!(info.unix_mode.is_some());
        assert!(info.owner.is_some());
        assert_eq!(info.win_attrs, None);
    }
    #[cfg(windows)]
    {
        assert!(info.win_attrs.is_some());
        assert_eq!(info.unix_mode, None);
    }
}

// Clearing the flag lets the temp folder be deleted on Windows; the file is private to the test.
#[allow(clippy::permissions_set_readonly_false)]
#[test]
fn read_info_reports_the_readonly_flag() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ro.txt");
    fs::write(&file, b"x").unwrap();
    let mut perm = fs::metadata(&file).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&file, perm.clone()).unwrap();
    assert!(read_info(&file).unwrap().readonly);
    perm.set_readonly(false);
    fs::set_permissions(&file, perm).unwrap();
}

#[test]
fn read_info_of_a_directory_counts_entries() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..3 {
        fs::write(dir.path().join(format!("f{i}")), b"").unwrap();
    }
    fs::create_dir(dir.path().join("sub")).unwrap();
    let info = read_info(dir.path()).unwrap();
    assert_eq!(info.kind, InfoKind::Dir);
    assert_eq!(info.size, None);
    assert_eq!(info.dir_items, Some((4, false)));
}

const _: () = assert!(DIR_COUNT_CAP >= 1000);

#[test]
fn read_info_of_a_missing_path_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_info(&dir.path().join("nope")).is_err());
}

#[cfg(unix)]
#[test]
fn read_info_of_symlinks() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("t.txt");
    fs::write(&target, b"abc").unwrap();
    let good = dir.path().join("good");
    symlink(&target, &good).unwrap();
    let info = read_info(&good).unwrap();
    assert_eq!(info.kind, InfoKind::Symlink);
    assert_eq!(info.link_target.as_deref(), Some(target.as_path()));
    assert!(!info.link_broken);

    let bad = dir.path().join("bad");
    symlink(dir.path().join("gone"), &bad).unwrap();
    let info = read_info(&bad).unwrap();
    assert_eq!(info.kind, InfoKind::Symlink);
    assert!(info.link_broken);
}

#[cfg(unix)]
#[test]
fn read_info_mode_matches_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.sh");
    fs::write(&file, b"#!/bin/sh").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o750)).unwrap();
    assert_eq!(read_info(&file).unwrap().unix_mode, Some(0o750));
}

#[test]
fn summarize_counts_files_dirs_and_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let d = dir.path().join("d");
    fs::write(&a, vec![0u8; 100]).unwrap();
    fs::write(&b, vec![0u8; 20]).unwrap();
    fs::create_dir(&d).unwrap();
    fs::write(d.join("inner"), vec![0u8; 999]).unwrap();
    let missing = dir.path().join("missing");
    let s = summarize(&[a, b, d, missing]);
    assert_eq!(
        s,
        MarkSummary {
            files: 2,
            dirs: 1,
            bytes: 120
        }
    );
    assert_eq!(summarize(&[]), MarkSummary::default());
}
