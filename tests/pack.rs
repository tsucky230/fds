//! Packs and their manifests (Phase 4: R-800 to R-803, C24 to C26, C38).

use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signer, SigningKey};
use fds::license::{Entitlement, InvalidReason, LicenseStatus};
use fds::pack::{PackState, SkipReason, load_packs, parse_manifest, valid_extension};

const KEY_ID: &str = "test-key";

fn signing() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn keys() -> Vec<(&'static str, [u8; 32])> {
    vec![(KEY_ID, signing().verifying_key().to_bytes())]
}

fn licensed(packs: &[&str]) -> LicenseStatus {
    LicenseStatus::Valid(Entitlement {
        license_id: "X".into(),
        series: 1,
        packs: packs.iter().map(|p| p.to_string()).collect(),
        issued: "d".into(),
        holder: None,
    })
}

fn community(name: &str, provider: &str, ext: &str) -> String {
    format!(
        "name = \"{name}\"\nversion = \"1.0.0\"\n\n[[provider]]\nid = \"{provider}\"\nlabel = \"{provider} view\"\nextensions = [\"{ext}\"]\ncommand = [\"tool\", \"{{path}}\"]\n"
    )
}

fn official(name: &str, pack_id: &str, provider: &str) -> String {
    format!(
        "name = \"{name}\"\nversion = \"1.0.0\"\nofficial = true\npack_id = \"{pack_id}\"\n\n[[provider]]\nid = \"{provider}\"\nlabel = \"{provider} view\"\nextensions = [\"pdf\"]\ncommand = [\"tool\", \"{{path}}\"]\n"
    )
}

fn sig_for(bytes: &[u8], key_id: &str, key: &SigningKey) -> String {
    format!(
        "fds-pack-signature-v1\nkey: {key_id}\nsignature: {}\n",
        B64.encode(key.sign(bytes).to_bytes())
    )
}

/// Writes `<config>/packs/<folder>/pack.toml` (and the signature when given).
fn put(config: &Path, folder: &str, manifest: &str, sig: Option<&str>) {
    let dir = config.join("packs").join(folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("pack.toml"), manifest).unwrap();
    if let Some(sig) = sig {
        fs::write(dir.join("pack.toml.sig"), sig).unwrap();
    }
}

fn states(config: &Path, license: &LicenseStatus, disabled: &[&str]) -> Vec<(String, PackState)> {
    let disabled: Vec<String> = disabled.iter().map(|s| s.to_string()).collect();
    load_packs(Some(config), license, &keys(), &disabled)
        .into_iter()
        .map(|p| (p.folder, p.state))
        .collect()
}

// ---------- extensions (C38) ----------

#[test]
fn extension_rule() {
    for ext in [
        "pdf",
        "PDF",
        "7z",
        "m4a",
        "tar-gz",
        "c_",
        "abcdefghijklmnop",
    ] {
        assert!(valid_extension(ext), "{ext} should be accepted");
    }
    for ext in [
        "",
        "*",
        "?",
        "*.pdf",
        ".pdf",
        "a.b",
        "a/b",
        "a\\b",
        "p df",
        " pdf",
        "ｐｄｆ",
        "été",
        "abcdefghijklmnopq",
    ] {
        assert!(!valid_extension(ext), "{ext:?} should be rejected");
    }
}

// ---------- manifest ----------

#[test]
fn a_full_manifest_is_read() {
    let text = "name = \"pdf-text\"\nversion = \"1.2.0\"\nfuture_key = 1\n\n[[provider]]\nid = \"pdf-text\"\nlabel = \"PDF (text)\"\nextensions = [\"PDF\", \"ai\"]\ncommand = [\"pdftotext\", \"-layout\", \"{path}\", \"-\"]\n\n[[provider]]\nid = \"pdf-info\"\nlabel = \"PDF info\"\nextensions = [\"pdf\"]\ncommand = [\"bin/pdfinfo\", \"{path}\"]\n";
    let m = parse_manifest(text.as_bytes()).unwrap();
    assert_eq!(m.name, "pdf-text");
    assert_eq!(m.version, "1.2.0");
    assert!(!m.official);
    assert_eq!(m.pack_id, None);
    assert_eq!(m.providers.len(), 2);
    assert_eq!(m.providers[0].extensions, ["pdf", "ai"], "lowercased");
    assert_eq!(
        m.providers[0].command,
        ["pdftotext", "-layout", "{path}", "-"]
    );
    let off = parse_manifest(official("v", "viewer", "p").as_bytes()).unwrap();
    assert!(off.official);
    assert_eq!(off.pack_id.as_deref(), Some("viewer"));
}

