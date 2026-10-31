//! What the Help screen says about the license and the packs (Phase 4: C32, C37).

use fds::i18n::Catalog;
use fds::license::{Entitlement, InvalidReason, LicenseStatus};
use fds::pack::{PackInfo, PackState, ProviderSpec, SkipReason};
use fds::supporter::{SupporterInfo, help_lines};

fn info(license: LicenseStatus, packs: Vec<PackInfo>, dev_key: bool) -> SupporterInfo {
    SupporterInfo {
        license,
        packs,
        dev_key,
    }
}

fn joined(info: &SupporterInfo) -> String {
    help_lines(info, &Catalog::english()).join("\n")
}

fn valid(packs: &[&str]) -> LicenseStatus {
    LicenseStatus::Valid(Entitlement {
        license_id: "FDS1-0001".into(),
        series: 1,
        packs: packs.iter().map(|p| p.to_string()).collect(),
        issued: "2026-10-20".into(),
        holder: None,
    })
}

fn pack(folder: &str, name: Option<&str>, official: bool, state: PackState, n: usize) -> PackInfo {
    let providers = (0..n)
        .map(|i| ProviderSpec {
            id: format!("p{i}"),
            label: format!("P{i}"),
            extensions: vec!["pdf".into()],
            command: vec!["t".into(), "{path}".into()],
        })
        .collect();
    PackInfo {
        folder: folder.into(),
        name: name.map(str::to_string),
        official,
        state,
        providers,
    }
}

#[test]
fn the_license_line_for_each_state() {
    let none = joined(&info(LicenseStatus::NotInstalled, vec![], false));
    assert!(none.contains("Supporter"), "{none}");
    assert!(none.contains("not installed"), "{none}");

    let ok = joined(&info(valid(&["viewer", "power"]), vec![], false));
    assert!(ok.contains("FDS1-0001"), "{ok}");
    assert!(ok.contains("1.x"), "{ok}");
    assert!(ok.contains("viewer, power"), "{ok}");

    let no_packs = joined(&info(valid(&[]), vec![], false));
    assert!(no_packs.contains("FDS1-0001"), "{no_packs}");
}

#[test]
fn every_invalid_reason_is_explained() {
    let cases = [
        (
            InvalidReason::Malformed("missing signature line".into()),
            "missing signature line",
        ),
        (InvalidReason::UnknownKey("k-2030".into()), "k-2030"),
        (InvalidReason::BadSignature, "signature"),
        (InvalidReason::WrongProduct("fdx".into()), "fdx"),
        (InvalidReason::SeriesMismatch { license: 2, app: 1 }, "2"),
        (
            InvalidReason::Unreadable("Access is denied".into()),
            "Access is denied",
        ),
    ];
    for (reason, part) in cases {
        let text = joined(&info(LicenseStatus::Invalid(reason.clone()), vec![], false));
        assert!(text.contains("not active"), "{reason:?}: {text}");
        assert!(text.contains(part), "{reason:?}: {text}");
    }
}

#[test]
fn the_test_key_warning_is_always_shown_when_accepted() {
    for license in [LicenseStatus::NotInstalled, valid(&["all"])] {
        let with = joined(&info(license.clone(), vec![], true));
        assert!(with.contains("TEST KEY"), "{with}");
        let without = joined(&info(license, vec![], false));
        assert!(!without.contains("TEST KEY"), "{without}");
    }
}

#[test]
fn each_pack_has_a_line_with_its_state() {
    let packs = vec![
        pack("a", Some("pdf-text"), false, PackState::Loaded, 2),
        pack(
            "b",
            Some("viewer-x"),
            true,
            PackState::Skipped(SkipReason::NoLicense),
            0,
        ),
        pack(
            "unnamed-folder",
            None,
            false,
            PackState::Skipped(SkipReason::Malformed("command has no {path}".into())),
            0,
        ),
        pack(
            "d",
            Some("ent-pack"),
            true,
            PackState::Skipped(SkipReason::NotEntitled("viewer".into())),
            0,
        ),
        pack(
            "e",
            Some("dup-pack"),
            false,
            PackState::Skipped(SkipReason::Duplicate("p0".into())),
            0,
        ),
        pack(
            "f",
            Some("key-pack"),
            true,
            PackState::Skipped(SkipReason::UnknownKey("k9".into())),
            0,
        ),
        pack(
            "g",
            Some("unsigned-pack"),
            true,
            PackState::Skipped(SkipReason::Unsigned),
            0,
        ),
        pack(
            "h",
            Some("badsig-pack"),
            true,
            PackState::Skipped(SkipReason::BadSignature),
            0,
        ),
        pack(
            "i",
            Some("disabled-pack"),
            false,
            PackState::Skipped(SkipReason::Disabled),
            0,
        ),
    ];
    let lines = help_lines(
        &info(LicenseStatus::NotInstalled, packs, false),
        &Catalog::english(),
    );
    let find = |name: &str| {
        lines
            .iter()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("{name} missing in {lines:#?}"))
            .clone()
    };
    let a = find("pdf-text");
    assert!(a.contains("community") && a.contains('2'), "{a}");
    let b = find("viewer-x");
    assert!(b.contains("official") && b.contains("not loaded"), "{b}");
    let unnamed = find("unnamed-folder");
    assert!(unnamed.contains("command has no {path}"), "{unnamed}");
    assert!(find("ent-pack").contains("viewer"));
    assert!(find("dup-pack").contains("p0"));
    assert!(find("key-pack").contains("k9"));
    for name in ["unsigned-pack", "badsig-pack", "disabled-pack"] {
        assert!(find(name).contains("not loaded"), "{name}");
    }
    let distinct: std::collections::HashSet<&String> = lines.iter().collect();
    assert_eq!(distinct.len(), lines.len(), "one line per pack");
}

#[test]
fn values_from_files_cannot_put_control_characters_on_screen() {
    let evil = "x\u{1b}[2J\u{7}y";
    let packs = vec![pack("f", Some(evil), false, PackState::Loaded, 1)];
    let license = LicenseStatus::Invalid(InvalidReason::Malformed(evil.into()));
    let text = joined(&info(license, packs, false));
    for bad in ['\u{1b}', '\u{7}'] {
        assert!(!text.contains(bad), "{text:?}");
    }
}
