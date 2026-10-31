//! Details of the item under the cursor (permissions, attributes, size, dates, link target), shown
//! in the info panel so the user can judge a command before issuing it. Filesystem reads are
//! done off the UI thread (`run.rs`); everything else here is pure and unit-tested.
//! EXIF / preview are out of scope for this module (planned for 0.3.1).

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Directory entries are counted up to this many; more is shown as "N+".
pub const DIR_COUNT_CAP: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoKind {
    File,
    Dir,
    Symlink,
    Other,
}

/// Details of one item. Fields the platform does not provide are `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub path: PathBuf,
    pub kind: InfoKind,
    /// Size in bytes (files and symlinks; `None` for directories).
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    /// Unix permission bits (`st_mode & 0o7777`). Unix only.
    pub unix_mode: Option<u32>,
    /// Unix (uid, gid). Unix only; shown as numbers (no name lookup, no extra dependency).
    pub owner: Option<(u32, u32)>,
    /// Windows file attribute bits. Windows only.
    pub win_attrs: Option<u32>,
    /// The read-only flag as `std` reports it (works on every platform).
    pub readonly: bool,
    /// For a symlink: where it points (as stored), and whether the target is missing.
    pub link_target: Option<PathBuf>,
    pub link_broken: bool,
    /// For a directory: number of entries, and whether counting stopped at `DIR_COUNT_CAP`.
    pub dir_items: Option<(usize, bool)>,
}

/// Summary of the marked items (top level only; directories are not walked).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkSummary {
    pub files: usize,
    pub dirs: usize,
    /// Sum of the sizes of the marked files (directories add nothing).
    pub bytes: u64,
}

/// Reads the details of `path` without following a final symlink.
///
/// Behavior:
/// - Uses `symlink_metadata`. Kind is Symlink / Dir / File / Other accordingly.
/// - Symlink: `link_target` = `read_link`; `link_broken` = the target does not exist.
/// - Dir: `dir_items` = entries counted with `read_dir`, capped at `DIR_COUNT_CAP`
///   (`(cap, true)` when more). An unreadable dir gives `None`.
/// - Unix: `unix_mode` and `owner` are set. Windows: `win_attrs` is set.
/// - An error (missing file, no permission) is returned; the caller shows nothing then.
pub fn read_info(path: &Path) -> io::Result<Info> {
    let metadata = std::fs::symlink_metadata(path)?;
    let kind = if metadata.is_symlink() {
        InfoKind::Symlink
    } else if metadata.is_dir() {
        InfoKind::Dir
    } else if metadata.is_file() {
        InfoKind::File
    } else {
        InfoKind::Other
    };

    #[cfg(unix)]
    let (unix_mode, owner) = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        (
            Some(metadata.permissions().mode() & 0o7777),
            Some((metadata.uid(), metadata.gid())),
        )
    };
    #[cfg(not(unix))]
    let (unix_mode, owner) = (None, None);

    #[cfg(windows)]
    let win_attrs = {
        use std::os::windows::fs::MetadataExt;
        Some(metadata.file_attributes())
    };
    #[cfg(not(windows))]
    let win_attrs = None;

    let dir_items = if kind == InfoKind::Dir {
        std::fs::read_dir(path).ok().map(|entries| {
            let mut count = 0;
            for entry in entries {
                if entry.is_ok() {
                    if count == DIR_COUNT_CAP {
                        return (count, true);
                    }
                    count += 1;
                }
            }
            (count, false)
        })
    } else {
        None
    };
    let is_symlink = kind == InfoKind::Symlink;

    Ok(Info {
        path: path.to_path_buf(),
        kind,
        size: match kind {
            InfoKind::File | InfoKind::Symlink => Some(metadata.len()),
            InfoKind::Dir | InfoKind::Other => None,
        },
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        unix_mode,
        owner,
        win_attrs,
        readonly: metadata.permissions().readonly(),
        link_target: if is_symlink {
            std::fs::read_link(path).ok()
        } else {
            None
        },
        link_broken: is_symlink && std::fs::metadata(path).is_err(),
        dir_items,
    })
}