#[test]
fn invalid_manifests_are_rejected_with_a_reason() {
    let ok = community("p", "x", "pdf");
    let cases: Vec<(&str, String)> = vec![
        ("not toml", "name = ".into()),
        ("no name", ok.replace("name = \"p\"\n", "")),
        (
            "name with space",
            ok.replace("name = \"p\"", "name = \"a b\""),
        ),
        (
            "name with path",
            ok.replace("name = \"p\"", "name = \"../x\""),
        ),
        (
            "name too long",
            ok.replace("name = \"p\"", &format!("name = \"{}\"", "n".repeat(65))),
        ),
        ("no version", ok.replace("version = \"1.0.0\"\n", "")),
        (
            "empty version",
            ok.replace("version = \"1.0.0\"", "version = \"\""),
        ),
        ("no provider", "name = \"p\"\nversion = \"1\"\n".into()),
        ("no id", ok.replace("id = \"x\"\n", "")),
        ("bad id", ok.replace("id = \"x\"", "id = \"x y\"")),
        (
            "empty label",
            ok.replace("label = \"x view\"", "label = \"\""),
        ),
        (
            "label too long",
            ok.replace(
                "label = \"x view\"",
                &format!("label = \"{}\"", "l".repeat(41)),
            ),
        ),
        (
            "label control",
            ok.replace("label = \"x view\"", "label = \"a\\u001b[2Jb\""),
        ),
        (
            "no extensions",
            ok.replace("extensions = [\"pdf\"]", "extensions = []"),
        ),
        (
            "star extension",
            ok.replace("extensions = [\"pdf\"]", "extensions = [\"*\"]"),
        ),
        (
            "empty extension",
            ok.replace("extensions = [\"pdf\"]", "extensions = [\"pdf\", \"\"]"),
        ),
        (
            "dotted extension",
            ok.replace("extensions = [\"pdf\"]", "extensions = [\".pdf\"]"),
        ),
        (
            "empty command",
            ok.replace("command = [\"tool\", \"{path}\"]", "command = []"),
        ),
        (
            "empty program",
            ok.replace(
                "command = [\"tool\", \"{path}\"]",
                "command = [\"\", \"{path}\"]",
            ),
        ),
        (
            "path as program",
            ok.replace("command = [\"tool\", \"{path}\"]", "command = [\"{path}\"]"),
        ),
        (
            "no path argument",
            ok.replace(
                "command = [\"tool\", \"{path}\"]",
                "command = [\"tool\", \"-\"]",
            ),
        ),
        (
            "path only inside an argument",
            ok.replace(
                "command = [\"tool\", \"{path}\"]",
                "command = [\"tool\", \"--in={path}\"]",
            ),
        ),
        (
            "official without pack_id",
            ok.replace(
                "version = \"1.0.0\"",
                "version = \"1.0.0\"\nofficial = true",
            ),
        ),
        ("official with unknown pack_id", official("p", "games", "x")),
    ];
    for (name, text) in cases {
        let result = parse_manifest(text.as_bytes());
        let error = result.expect_err(name);
        assert!(!error.is_empty(), "{name}");
    }
    let dup = format!(
        "{ok}\n[[provider]]\nid = \"x\"\nlabel = \"again\"\nextensions = [\"txt\"]\ncommand = [\"t\", \"{{path}}\"]\n"
    );
    assert!(parse_manifest(dup.as_bytes()).is_err(), "duplicate id");
    assert!(parse_manifest(&[0xff, 0xfe]).is_err(), "not utf-8");
}

