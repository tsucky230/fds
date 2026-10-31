//! Places (drives, home, mounts) for the destination popup, and how a destination relates to the
//! source folder (M23 UX follow-up). Pure logic; the filesystem is injected so it is testable.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceKind {
    /// The folder the sources came from (added by App while choosing a destination).
    Origin,
    Home,
    /// A Windows drive such as "C:".
    Drive,
    /// Unix root "/".
    Root,
    /// A Unix mount point under /mnt, /media, /run/media.
    Mount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub kind: PlaceKind,
    /// Raw label: the drive name ("C:") or the path for mounts. Home / Origin / Root are labelled
    /// by the UI from the language file, so this is only a fallback for them.
    pub label: String,
    pub path: PathBuf,
}

/// How the destination folder relates to the folder the sources came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// The very same folder.
    Same,
    /// A folder above the origin (an ancestor).
    Above,
    /// A folder inside the origin.
    Below,
    /// Another Windows drive.
    OtherDrive,
    Elsewhere,
}

/// Relation of `dest` to `origin`.
///
/// Behavior:
/// - equal paths -> Same
/// - `origin` starts with `dest` -> Above; `dest` starts with `origin` -> Below
/// - On Windows, a different drive prefix (letters compared case-insensitively) -> OtherDrive
/// - otherwise Elsewhere
pub fn relation(origin: &Path, dest: &Path) -> Relation {
    if origin == dest {
        return Relation::Same;
    }
    if origin.starts_with(dest) {
        return Relation::Above;
    }
    if dest.starts_with(origin) {
        return Relation::Below;
    }
    if drive_of(origin).is_some() && drive_of(dest).is_some() && drive_of(origin) != drive_of(dest)
    {
        return Relation::OtherDrive;
    }
    Relation::Elsewhere
}

/// Upper-cased drive prefix of a Windows path (e.g. "C:"), if it has one.
fn drive_of(path: &Path) -> Option<String> {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            Some(prefix.as_os_str().to_string_lossy().to_uppercase())
        }
        _ => None,
    }
}

/// The places to offer in the popup, in display order.
///
/// Behavior:
/// - Home first, when the home variable (`USERPROFILE` on Windows, `HOME` elsewhere) is set and
///   the folder exists.
/// - Windows: every drive "A:".."Z:" whose root `X:\` exists (kind Drive, label "X:").
/// - Unix: the root "/" (kind Root), then the sub-folders of /mnt, /media and /run/media/<user>
///   (kind Mount, label = full path), in the order `read_dir` returns them.
/// - Nothing is returned for places that do not exist; never panics.
pub fn list_places(
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
    read_dir: &dyn Fn(&Path) -> Vec<PathBuf>,
    windows: bool,
) -> Vec<Place> {
    let mut places = Vec::new();
    let home_var = if windows { "USERPROFILE" } else { "HOME" };
    if let Some(home) = env(home_var).filter(|h| !h.is_empty()) {
        let path = PathBuf::from(home);
        if exists(&path) {
            places.push(Place {
                kind: PlaceKind::Home,
                label: path.to_string_lossy().into_owned(),
                path,
            });
        }
    }
    if windows {
        for letter in 'A'..='Z' {
            let path = PathBuf::from(format!("{letter}:\\"));
            if exists(&path) {
                places.push(Place {
                    kind: PlaceKind::Drive,
                    label: format!("{letter}:"),
                    path,
                });
            }
        }
        return places;
    }
    let root = PathBuf::from("/");
    if exists(&root) {
        places.push(Place {
            kind: PlaceKind::Root,
            label: "/".to_string(),
            path: root,
        });
    }
    let mut bases = vec![PathBuf::from("/mnt"), PathBuf::from("/media")];
    if let Some(user) = env("USER").filter(|u| !u.is_empty()) {
        bases.push(PathBuf::from("/run/media").join(user));
    }
    for base in bases {
        if !exists(&base) {
            continue;
        }
        for path in read_dir(&base) {
            places.push(Place {
                kind: PlaceKind::Mount,
                label: path.to_string_lossy().into_owned(),
                path,
            });
        }
    }
    places
}

/// `list_places` against the real filesystem (used by the loader thread).
pub fn list_places_real() -> Vec<Place> {
    list_places(
        &|key| std::env::var(key).ok(),
        &|path| path.is_dir(),
        &|dir| {
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .map(|e| e.path())
                        .filter(|p| p.is_dir())
                        .collect()
                })
                .unwrap_or_default();
            dirs.sort();
            dirs
        },
        cfg!(windows),
    )
}
