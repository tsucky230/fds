//! Supporter license verification (Phase 4: R-1000 to R-1004, C15 to C23).
//!
//! The tests sign with a TEST key pair made from a fixed seed. Its public key must never be an
//! official key (C21).

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signer, SigningKey};
use fds::license::{
    Entitlement, InvalidReason, LicenseStatus, MAX_LICENSE_BYTES, OFFICIAL_KEYS, accepts_dev_key,
    build_keys, load, required_series, verify,
};

const SEED: [u8; 32] = [7; 32];
const KEY_ID: &str = "test-key";

fn signing() -> SigningKey {
    SigningKey::from_bytes(&SEED)
}

fn keys() -> Vec<(&'static str, [u8; 32])> {
    vec![(KEY_ID, signing().verifying_key().to_bytes())]
}

const PAYLOAD: &str = "license_id = \"FDS1-0001\"\nproduct = \"fds\"\nseries = 1\npacks = [\"all\"]\nissued = \"2026-10-20\"\nholder = \"Tester\"\n";

/// A license file for `payload` signed by `key` and labelled `key_id`.
fn file_with(key_id: &str, payload: &[u8], key: &SigningKey) -> String {
    let signature = key.sign(payload).to_bytes();
    format!(
        "fds-license-v1\nkey: {key_id}\npayload: {}\nsignature: {}\n",
        B64.encode(payload),
        B64.encode(signature)
    )
}

fn signed(payload: &str) -> String {
    file_with(KEY_ID, payload.as_bytes(), &signing())
}

fn status(content: &str, app_major: u32) -> LicenseStatus {
    verify(content.as_bytes(), &keys(), app_major)
}

fn malformed(s: &LicenseStatus) -> bool {
    matches!(s, LicenseStatus::Invalid(InvalidReason::Malformed(text)) if !text.is_empty())
}

// ---------- valid licenses (C15) ----------

#[test]
fn a_signed_license_is_valid_and_readable() {
    let s = status(&signed(PAYLOAD), 1);
    assert_eq!(
        s,
        LicenseStatus::Valid(Entitlement {
            license_id: "FDS1-0001".into(),
            series: 1,
            packs: vec!["all".into()],
            issued: "2026-10-20".into(),
            holder: Some("Tester".into()),
        })
    );
}

#[test]
fn layout_variations_are_accepted() {
    let base = signed(PAYLOAD);
    let lines: Vec<&str> = base.lines().collect();
    let crlf = base.replace('\n', "\r\n");
    let commented = format!("# FDS Supporter license\n\n{}\n\n# end\n", base);
    let reordered = format!("{}\n{}\n{}\n{}\n", lines[0], lines[3], lines[1], lines[2]);
    let spaced = base
        .replace("key: ", "key:   ")
        .replace("signature: ", "signature:\t");
    for (name, text) in [
        ("crlf", crlf),
        ("comments", commented),
        ("order", reordered),
        ("spaces", spaced),
    ] {
        assert!(
            matches!(status(&text, 1), LicenseStatus::Valid(_)),
            "{name}: {:?}",
            status(&text, 1)
        );
    }
}

#[test]
fn holder_is_optional_and_unknown_keys_are_ignored() {
    let payload = "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\npacks = []\nissued = \"d\"\nfuture = 3\n";
    let LicenseStatus::Valid(e) = status(&signed(payload), 1) else {
        panic!()
    };
    assert_eq!(e.holder, None);
    assert!(e.packs.is_empty());
}

// ---------- tampering (C16) ----------

#[test]
fn a_changed_payload_byte_is_a_bad_signature() {
    let payload = PAYLOAD.as_bytes();
    let signature = signing().sign(payload).to_bytes();
    for at in [0, payload.len() / 2, payload.len() - 1] {
        let mut changed = payload.to_vec();
        changed[at] ^= 0x01;
        let text = format!(
            "fds-license-v1\nkey: {KEY_ID}\npayload: {}\nsignature: {}\n",
            B64.encode(&changed),
            B64.encode(signature)
        );
        assert_eq!(
            status(&text, 1),
            LicenseStatus::Invalid(InvalidReason::BadSignature),
            "byte {at}"
        );
    }
}

#[test]
fn a_changed_signature_byte_is_a_bad_signature() {
    let payload = PAYLOAD.as_bytes();
    let signature = signing().sign(payload).to_bytes();
    for at in [0, 31, 63] {
        let mut changed = signature;
        changed[at] ^= 0x80;
        let text = format!(
            "fds-license-v1\nkey: {KEY_ID}\npayload: {}\nsignature: {}\n",
            B64.encode(payload),
            B64.encode(changed)
        );
        assert_eq!(
            status(&text, 1),
            LicenseStatus::Invalid(InvalidReason::BadSignature),
            "byte {at}"
        );
    }
}