/// Summarizes marked paths. Unreadable paths are skipped; never panics.
pub fn summarize(paths: &[PathBuf]) -> MarkSummary {
    let mut summary = MarkSummary::default();
    for path in paths {
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            continue;
        };
        if metadata.is_dir() {
            summary.dirs = summary.dirs.saturating_add(1);
        } else if metadata.is_file() {
            summary.files = summary.files.saturating_add(1);
            summary.bytes = summary.bytes.saturating_add(metadata.len());
        }
    }
    summary
}

/// Permission bits as octal plus symbolic, e.g. `755 rwxr-xr-x`.
///
/// Behavior:
/// - Octal has 3 digits, or 4 when setuid/setgid/sticky is set (e.g. `4755`).
/// - Symbolic: 9 chars `rwxrwxrwx` with `-` for missing bits; setuid shows `s` (or `S` without
///   x) in the owner slot, setgid the same in the group slot, sticky `t` / `T` in the other slot.
/// - Bits above 0o7777 are ignored.
pub fn format_mode(mode: u32) -> String {
    let mode = mode & 0o7777;
    let mut symbolic = String::with_capacity(9);
    for (shift, special, lower, upper) in [
        (6, 0o4000, 's', 'S'),
        (3, 0o2000, 's', 'S'),
        (0, 0o1000, 't', 'T'),
    ] {
        let bits = (mode >> shift) & 0o7;
        symbolic.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        symbolic.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        symbolic.push(match (bits & 0o1 != 0, mode & special != 0) {
            (true, true) => lower,
            (false, true) => upper,
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    format!("{mode:03o} {symbolic}")
}

/// Human-readable size with binary units: `0 B`, `1023 B`, `1.0 KiB`, `12.3 KiB`, `1.0 MiB`,
/// `GiB`, `TiB`. Bytes are shown as integers; larger units with one decimal (rounded down).
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut divisor = 1024_u64;
    let mut unit = "KiB";
    for next in ["MiB", "GiB", "TiB"] {
        if bytes < divisor * 1024 {
            break;
        }
        divisor *= 1024;
        unit = next;
    }
    let whole = bytes / divisor;
    let decimal = (bytes % divisor) * 10 / divisor;
    format!("{whole}.{decimal} {unit}")
}

/// Windows attributes as four letters in the fixed order `R H S A`; absent flags are `-`.
/// Example: read-only + archive -> `R--A`. Bits: R = 0x1, H = 0x2, S = 0x4, A = 0x20.
pub fn format_win_attrs(attrs: u32) -> String {
    [(0x1, 'R'), (0x2, 'H'), (0x4, 'S'), (0x20, 'A')]
        .into_iter()
        .map(|(bit, letter)| if attrs & bit != 0 { letter } else { '-' })
        .collect()
}

/// `YYYY-MM-DD HH:MM` in the given time zone. Times before 1970 or out of range give `-`.
pub fn format_time(time: SystemTime, tz: &jiff::tz::TimeZone) -> String {
    format_with(time, tz, "%Y-%m-%d %H:%M")
}

/// `YYYY/MM/DD HH:MM` for the title band (R-200-12). Same fallback as `format_time`.
pub fn format_clock(time: SystemTime, tz: &jiff::tz::TimeZone) -> String {
    format_with(time, tz, "%Y/%m/%d %H:%M")
}

fn format_with(time: SystemTime, tz: &jiff::tz::TimeZone, format: &str) -> String {
    use std::fmt::Write;

    if time.duration_since(SystemTime::UNIX_EPOCH).is_err() {
        return "-".to_owned();
    }
    let Ok(timestamp) = jiff::Timestamp::try_from(time) else {
        return "-".to_owned();
    };
    let zoned = timestamp.to_zoned(tz.clone());
    let mut output = String::new();
    if write!(&mut output, "{}", zoned.strftime(format)).is_err() {
        return "-".to_owned();
    }
    output
}
