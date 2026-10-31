//! Places (drives, home, mounts) and the relation of a destination to the source folder (M23 UX).

use std::path::{Path, PathBuf};

use fds::ops::place::{PlaceKind, Relation, list_places, relation};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// ---------- relation ----------

#[test]
fn relation_same_above_below() {
    let origin = p(if cfg!(windows) {
        r"C:\work\proj"
    } else {
        "/work/proj"
    });
    let parent = origin.parent().unwrap().to_path_buf();
    let child = origin.join("sub");
    assert_eq!(relation(&origin, &origin), Relation::Same);
    assert_eq!(relation(&origin, &parent), Relation::Above);
    assert_eq!(relation(&origin, &child), Relation::Below);
    let sibling = parent.join("other");
    assert_eq!(relation(&origin, &sibling), Relation::Elsewhere);
}

#[cfg(windows)]
#[test]
fn relation_other_drive_on_windows() {
    assert_eq!(
        relation(Path::new(r"C:\work"), Path::new(r"D:\backup")),
        Relation::OtherDrive
    );
    assert_eq!(
        relation(Path::new(r"C:\work"), Path::new(r"c:\other")),
        Relation::Elsewhere,
        "drive letters compare case-insensitively"
    );
}

// ---------- places ----------

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn windows_lists_existing_drives_and_home() {
    let exists = |path: &Path| {
        matches!(
            path.to_str(),
            Some(r"C:\") | Some(r"E:\") | Some(r"C:\Users\me")
        )
    };
    let env = |k: &str| (k == "USERPROFILE").then(|| r"C:\Users\me".to_string());
    let places = list_places(&env, &exists, &|_| Vec::new(), true);
    let drives: Vec<_> = places
        .iter()
        .filter(|x| x.kind == PlaceKind::Drive)
        .map(|x| x.label.as_str())
        .collect();
    assert_eq!(drives, ["C:", "E:"], "only drives that exist");
    let home = places
        .iter()
        .find(|x| x.kind == PlaceKind::Home)
        .expect("home");
    assert_eq!(home.path, p(r"C:\Users\me"));
    assert_eq!(
        places.iter().position(|x| x.kind == PlaceKind::Home),
        Some(0),
        "home comes first"
    );
}

#[test]
fn unix_lists_root_home_and_mounts() {
    let exists = |path: &Path| {
        path.to_str()
            .is_some_and(|s| ["/", "/home/me", "/mnt", "/media"].contains(&s))
    };
    let env = |k: &str| (k == "HOME").then(|| "/home/me".to_string());
    let read_dir = |dir: &Path| match dir.to_str() {
        Some("/mnt") => vec![p("/mnt/usb"), p("/mnt/e")],
        Some("/media") => vec![p("/media/disk")],
        _ => Vec::new(),
    };
    let places = list_places(&env, &exists, &read_dir, false);
    assert_eq!(places[0].kind, PlaceKind::Home);
    assert!(
        places
            .iter()
            .any(|x| x.kind == PlaceKind::Root && x.path == p("/"))
    );
    let mounts: Vec<_> = places
        .iter()
        .filter(|x| x.kind == PlaceKind::Mount)
        .map(|x| x.path.clone())
        .collect();
    assert_eq!(mounts, [p("/mnt/usb"), p("/mnt/e"), p("/media/disk")]);
}

#[test]
fn missing_home_and_mounts_are_skipped() {
    let places = list_places(&no_env, &|_| false, &|_| Vec::new(), false);
    assert!(
        places
            .iter()
            .all(|x| x.kind != PlaceKind::Home && x.kind != PlaceKind::Mount)
    );
}