#[test]
fn another_key_under_a_known_id_is_a_bad_signature() {
    let forger = SigningKey::from_bytes(&[8; 32]);
    let text = file_with(KEY_ID, PAYLOAD.as_bytes(), &forger);
    assert_eq!(
        status(&text, 1),
        LicenseStatus::Invalid(InvalidReason::BadSignature)
    );
}

// ---------- keys (C17, C21) ----------

#[test]
fn an_unknown_key_id_is_reported() {
    let text = file_with("other-key", PAYLOAD.as_bytes(), &signing());
    assert_eq!(
        status(&text, 1),
        LicenseStatus::Invalid(InvalidReason::UnknownKey("other-key".into()))
    );
}

#[test]
fn with_no_keys_nothing_is_valid() {
    let s = verify(signed(PAYLOAD).as_bytes(), &[], 1);
    assert_eq!(
        s,
        LicenseStatus::Invalid(InvalidReason::UnknownKey(KEY_ID.into()))
    );
}

#[test]
fn the_test_key_is_never_an_official_key() {
    let test_public = signing().verifying_key().to_bytes();
    assert!(OFFICIAL_KEYS.iter().all(|(_, k)| *k != test_public));
    // Tests are built without dev-license-key: this build verifies with the official keys only.
    assert!(!accepts_dev_key());
    assert_eq!(build_keys(), OFFICIAL_KEYS.to_vec());
    // The official list as shipped cannot validate the test license.
    assert!(!matches!(
        verify(signed(PAYLOAD).as_bytes(), &build_keys(), 1),
        LicenseStatus::Valid(_)
    ));
}

// ---------- malformed files (C18) ----------

#[test]
fn broken_files_are_malformed_and_never_panic() {
    let good = signed(PAYLOAD);
    let lines: Vec<&str> = good.lines().collect();
    let (key, payload, signature) = (lines[1], lines[2], lines[3]);
    let sig63 = format!("signature: {}", B64.encode([1u8; 63]));
    let sig65 = format!("signature: {}", B64.encode([1u8; 65]));
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("only blank lines", b"\n\n\r\n".to_vec()),
        ("not utf-8", vec![0xff, 0xfe, 0x00, 0x80]),
        (
            "no format line",
            format!("{key}\n{payload}\n{signature}\n").into_bytes(),
        ),
        (
            "other format",
            format!("fds-license-v2\n{key}\n{payload}\n{signature}\n").into_bytes(),
        ),
        (
            "format not first",
            format!("{key}\nfds-license-v1\n{payload}\n{signature}\n").into_bytes(),
        ),
        (
            "no key",
            format!("fds-license-v1\n{payload}\n{signature}\n").into_bytes(),
        ),
        (
            "no payload",
            format!("fds-license-v1\n{key}\n{signature}\n").into_bytes(),
        ),
        (
            "no signature",
            format!("fds-license-v1\n{key}\n{payload}\n").into_bytes(),
        ),
        (
            "two keys",
            format!("fds-license-v1\n{key}\n{key}\n{payload}\n{signature}\n").into_bytes(),
        ),
        ("extra line", format!("{good}extra: 1\n").into_bytes()),
        (
            "payload not base64",
            format!("fds-license-v1\n{key}\npayload: @@@\n{signature}\n").into_bytes(),
        ),
        (
            "signature not base64",
            format!("fds-license-v1\n{key}\n{payload}\nsignature: !!\n").into_bytes(),
        ),
        (
            "signature 63 bytes",
            format!("fds-license-v1\n{key}\n{payload}\n{sig63}\n").into_bytes(),
        ),
        (
            "signature 65 bytes",
            format!("fds-license-v1\n{key}\n{payload}\n{sig65}\n").into_bytes(),
        ),
        (
            "empty key",
            format!("fds-license-v1\nkey:\n{payload}\n{signature}\n").into_bytes(),
        ),
    ];
    for (name, bytes) in cases {
        let s = verify(&bytes, &keys(), 1);
        assert!(malformed(&s), "{name}: {s:?}");
    }
}

