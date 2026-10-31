//! Encoding detection and decoding (P2-5).

use fds::viewer::encoding::{decode, detect};

const JP: &str = "日本語のテキストです。漢字とかなを含みます。";

fn encode(name: &str, text: &str) -> Vec<u8> {
    let enc = encoding_rs::Encoding::for_label(name.as_bytes()).unwrap();
    let (bytes, _, had_errors) = enc.encode(text);
    assert!(!had_errors);
    bytes.into_owned()
}

#[test]
fn utf8_with_and_without_bom() {
    assert_eq!(decode(JP.as_bytes()), (JP.to_string(), "UTF-8"));
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(JP.as_bytes());
    assert_eq!(decode(&bom), (JP.to_string(), "UTF-8"));
}

#[test]
fn utf16_with_bom() {
    let mut le = vec![0xFF, 0xFE];
    for u in JP.encode_utf16() {
        le.extend_from_slice(&u.to_le_bytes());
    }
    assert_eq!(decode(&le), (JP.to_string(), "UTF-16LE"));
    let mut be = vec![0xFE, 0xFF];
    for u in JP.encode_utf16() {
        be.extend_from_slice(&u.to_be_bytes());
    }
    assert_eq!(decode(&be), (JP.to_string(), "UTF-16BE"));
}

#[test]
fn legacy_japanese_encodings() {
    // Repeat so the detector has enough evidence.
    let text = JP.repeat(4);
    for name in ["Shift_JIS", "EUC-JP"] {
        let bytes = encode(name, &text);
        assert_eq!(detect(&bytes).name(), name, "detect {name}");
        assert_eq!(decode(&bytes), (text.clone(), name));
    }
}

#[test]
fn iso_2022_jp_is_not_mistaken_for_utf8() {
    let bytes = encode("ISO-2022-JP", JP);
    assert!(
        std::str::from_utf8(&bytes).is_ok(),
        "precondition: 7-bit bytes are valid UTF-8"
    );
    assert_eq!(detect(&bytes).name(), "ISO-2022-JP");
    assert_eq!(decode(&bytes).0, JP);
}

#[test]
fn utf8_cut_in_the_middle_of_a_char_is_still_utf8() {
    // TEXT_LIMIT may cut the last character.
    let bytes = JP.as_bytes();
    let cut = &bytes[..bytes.len() - 1];
    assert_eq!(detect(cut).name(), "UTF-8");
    let (text, _) = decode(cut);
    assert!(text.starts_with("日本語"));
}

#[test]
fn ascii_and_empty() {
    assert_eq!(decode(b"hello"), ("hello".to_string(), "UTF-8"));
    assert_eq!(decode(b""), (String::new(), "UTF-8"));
}

#[test]
fn invalid_bytes_do_not_panic() {
    let garbage: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let (text, _) = decode(&garbage);
    assert!(!text.is_empty());
}

// ---------- manual switch (E, M1-b) ----------

use encoding_rs::Encoding;
use fds::viewer::encoding::{decode_with, next_choice};

const CYCLE: [&str; 7] = [
    "UTF-8",
    "Shift_JIS",
    "EUC-JP",
    "ISO-2022-JP",
    "UTF-16LE",
    "UTF-16BE",
    "windows-1252",
];

fn by_name(name: &str) -> &'static Encoding {
    Encoding::for_label(name.as_bytes()).unwrap()
}

fn utf16(text: &str, little: bool) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|u| {
            if little {
                u.to_le_bytes()
            } else {
                u.to_be_bytes()
            }
        })
        .collect()
}

#[test]
fn manual_cycle_visits_each_encoding_once_then_returns_to_auto() {
    let mut current = None;
    let mut seen = Vec::new();
    for _ in 0..CYCLE.len() {
        current = next_choice(current);
        seen.push(current.expect("a manual choice").name());
    }
    assert_eq!(seen, CYCLE);
    assert_eq!(next_choice(current), None, "back to automatic detection");
}

#[test]
fn an_encoding_outside_the_cycle_moves_to_utf8() {
    for enc in [encoding_rs::GBK, encoding_rs::BIG5, encoding_rs::KOI8_R] {
        assert_eq!(
            next_choice(Some(enc)),
            Some(encoding_rs::UTF_8),
            "{}",
            enc.name()
        );
    }
}

#[test]
fn forced_decoding_reads_every_manual_encoding() {
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        ("UTF-8", JP.as_bytes().to_vec(), JP),
        ("Shift_JIS", encode("shift_jis", JP), JP),
        ("EUC-JP", encode("euc-jp", JP), JP),
        ("ISO-2022-JP", encode("iso-2022-jp", JP), JP),
        ("UTF-16LE", utf16(JP, true), JP),
        ("UTF-16BE", utf16(JP, false), JP),
        ("windows-1252", vec![0x63, 0x61, 0x66, 0xE9], "café"),
    ];
    for (name, bytes, text) in cases {
        let enc = by_name(name);
        assert_eq!(
            decode_with(&bytes, Some(enc)),
            (text.to_string(), enc.name()),
            "{name}"
        );
    }
}

#[test]
fn the_bom_of_the_forced_encoding_is_removed() {
    let mut le = vec![0xFF, 0xFE];
    le.extend(utf16(JP, true));
    let mut be = vec![0xFE, 0xFF];
    be.extend(utf16(JP, false));
    let mut u8bom = vec![0xEF, 0xBB, 0xBF];
    u8bom.extend_from_slice(JP.as_bytes());
    for (name, bytes) in [("UTF-16LE", le), ("UTF-16BE", be), ("UTF-8", u8bom)] {
        assert_eq!(decode_with(&bytes, Some(by_name(name))).0, JP, "{name}");
    }
}

#[test]
fn a_bom_of_another_encoding_is_decoded_as_data() {
    // FF FE is not a Shift_JIS BOM: it must not vanish silently.
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(b"abc");
    let (text, name) = decode_with(&bytes, Some(encoding_rs::SHIFT_JIS));
    assert_eq!(name, "Shift_JIS");
    assert!(text.ends_with("abc"), "{text:?}");
    assert!(
        text.chars().count() > 3,
        "the two bytes are shown, not dropped: {text:?}"
    );
}

#[test]
fn forcing_the_wrong_encoding_marks_bad_bytes_and_never_panics() {
    let sjis = encode("shift_jis", JP);
    let (text, name) = decode_with(&sjis, Some(encoding_rs::UTF_8));
    assert_eq!(name, "UTF-8");
    assert!(text.contains('\u{FFFD}'));
    let cut = &JP.as_bytes()[..JP.len() - 1];
    let garbage: Vec<u8> = (0..=255u8).cycle().take(1001).collect();
    for name in CYCLE {
        let enc = by_name(name);
        assert_eq!(
            decode_with(b"", Some(enc)),
            (String::new(), enc.name()),
            "{name}"
        );
        for bytes in [&[0x41_u8, 0x00, 0x42][..], cut, &garbage] {
            let (_, got) = decode_with(bytes, Some(enc));
            assert_eq!(got, enc.name());
        }
    }
}

#[test]
fn no_forced_encoding_is_the_same_as_decode() {
    let mut le = vec![0xFF, 0xFE];
    le.extend(utf16(JP, true));
    for bytes in [
        JP.as_bytes().to_vec(),
        encode("shift_jis", JP),
        encode("euc-jp", JP),
        le,
        b"plain".to_vec(),
        Vec::new(),
    ] {
        assert_eq!(decode_with(&bytes, None), decode(&bytes));
    }
}