// ---------- loading (C24 to C26) ----------

#[test]
fn nothing_to_load() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(load_packs(None, &LicenseStatus::NotInstalled, &keys(), &[]).is_empty());
    assert!(states(tmp.path(), &LicenseStatus::NotInstalled, &[]).is_empty());
    fs::create_dir_all(tmp.path().join("packs")).unwrap();
    fs::write(tmp.path().join("packs/readme.txt"), "not a pack").unwrap();
    assert!(states(tmp.path(), &LicenseStatus::NotInstalled, &[]).is_empty());
}

#[test]
fn community_packs_load_without_a_license() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "a-pack", &community("a", "a1", "pdf"), None);
    let packs = load_packs(Some(tmp.path()), &LicenseStatus::NotInstalled, &keys(), &[]);
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].state, PackState::Loaded);
    assert!(!packs[0].official);
    assert_eq!(packs[0].name.as_deref(), Some("a"));
    assert_eq!(packs[0].providers.len(), 1);
    // An invalid license does not matter for community packs either.
    let invalid = LicenseStatus::Invalid(InvalidReason::BadSignature);
    assert_eq!(
        states(tmp.path(), &invalid, &[]),
        [("a-pack".to_string(), PackState::Loaded)]
    );
}

#[test]
fn programs_with_a_relative_path_are_inside_the_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let text = community("a", "a1", "pdf").replace("[\"tool\"", "[\"bin/tool\"");
    put(tmp.path(), "a", &text, None);
    put(tmp.path(), "b", &community("b", "b1", "txt"), None);
    let packs = load_packs(Some(tmp.path()), &LicenseStatus::NotInstalled, &keys(), &[]);
    let program_a = Path::new(&packs[0].providers[0].command[0]).to_path_buf();
    assert_eq!(
        program_a,
        tmp.path().join("packs").join("a").join("bin/tool")
    );
    assert_eq!(
        packs[1].providers[0].command[0], "tool",
        "bare names use PATH"
    );
}

#[test]
fn disabled_packs_by_folder_or_name() {
    let tmp = tempfile::tempdir().unwrap();
    put(
        tmp.path(),
        "folder-a",
        &community("name-a", "a1", "pdf"),
        None,
    );
    put(
        tmp.path(),
        "folder-b",
        &community("name-b", "b1", "txt"),
        None,
    );
    put(
        tmp.path(),
        "folder-c",
        &community("name-c", "c1", "md"),
        None,
    );
    let got = states(
        tmp.path(),
        &LicenseStatus::NotInstalled,
        &["folder-a", "name-b"],
    );
    assert_eq!(
        got,
        [
            (
                "folder-a".to_string(),
                PackState::Skipped(SkipReason::Disabled)
            ),
            (
                "folder-b".to_string(),
                PackState::Skipped(SkipReason::Disabled)
            ),
            ("folder-c".to_string(), PackState::Loaded),
        ]
    );
}

#[test]
fn a_broken_pack_does_not_stop_the_others() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "1-broken", "name = ", None);
    fs::create_dir_all(tmp.path().join("packs/2-empty")).unwrap();
    put(tmp.path(), "3-good", &community("good", "g", "pdf"), None);
    let big = format!(
        "{}\n{}",
        "#".repeat(70 * 1024),
        community("big", "b", "txt")
    );
    put(tmp.path(), "4-big", &big, None);
    let got = states(tmp.path(), &LicenseStatus::NotInstalled, &[]);
    assert_eq!(got.len(), 4);
    assert!(matches!(&got[0].1, PackState::Skipped(SkipReason::Malformed(t)) if !t.is_empty()));
    assert!(
        matches!(&got[1].1, PackState::Skipped(SkipReason::Malformed(_))),
        "no pack.toml"
    );
    assert_eq!(got[2].1, PackState::Loaded);
    assert!(
        matches!(&got[3].1, PackState::Skipped(SkipReason::Malformed(_))),
        "too large"
    );
}

