//! Text encoding detection and decoding (P2-5). Pure logic on byte slices.
//!
//! Uses `encoding_rs` for decoding and `chardetng` for guessing legacy encodings.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;

/// Picks the encoding of `bytes`.
///
/// Behavior, in this order:
/// 1. BOM: `EF BB BF` -> UTF-8, `FF FE` -> UTF-16LE, `FE FF` -> UTF-16BE.
/// 2. ISO-2022-JP escape sequences anywhere in `bytes`: `ESC $ B`, `ESC $ @`, `ESC ( J` or
///    `ESC ( I` -> ISO-2022-JP. (Checked before UTF-8 because ISO-2022-JP is 7-bit and would
///    otherwise pass as valid UTF-8.)
/// 3. Valid UTF-8 (`std::str::from_utf8` succeeds, allowing a truncated final sequence of at most
///    3 bytes at the very end, since the text may be cut at `TEXT_LIMIT`) -> UTF-8.
/// 4. Otherwise ask `chardetng::EncodingDetector` (ISO-2022-JP allowed, feed all bytes with
///    `last = true`, `guess(None, Utf8Detection::Allow)`).
pub fn detect(bytes: &[u8]) -> &'static Encoding {
    // 1. BOM
    if let Some((enc, _)) = Encoding::for_bom(bytes) {
        return enc;
    }

    // 2. ISO-2022-JP escape sequences
    if has_iso2022_escapes(bytes) {
        return encoding_rs::ISO_2022_JP;
    }

    // 3. Valid UTF-8, allowing a truncated final multibyte sequence
    if is_valid_utf8_or_truncated(bytes) {
        return encoding_rs::UTF_8;
    }

    // 4. Ask chardetng
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(bytes, true);
    detector.guess(None, Utf8Detection::Allow)
}

/// Decodes `bytes` with the detected encoding.
///
/// Returns (text, encoding name). The BOM is removed. Invalid sequences become U+FFFD; never panics.
/// The name is `Encoding::name()` (e.g. "UTF-8", "Shift_JIS", "EUC-JP", "ISO-2022-JP", "UTF-16LE").
pub fn decode(bytes: &[u8]) -> (String, &'static str) {
    let (enc, stripped) = if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        (enc, bytes.get(bom_len..).unwrap_or(&[]))
    } else {
        (detect(bytes), bytes)
    };

    let (cow, _) = enc.decode_without_bom_handling(stripped);
    (cow.into_owned(), enc.name())
}

/// What the viewer's manual switch (`E`) moves to after `current` (ADR 0003, M1-b).
///
/// The cycle is: None (detect automatically) -> UTF-8 -> Shift_JIS -> EUC-JP -> ISO-2022-JP
/// -> UTF-16LE -> UTF-16BE -> windows-1252 -> None.
/// An encoding that is not in the cycle (e.g. GBK) moves to the first entry, UTF-8.
pub fn next_choice(current: Option<&'static Encoding>) -> Option<&'static Encoding> {
    match current {
        None => Some(encoding_rs::UTF_8),
        Some(enc) if enc == encoding_rs::UTF_8 => Some(encoding_rs::SHIFT_JIS),
        Some(enc) if enc == encoding_rs::SHIFT_JIS => Some(encoding_rs::EUC_JP),
        Some(enc) if enc == encoding_rs::EUC_JP => Some(encoding_rs::ISO_2022_JP),
        Some(enc) if enc == encoding_rs::ISO_2022_JP => Some(encoding_rs::UTF_16LE),
        Some(enc) if enc == encoding_rs::UTF_16LE => Some(encoding_rs::UTF_16BE),
        Some(enc) if enc == encoding_rs::UTF_16BE => Some(encoding_rs::WINDOWS_1252),
        Some(enc) if enc == encoding_rs::WINDOWS_1252 => None,
        Some(_) => Some(encoding_rs::UTF_8),
    }
}

/// Decodes `bytes` with `forced`, or exactly like `decode` when `forced` is None.
///
/// With `forced`:
/// - A BOM is removed only when it is the BOM of `forced` (EF BB BF for UTF-8, FF FE for
///   UTF-16LE, FE FF for UTF-16BE). Any other leading bytes are decoded as data.
/// - Invalid or incomplete sequences (odd-length UTF-16, a cut multibyte character at the end)
///   become U+FFFD. Never panics.
/// - Returns (text, `forced.name()`).
pub fn decode_with(bytes: &[u8], forced: Option<&'static Encoding>) -> (String, &'static str) {
    let Some(enc) = forced else {
        return decode(bytes);
    };
    let stripped = match Encoding::for_bom(bytes) {
        Some((bom_enc, bom_len)) if bom_enc == enc => bytes.get(bom_len..).unwrap_or(&[]),
        _ => bytes,
    };
    let (cow, _) = enc.decode_without_bom_handling(stripped);
    (cow.into_owned(), enc.name())
}

/// Returns true if `bytes` is valid UTF-8 except possibly for an incomplete final multibyte
/// sequence of at most 3 bytes.
fn is_valid_utf8_or_truncated(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && bytes.len().saturating_sub(e.valid_up_to()) <= 3,
    }
}

/// Returns true if `bytes` contains an ISO-2022-JP designation/invocation escape sequence.
fn has_iso2022_escapes(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|w| {
        w == [0x1B, 0x24, 0x42]
            || w == [0x1B, 0x24, 0x40]
            || w == [0x1B, 0x28, 0x4A]
            || w == [0x1B, 0x28, 0x49]
    })
}
