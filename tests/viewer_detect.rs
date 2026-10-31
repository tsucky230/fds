//! File type detection (P2-4).

use std::ffi::OsStr;

use fds::viewer::detect::{FileClass, ImageFormat, Sniff, classify, is_binary};

fn sniff(head: &[u8], ext: Option<&str>) -> Sniff {
    Sniff::new(head.to_vec(), head.len() as u64, ext.map(OsStr::new))
}

const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F'];

#[test]
fn images_by_magic_regardless_of_extension() {
    assert_eq!(
        classify(&sniff(PNG, Some("png"))),
        FileClass::Image(ImageFormat::Png)
    );
    assert_eq!(
        classify(&sniff(PNG, Some("txt"))),
        FileClass::Image(ImageFormat::Png)
    );
    assert_eq!(
        classify(&sniff(JPEG, None)),
        FileClass::Image(ImageFormat::Jpeg)
    );
    // Extension alone does not make an image.
    assert_eq!(
        classify(&sniff(b"not really", Some("png"))),
        FileClass::Text
    );
}

#[test]
fn markdown_by_extension_case_insensitive() {
    assert_eq!(
        classify(&sniff(b"# Title\n", Some("md"))),
        FileClass::Markdown
    );
    assert_eq!(
        classify(&sniff(b"# Title\n", Some("MARKDOWN"))),
        FileClass::Markdown
    );
    assert_eq!(classify(&sniff(b"# Title\n", Some("txt"))), FileClass::Text);
}

#[test]
fn bom_means_text_even_with_nul_bytes() {
    // UTF-16LE "ab" contains NUL bytes but must still be text.
    let utf16 = [0xFF, 0xFE, b'a', 0, b'b', 0];
    assert_eq!(classify(&sniff(&utf16, None)), FileClass::Text);
    assert_eq!(classify(&sniff(&utf16, Some("md"))), FileClass::Markdown);
    let utf16be = [0xFE, 0xFF, 0, b'a'];
    assert_eq!(classify(&sniff(&utf16be, None)), FileClass::Text);
    assert_eq!(
        classify(&sniff(&[0xEF, 0xBB, 0xBF, b'x'], None)),
        FileClass::Text
    );
}

#[test]
fn binary_detection() {
    assert_eq!(
        classify(&sniff(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0], None)),
        FileClass::Binary
    );
    assert_eq!(classify(&sniff(b"", None)), FileClass::Text);
    assert!(!is_binary(b""));
    assert!(is_binary(b"abc\0def"));
    assert!(!is_binary(b"line1\r\n\tline2\x1b[0m\x0c"));
    // Legacy-encoded Japanese (Shift_JIS bytes >= 0x80) is not binary.
    assert!(!is_binary(&[0x93, 0xFA, 0x96, 0x7B, 0x8C, 0xEA]));
    // 1 suspicious byte in 10 = 10% -> not more than 10% -> text.
    assert!(!is_binary(b"\x01abcdefghi"));
    // 2 in 10 -> binary.
    assert!(is_binary(b"\x01\x02abcdefgh"));
}