#[test]
fn official_packs_need_a_signature_and_an_entitlement() {
    let text = official("off", "viewer", "o1");
    let good_sig = sig_for(text.as_bytes(), KEY_ID, &signing());
    let other_bytes_sig = sig_for(b"something else", KEY_ID, &signing());
    let forged_sig = sig_for(text.as_bytes(), KEY_ID, &SigningKey::from_bytes(&[8; 32]));
    let unknown_sig = sig_for(text.as_bytes(), "other-key", &signing());
    let cases: Vec<(&str, Option<String>, LicenseStatus, PackState)> = vec![
        (
            "unsigned",
            None,
            licensed(&["all"]),
            PackState::Skipped(SkipReason::Unsigned),
        ),
        (
            "signed other bytes",
            Some(other_bytes_sig),
            licensed(&["all"]),
            PackState::Skipped(SkipReason::BadSignature),
        ),
        (
            "forged",
            Some(forged_sig),
            licensed(&["all"]),
            PackState::Skipped(SkipReason::BadSignature),
        ),
        (
            "garbage signature",
            Some("fds-pack-signature-v1\nkey: test-key\nsignature: !!\n".into()),
            licensed(&["all"]),
            PackState::Skipped(SkipReason::BadSignature),
        ),
        (
            "unknown key",
            Some(unknown_sig),
            licensed(&["all"]),
            PackState::Skipped(SkipReason::UnknownKey("other-key".into())),
        ),
        (
            "no license",
            Some(good_sig.clone()),
            LicenseStatus::NotInstalled,
            PackState::Skipped(SkipReason::NoLicense),
        ),
        (
            "invalid license",
            Some(good_sig.clone()),
            LicenseStatus::Invalid(InvalidReason::BadSignature),
            PackState::Skipped(SkipReason::NoLicense),
        ),
        (
            "other pack licensed",
            Some(good_sig.clone()),
            licensed(&["developer"]),
            PackState::Skipped(SkipReason::NotEntitled("viewer".into())),
        ),
        (
            "licensed for viewer",
            Some(good_sig.clone()),
            licensed(&["viewer"]),
            PackState::Loaded,
        ),
        (
            "licensed for all",
            Some(good_sig.clone()),
            licensed(&["all"]),
            PackState::Loaded,
        ),
    ];
    for (name, sig, license, expected) in cases {
        let tmp = tempfile::tempdir().unwrap();
        put(tmp.path(), "off", &text, sig.as_deref());
        let packs = load_packs(Some(tmp.path()), &license, &keys(), &[]);
        assert_eq!(packs.len(), 1, "{name}");
        assert!(packs[0].official, "{name}");
        assert_eq!(packs[0].state, expected, "{name}");
        assert_eq!(
            packs[0].providers.is_empty(),
            expected != PackState::Loaded,
            "{name}: providers only when loaded"
        );
    }
}

#[test]
fn a_changed_manifest_breaks_its_signature() {
    let text = official("off", "viewer", "o1");
    let sig = sig_for(text.as_bytes(), KEY_ID, &signing());
    let changed = text.replace("[\"tool\"", "[\"evil\"");
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "off", &changed, Some(&sig));
    assert_eq!(
        states(tmp.path(), &licensed(&["all"]), &[]),
        [(
            "off".to_string(),
            PackState::Skipped(SkipReason::BadSignature)
        )]
    );
}

#[test]
fn duplicate_providers_keep_the_first_pack_in_folder_order() {
    let tmp = tempfile::tempdir().unwrap();
    put(
        tmp.path(),
        "b-second",
        &community("second", "same", "txt"),
        None,
    );
    put(
        tmp.path(),
        "a-first",
        &community("first", "same", "pdf"),
        None,
    );
    let got = states(tmp.path(), &LicenseStatus::NotInstalled, &[]);
    assert_eq!(got[0], ("a-first".to_string(), PackState::Loaded));
    assert_eq!(
        got[1],
        (
            "b-second".to_string(),
            PackState::Skipped(SkipReason::Duplicate("same".into()))
        )
    );
}
