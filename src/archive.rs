//! Standard Core archive operations (R-407).
//!
//! ZIP, tar and gzip-compressed tar are handled without an external archiver or Pack. The
//! filesystem-facing functions are deliberately separate from `App`: the event loop runs them on
//! a worker thread and reports the result to the UI.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tempfile::Builder as TempBuilder;
use zip::write::SimpleFileOptions;

const MAX_MEMBERS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
}

/// A background archive operation requested by the pure App state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Create {
        destination: PathBuf,
        sources: Vec<PathBuf>,
    },
    Extract {
        source: PathBuf,
        destination: PathBuf,
    },
}

impl Action {
    pub fn destination(&self) -> &Path {
        match self {
            Self::Create { destination, .. } | Self::Extract { destination, .. } => destination,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// A safe path relative to the archive root. Never absolute or containing `..`.
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

/// Classify a filename by its suffix, case-insensitively. Unknown names return `None`.
pub fn format(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(Format::TarGz)
    } else if name.ends_with(".tar") {
        Some(Format::Tar)
    } else if name.ends_with(".zip") {
        Some(Format::Zip)
    } else {
        None
    }
}

/// List archive members for hierarchical browsing.
///
/// Returns normalized, relative file and directory paths (including implicit parent directories)
/// in a stable order. Rejects unsafe names, links, special files, malformed data and an excessive
/// entry count. Invalid archive content returns `InvalidData`; unsupported suffixes return
/// `InvalidInput`. Reads no file outside `source` and writes nothing.
pub fn list(source: &Path) -> io::Result<Vec<Member>> {
    let kind = format(source).ok_or_else(|| input_error("unsupported archive format"))?;
    let mut found = BTreeMap::new();
    match kind {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(File::open(source)?).map_err(invalid)?;
            for i in 0..zip.len() {
                let entry = zip.by_index(i).map_err(invalid)?;
                let is_dir = entry.is_dir();
                validate_zip_kind(entry.unix_mode(), is_dir)?;
                add_member(
                    &mut found,
                    safe_member_path(&entry.name().map_err(invalid)?)?,
                    is_dir,
                    entry.size(),
                )?
            }
        }
        Format::Tar => {
            let file = File::open(source)?;
            if file.metadata()?.len() < 1024 {
                return Err(invalid("short tar archive"));
            }
            list_tar(file, &mut found)?;
        }
        Format::TarGz => list_tar(GzDecoder::new(File::open(source)?), &mut found)?,
    }
    Ok(found.into_values().collect())
}

/// Create a ZIP, tar or tar.gz archive at `destination` from selected filesystem entries.
///
/// Each source must be an immediate child of the same directory, and its name is the archive
/// root. Directories are included recursively, including empty ones; links and special files are
/// rejected. `destination` must not exist or be inside a source. On failure or cancellation, it
/// leaves no final archive and removes its temporary output. Existing output returns
/// `AlreadyExists`; cancellation returns `Interrupted`.
pub fn create(destination: &Path, sources: &[PathBuf], cancel: &AtomicBool) -> io::Result<()> {
    let kind = format(destination).ok_or_else(|| input_error("unsupported archive format"))?;
    ensure_absent(destination)?;
    check_cancel(cancel)?;
    let members = source_members(destination, sources, cancel)?;
    let parent = destination
        .parent()
        .ok_or_else(|| input_error("missing destination parent"))?;
    let mut temp = TempBuilder::new()
        .prefix(".fds-archive-partial-")
        .tempfile_in(parent)?;
    match kind {
        Format::Zip => write_zip(temp.as_file_mut(), &members, cancel)?,
        Format::Tar => {
            write_tar(temp.as_file_mut(), &members, cancel)?;
        }
        Format::TarGz => {
            let gz = GzEncoder::new(temp.as_file_mut(), Compression::default());
            write_tar(gz, &members, cancel)?.finish()?;
        }
    }
    check_cancel(cancel)?;
    temp.as_file_mut().sync_all()?;
    temp.persist_noclobber(destination).map_err(|e| e.error)?;
    Ok(())
}

/// Extract every member into a new directory at `destination`.
///
/// The destination must not exist. Unsafe paths, links, special files, duplicate members and
/// name collisions are rejected before publishing any result. A failure or cancellation leaves
/// `destination` absent and removes temporary output. No existing entry is overwritten. Unsafe or
/// malformed input returns `InvalidData`, an existing destination returns `AlreadyExists`, and
/// cancellation returns `Interrupted`.
pub fn extract(source: &Path, destination: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let kind = format(source).ok_or_else(|| input_error("unsupported archive format"))?;
    ensure_absent(destination)?;
    check_cancel(cancel)?;
    list(source)?;
    let parent = destination
        .parent()
        .ok_or_else(|| input_error("missing destination parent"))?;
    let stage = TempBuilder::new()
        .prefix(".fds-extract-partial-")
        .tempdir_in(parent)?;
    match kind {
        Format::Zip => extract_zip(source, stage.path(), cancel)?,
        Format::Tar => extract_tar(File::open(source)?, stage.path(), cancel)?,
        Format::TarGz => extract_tar(GzDecoder::new(File::open(source)?), stage.path(), cancel)?,
    }
    check_cancel(cancel)?;
    ensure_absent(destination)?;
    fs::rename(stage.path(), destination)?;
    Ok(())
}

/// Suggest a sibling directory name for extraction, stripping one supported archive suffix.
/// Returns `None` when the source is not a supported archive or has no usable stem.
pub fn extract_destination(source: &Path) -> Option<PathBuf> {
    let name = source.file_name()?.to_str()?;
    let suffix = match format(source)? {
        Format::Zip | Format::Tar => 4,
        Format::TarGz if name.to_ascii_lowercase().ends_with(".tgz") => 4,
        Format::TarGz => 7,
    };
    let stem = &name[..name.len() - suffix];
    if stem.is_empty() || stem == "." || stem == ".." {
        return None;
    }
    Some(source.parent()?.join(stem))
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn input_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "archive operation cancelled",
        ))
    } else {
        Ok(())
    }
}

