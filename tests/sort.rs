//! Sorting (AC 32 / R-302).

use std::cmp::Ordering;
use std::path::PathBuf;

use fds::entry::{EntryKind, FileEntry, LinkTarget};
use fds::sort::{natural_cmp, sort_entries};

fn entry(name: &str, kind: EntryKind) -> FileEntry {
    FileEntry::new(PathBuf::from("/d").join(name), name.into(), kind, false)
}

fn names(entries: &[FileEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.display_name.as_str()).collect()
}

#[test]
fn natural_order_compares_digit_runs_numerically() {
    assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
    assert_eq!(natural_cmp("file10", "file2"), Ordering::Greater);
    assert_eq!(natural_cmp("a9b", "a10a"), Ordering::Less);
}

#[test]
fn natural_order_ignores_case_first() {
    assert_eq!(natural_cmp("apple", "Banana"), Ordering::Less);
    assert_eq!(natural_cmp("Banana", "apple"), Ordering::Greater);
}

#[test]
fn natural_order_is_total_with_byte_tiebreak() {
    // Equal ignoring case -> original bytes decide ('R' < 'r').
    assert_eq!(natural_cmp("README", "readme"), Ordering::Less);
    // Equal numerically -> original bytes decide ('0' < '1').
    assert_eq!(natural_cmp("01", "1"), Ordering::Less);
    assert_eq!(natural_cmp("same", "same"), Ordering::Equal);
}

#[test]
fn natural_order_handles_huge_numbers_and_edges() {
    assert_eq!(
        natural_cmp("x99999999999999999999999", "x100000000000000000000000"),
        Ordering::Less
    );
    assert_eq!(natural_cmp("", "a"), Ordering::Less);
    assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
    // Digit chunk sorts before a non-digit chunk.
    assert_eq!(natural_cmp("1a", "a1"), Ordering::Less);
}

#[test]
fn natural_order_handles_non_ascii() {
    assert_eq!(natural_cmp("Äpfel", "äpfel"), Ordering::Less);
    assert_eq!(natural_cmp("日本2", "日本10"), Ordering::Less);
}

#[test]
fn groups_parent_then_dirs_then_files() {
    let mut v = vec![
        entry("b.txt", EntryKind::File),
        entry("zdir", EntryKind::Directory),
        entry("link_dir", EntryKind::Symlink(LinkTarget::Directory)),
        entry("broken", EntryKind::Symlink(LinkTarget::Broken)),
        entry("adir", EntryKind::Directory),
        entry("a.txt", EntryKind::File),
        FileEntry::parent_link(PathBuf::from("/")),
        entry("fifo", EntryKind::Other),
    ];
    sort_entries(&mut v);
    assert_eq!(
        names(&v),
        [
            "..", "adir", "link_dir", "zdir", "a.txt", "b.txt", "broken", "fifo"
        ]
    );
}

#[test]
fn sorts_within_group_by_natural_order() {
    let mut v = vec![
        entry("File10", EntryKind::File),
        entry("file2", EntryKind::File),
        entry("file1", EntryKind::File),
    ];
    sort_entries(&mut v);
    assert_eq!(names(&v), ["file1", "file2", "File10"]);
}

#[test]
fn marks_and_hidden_do_not_change_order() {
    let mut a = entry("b", EntryKind::File);
    a.marked = true;
    let mut c = entry("a", EntryKind::File);
    c.hidden = true;
    let mut v = vec![a, c];
    sort_entries(&mut v);
    assert_eq!(names(&v), ["a", "b"]);
}

#[test]
fn sorting_empty_slice_does_not_panic() {
    let mut v: Vec<FileEntry> = Vec::new();
    sort_entries(&mut v);
    assert!(v.is_empty());
}