#[test]
fn a_signed_but_broken_payload_is_malformed() {
    // Signed correctly, so only the payload check (step 4) can reject them.
    let cases = [
        ("not toml", "license_id = \n"),
        (
            "no license_id",
            "product = \"fds\"\nseries = 1\npacks = []\nissued = \"d\"\n",
        ),
        (
            "empty license_id",
            "license_id = \"\"\nproduct = \"fds\"\nseries = 1\npacks = []\nissued = \"d\"\n",
        ),
        (
            "license_id too long",
            "license_id = \"0123456789012345678901234567890123456789012345678901234567890123x\"\nproduct = \"fds\"\nseries = 1\npacks = []\nissued = \"d\"\n",
        ),
        (
            "license_id control",
            "license_id = \"a\\u001bb\"\nproduct = \"fds\"\nseries = 1\npacks = []\nissued = \"d\"\n",
        ),
        (
            "no product",
            "license_id = \"X\"\nseries = 1\npacks = []\nissued = \"d\"\n",
        ),
        (
            "series 0",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 0\npacks = []\nissued = \"d\"\n",
        ),
        (
            "series -1",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = -1\npacks = []\nissued = \"d\"\n",
        ),
        (
            "series string",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = \"1\"\npacks = []\nissued = \"d\"\n",
        ),
        (
            "series float",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1.0\npacks = []\nissued = \"d\"\n",
        ),
        (
            "packs string",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\npacks = \"all\"\nissued = \"d\"\n",
        ),
        (
            "packs empty name",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\npacks = [\"\"]\nissued = \"d\"\n",
        ),
        (
            "packs number",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\npacks = [1]\nissued = \"d\"\n",
        ),
        (
            "no packs",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\nissued = \"d\"\n",
        ),
        (
            "no issued",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\npacks = []\n",
        ),
        (
            "empty issued",
            "license_id = \"X\"\nproduct = \"fds\"\nseries = 1\npacks = []\nissued = \"\"\n",
        ),
    ];
    for (name, payload) in cases {
        let s = status(&signed(payload), 1);
        assert!(malformed(&s), "{name}: {s:?}");
    }
    // A payload that is not UTF-8 at all.
    let s = verify(
        file_with(KEY_ID, &[0xff, 0xfe, 0xfd], &signing()).as_bytes(),
        &keys(),
        1,
    );
    assert!(malformed(&s), "binary payload: {s:?}");
}

// ---------- product and series (C19) ----------

#[test]
fn another_product_is_rejected() {
    let payload = PAYLOAD.replace("product = \"fds\"", "product = \"fdx\"");
    assert_eq!(
        status(&signed(&payload), 1),
        LicenseStatus::Invalid(InvalidReason::WrongProduct("fdx".into()))
    );
}

#[test]
fn the_series_must_match_the_major_version() {
    let for_series = |n: u32| signed(&PAYLOAD.replace("series = 1", &format!("series = {n}")));
    // (license series, app major, valid?)
    for (series, app, valid) in [
        (1, 0, true), // pre-1.0 builds accept series 1
        (1, 1, true),
        (2, 2, true),
        (3, 3, true),
        (2, 0, false),
        (2, 1, false),
        (1, 2, false),
        (3, 2, false),
    ] {
        let s = status(&for_series(series), app);
        if valid {
            assert!(
                matches!(s, LicenseStatus::Valid(_)),
                "{series} on {app}: {s:?}"
            );
        } else {
            assert_eq!(
                s,
                LicenseStatus::Invalid(InvalidReason::SeriesMismatch {
                    license: series,
                    app
                }),
                "{series} on {app}"
            );
        }
    }
}

#[test]
fn required_series_rule() {
    for (major, series) in [(0, 1), (1, 1), (2, 2), (9, 9), (10, 10)] {
        assert_eq!(required_series(major), series, "{major}");
    }
}

// ---------- entitlement (C23) ----------

#[test]
fn packs_allow_only_what_they_name() {
    let with = |packs: &[&str]| Entitlement {
        license_id: "X".into(),
        series: 1,
        packs: packs.iter().map(|p| p.to_string()).collect(),
        issued: "d".into(),
        holder: None,
    };
    for (packs, pack, allowed) in [
        (&["all"][..], "viewer", true),
        (&["all"][..], "power", true),
        (&["all"][..], "anything", true),
        (&["viewer"][..], "viewer", true),
        (&["viewer"][..], "power", false),
        (&["viewer"][..], "Viewer", false),
        (&["viewer", "remote"][..], "remote", true),
        (&[][..], "viewer", false),
        (&["unknown"][..], "viewer", false),
        (&["ALL"][..], "viewer", false),
    ] {
        assert_eq!(with(packs).allows(pack), allowed, "{packs:?} / {pack}");
    }
}

// ---------- reading the file (C20) ----------

#[test]
fn loading_from_the_config_folder() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(load(None, &keys(), 1), LicenseStatus::NotInstalled);
    assert_eq!(
        load(Some(tmp.path()), &keys(), 1),
        LicenseStatus::NotInstalled
    );

    let file = tmp.path().join("supporter.license");
    std::fs::write(&file, signed(PAYLOAD)).unwrap();
    assert!(matches!(
        load(Some(tmp.path()), &keys(), 1),
        LicenseStatus::Valid(_)
    ));

    let big = "#".repeat(MAX_LICENSE_BYTES as usize + 1);
    std::fs::write(&file, format!("{big}\n{}", signed(PAYLOAD))).unwrap();
    assert!(malformed(&load(Some(tmp.path()), &keys(), 1)));

    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(matches!(
        load(Some(tmp.path()), &keys(), 1),
        LicenseStatus::Invalid(InvalidReason::Unreadable(_))
    ));
}

// ---------- no network (C22) ----------

#[test]
fn no_network_crates_are_linked() {
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock")).unwrap();
    for name in [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "isahc",
        "attohttpc",
        "native-tls",
        "rustls",
        "h2",
    ] {
        assert!(
            !lock.contains(&format!("name = \"{name}\"")),
            "{name} would let the license check reach the network"
        );
    }
}