fn ensure_absent(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Accept portable relative names only. Backslashes and colons are rejected so an archive made
/// on one OS cannot reinterpret them as separators, drive prefixes or NTFS streams on another.
fn safe_member_path(raw: &str) -> io::Result<PathBuf> {
    if raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.contains('\\')
        || raw.contains(':')
        || raw.contains('\0')
    {
        return Err(invalid("unsafe archive path"));
    }
    let mut path = PathBuf::new();
    for (i, part) in raw.trim_end_matches('/').split('/').enumerate() {
        if part == ".." || part.is_empty() || (part == "." && i != 0) {
            return Err(invalid("unsafe archive path"));
        }
        if part != "." {
            crate::ops::name::validate(part, cfg!(windows))
                .map_err(|_| invalid("unsafe archive path"))?;
            path.push(part);
        }
    }
    if path.as_os_str().is_empty() {
        return Err(invalid("empty archive path"));
    }
    Ok(path)
}

fn add_member(
    found: &mut BTreeMap<PathBuf, Member>,
    path: PathBuf,
    is_dir: bool,
    size: u64,
) -> io::Result<()> {
    let mut parents = Vec::new();
    let mut parent = path.parent();
    while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
        parents.push(p.to_path_buf());
        parent = p.parent();
    }
    for p in parents.into_iter().rev() {
        match found.get(&p) {
            Some(existing) if !existing.is_dir => return Err(invalid("file used as directory")),
            Some(_) => {}
            None => {
                found.insert(
                    p.clone(),
                    Member {
                        path: p,
                        is_dir: true,
                        size: 0,
                    },
                );
            }
        }
    }
    if let Some(existing) = found.get(&path) {
        if existing.is_dir && is_dir && existing.size == 0 {
            return Ok(());
        }
        return Err(invalid("duplicate archive member"));
    }
    found.insert(path.clone(), Member { path, is_dir, size });
    if found.len() > MAX_MEMBERS {
        return Err(invalid("too many archive members"));
    }
    Ok(())
}

fn validate_zip_kind(mode: Option<u32>, is_dir: bool) -> io::Result<()> {
    if let Some(mode) = mode {
        let file_type = mode & 0o170000;
        if file_type != 0 && file_type != if is_dir { 0o040000 } else { 0o100000 } {
            return Err(invalid("archive link or special file is unsupported"));
        }
    }
    Ok(())
}

fn list_tar<R: Read>(reader: R, found: &mut BTreeMap<PathBuf, Member>) -> io::Result<()> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(invalid)? {
        let entry = entry.map_err(invalid)?;
        let ty = entry.header().entry_type();
        if !ty.is_file() && !ty.is_dir() {
            return Err(invalid("archive link or special file is unsupported"));
        }
        let name = entry.path().map_err(invalid)?;
        let name = name
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 archive name"))?;
        add_member(found, safe_member_path(name)?, ty.is_dir(), entry.size())?;
    }
    Ok(())
}

struct SourceMember {
    source: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
}

fn source_members(
    destination: &Path,
    sources: &[PathBuf],
    cancel: &AtomicBool,
) -> io::Result<Vec<SourceMember>> {
    let first = sources
        .first()
        .ok_or_else(|| input_error("no archive sources"))?;
    let base = first
        .parent()
        .ok_or_else(|| input_error("source has no parent"))?;
    let mut found = Vec::new();
    for source in sources {
        check_cancel(cancel)?;
        if source.parent() != Some(base) {
            return Err(input_error("sources must share one directory"));
        }
        if source.is_dir() && destination.starts_with(source) {
            return Err(input_error("archive cannot be inside a source"));
        }
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| input_error("non-UTF-8 source name"))?;
        safe_member_path(name).map_err(|e| input_error(&e.to_string()))?;
        collect_source(source, name.to_string(), cancel, &mut found, 0)?;
    }
    Ok(found)
}

