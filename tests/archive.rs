//! R-407: Core archive round trips and safety at the public API boundary.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use fds::archive::{self, Format};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

fn populate(dir: &Path) -> PathBuf {
    let tree = dir.join("日本語 folder");
    fs::create_dir(&tree).unwrap();
    fs::create_dir(tree.join("empty")).unwrap();
    fs::create_dir(tree.join("nested")).unwrap();
    fs::write(tree.join("nested/a b.txt"), b"hello archive").unwrap();
    tree
}

#[test]
fn formats_and_extraction_names_are_predictable() {
    let dir = Path::new("/work");
    for (name, expected) in [
        ("x.zip", Format::Zip),
        ("x.TAR", Format::Tar),
        ("x.tar.gz", Format::TarGz),
        ("x.TGZ", Format::TarGz),
    ] {
        assert_eq!(archive::format(Path::new(name)), Some(expected));
        assert_eq!(
            archive::extract_destination(&dir.join(name)),
            Some(dir.join("x"))
        );
    }
    assert_eq!(archive::format(Path::new("x.7z")), None);
    assert_eq!(archive::extract_destination(Path::new(".zip")), None);
}

#[test]
fn each_format_round_trips_nested_files_and_empty_directories() {
    for suffix in ["zip", "tar", "tar.gz"] {
        let tmp = tempdir().unwrap();
        let tree = populate(tmp.path());
        let archive_path = tmp.path().join(format!("bundle.{suffix}"));
        archive::create(&archive_path, &[tree], &AtomicBool::new(false)).unwrap();
        let members = archive::list(&archive_path).unwrap();
        assert!(
            members
                .iter()
                .any(|m| m.path == Path::new("日本語 folder/nested/a b.txt") && !m.is_dir)
        );
        assert!(
            members
                .iter()
                .any(|m| m.path == Path::new("日本語 folder/empty") && m.is_dir)
        );
        let out = tmp.path().join("out");
        archive::extract(&archive_path, &out, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            fs::read(out.join("日本語 folder/nested/a b.txt")).unwrap(),
            b"hello archive"
        );
        assert!(out.join("日本語 folder/empty").is_dir());
    }
}

#[test]
fn creation_never_replaces_an_existing_archive_or_leaves_a_cancelled_result() {
    let tmp = tempdir().unwrap();
    let tree = populate(tmp.path());
    let archive_path = tmp.path().join("bundle.zip");
    fs::write(&archive_path, b"keep").unwrap();
    assert_eq!(
        archive::create(
            &archive_path,
            std::slice::from_ref(&tree),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&archive_path).unwrap(), b"keep");
    fs::remove_file(&archive_path).unwrap();
    let cancel = AtomicBool::new(true);
    assert_eq!(
        archive::create(&archive_path, &[tree], &cancel)
            .unwrap_err()
            .kind(),
        ErrorKind::Interrupted
    );
    assert!(!archive_path.exists());
}

#[test]
fn extraction_rejects_escape_and_keeps_destination_absent() {
    let tmp = tempdir().unwrap();
    let archive_path = tmp.path().join("bad.zip");
    let mut zip = zip::ZipWriter::new(File::create(&archive_path).unwrap());
    zip.start_file("../escape.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"bad").unwrap();
    zip.finish().unwrap();
    let out = tmp.path().join("out");
    assert_eq!(
        archive::list(&archive_path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        archive::extract(&archive_path, &out, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert!(!out.exists());
    assert!(!tmp.path().join("escape.txt").exists());
}

#[test]
fn extraction_rejects_links_and_existing_destinations() {
    let tmp = tempdir().unwrap();
    let archive_path = tmp.path().join("link.tar");
    let mut tar = tar::Builder::new(File::create(&archive_path).unwrap());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_cksum();
    tar.append_link(&mut header, "link", "../outside").unwrap();
    tar.finish().unwrap();
    let out = tmp.path().join("out");
    assert_eq!(
        archive::list(&archive_path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        archive::extract(&archive_path, &out, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert!(!out.exists());

    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), "keep").unwrap();
    assert_eq!(
        archive::extract(&archive_path, &out, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(out.join("keep")).unwrap(), b"keep");
}

#[test]
fn malformed_archives_fail_without_writing() {
    let tmp = tempdir().unwrap();
    for suffix in ["zip", "tar", "tar.gz"] {
        let file = tmp.path().join(format!("bad.{suffix}"));
        fs::write(&file, b"broken").unwrap();
        let out = tmp.path().join(format!("out.{suffix}"));
        assert_eq!(
            archive::list(&file).unwrap_err().kind(),
            ErrorKind::InvalidData,
            "{suffix}"
        );
        assert_eq!(
            archive::extract(&file, &out, &AtomicBool::new(false))
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData,
            "{suffix}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn extraction_discards_staging_directory_after_a_late_crc_error() {
    let tmp = tempdir().unwrap();
    let source = tmp.path().join("crc.zip");
    let mut zip = zip::ZipWriter::new(File::create(&source).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("first.txt", options).unwrap();
    zip.write_all(b"first").unwrap();
    zip.start_file("last.txt", options).unwrap();
    zip.write_all(b"unique payload for CRC test").unwrap();
    zip.finish().unwrap();
    let mut bytes = fs::read(&source).unwrap();
    let marker = b"unique payload for CRC test";
    let offset = bytes
        .windows(marker.len())
        .position(|window| window == marker)
        .unwrap();
    bytes[offset] ^= 1;
    fs::write(&source, bytes).unwrap();
    assert_eq!(archive::list(&source).unwrap().len(), 2);

    let destination = tmp.path().join("out");
    assert!(archive::extract(&source, &destination, &AtomicBool::new(false)).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn extraction_rejects_windows_device_names_before_writing() {
    let tmp = tempdir().unwrap();
    let source = tmp.path().join("device.zip");
    let mut zip = zip::ZipWriter::new(File::create(&source).unwrap());
    zip.start_file("folder/CON.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"bad").unwrap();
    zip.finish().unwrap();
    assert_eq!(
        archive::list(&source).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let destination = tmp.path().join("out");
    assert_eq!(
        archive::extract(&source, &destination, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert!(!destination.exists());
}