fn collect_source(
    source: &Path,
    name: String,
    cancel: &AtomicBool,
    found: &mut Vec<SourceMember>,
    depth: usize,
) -> io::Result<()> {
    check_cancel(cancel)?;
    if depth > 128 || found.len() >= MAX_MEMBERS {
        return Err(input_error("archive source tree too large"));
    }
    let metadata = fs::symlink_metadata(source)?;
    let is_dir = metadata.is_dir();
    if !is_dir && !metadata.is_file() {
        return Err(input_error(
            "links and special files are not supported in basic archives",
        ));
    }
    found.push(SourceMember {
        source: source.to_path_buf(),
        name: name.clone(),
        is_dir,
        size: metadata.len(),
    });
    if is_dir {
        let mut children = fs::read_dir(source)?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child
                .file_name()
                .into_string()
                .map_err(|_| input_error("non-UTF-8 source name"))?;
            safe_member_path(&child_name).map_err(|e| input_error(&e.to_string()))?;
            collect_source(
                &child.path(),
                format!("{name}/{child_name}"),
                cancel,
                found,
                depth + 1,
            )?;
        }
    }
    Ok(())
}

struct CancelReader<'a, R> {
    inner: R,
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for CancelReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        check_cancel(self.cancel)?;
        self.inner.read(buf)
    }
}

fn copy_cancel<R: Read, W: Write>(
    reader: R,
    writer: &mut W,
    cancel: &AtomicBool,
) -> io::Result<()> {
    io::copy(
        &mut CancelReader {
            inner: reader,
            cancel,
        },
        writer,
    )?;
    Ok(())
}

fn write_zip(file: &mut File, members: &[SourceMember], cancel: &AtomicBool) -> io::Result<()> {
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for member in members {
        check_cancel(cancel)?;
        if member.is_dir {
            zip.add_directory(format!("{}/", member.name), options)
                .map_err(invalid)?;
        } else {
            zip.start_file(&member.name, options).map_err(invalid)?;
            copy_cancel(File::open(&member.source)?, &mut zip, cancel)?;
        }
    }
    zip.finish().map_err(invalid)?;
    Ok(())
}

fn write_tar<W: Write>(writer: W, members: &[SourceMember], cancel: &AtomicBool) -> io::Result<W> {
    let mut tar = tar::Builder::new(writer);
    for member in members {
        check_cancel(cancel)?;
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(if member.is_dir {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_mode(if member.is_dir { 0o755 } else { 0o644 });
        header.set_size(if member.is_dir { 0 } else { member.size });
        header.set_cksum();
        if member.is_dir {
            tar.append_data(&mut header, &member.name, io::empty())?;
        } else {
            let reader = CancelReader {
                inner: File::open(&member.source)?,
                cancel,
            };
            tar.append_data(&mut header, &member.name, reader)?;
        }
    }
    tar.finish()?;
    tar.into_inner()
}

fn extract_zip(source: &Path, stage: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(source)?).map_err(invalid)?;
    for i in 0..zip.len() {
        check_cancel(cancel)?;
        let mut entry = zip.by_index(i).map_err(invalid)?;
        let is_dir = entry.is_dir();
        validate_zip_kind(entry.unix_mode(), is_dir)?;
        let rel = safe_member_path(&entry.name().map_err(invalid)?)?
        write_member(&mut entry, &stage.join(rel), is_dir, cancel)?;
    }
    Ok(())
}

fn extract_tar<R: Read>(reader: R, stage: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let mut tar = tar::Archive::new(reader);
    for entry in tar.entries().map_err(invalid)? {
        check_cancel(cancel)?;
        let mut entry = entry.map_err(invalid)?;
        let ty = entry.header().entry_type();
        if !ty.is_file() && !ty.is_dir() {
            return Err(invalid("archive link or special file is unsupported"));
        }
        let name = entry.path().map_err(invalid)?;
        let name = name
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 archive name"))?;
        let rel = safe_member_path(name)?;
        write_member(&mut entry, &stage.join(rel), ty.is_dir(), cancel)?;
    }
    Ok(())
}

fn write_member<R: Read>(
    reader: &mut R,
    target: &Path,
    is_dir: bool,
    cancel: &AtomicBool,
) -> io::Result<()> {
    if is_dir {
        fs::create_dir_all(target)?;
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| invalid("archive entry has no parent"))?;
        fs::create_dir_all(parent)?;
        let mut file = File::options().write(true).create_new(true).open(target)?;
        copy_cancel(reader, &mut file, cancel)?;
    }
    Ok(())
}
