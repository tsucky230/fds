//! File type detection from a bounded head of the file (R-503, P2-4). Pure logic.

use std::ffi::OsStr;

/// The first bytes of a file plus what we know without reading more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sniff {
    /// At most `SNIFF_BYTES` from the start of the file.
    pub head: Vec<u8>,
    /// Size of the whole file.
    pub len: u64,
    /// Lowercased extension without the dot, if any (e.g. "md").
    pub extension: Option<String>,
}

impl Sniff {
    pub fn new(head: Vec<u8>, len: u64, extension: Option<&OsStr>) -> Self {
        let extension = extension.and_then(|e| e.to_str()).map(str::to_lowercase);
        Self {
            head,
            len,
            extension,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// What kind of content the file looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    Image(ImageFormat),
    Markdown,
    Text,
    Binary,
}

/// Classifies a file.
///
/// Behavior, in this order:
/// 1. `head` starts with the PNG signature `89 50 4E 47 0D 0A 1A 0A` -> Image(Png),
///    or with `FF D8 FF` -> Image(Jpeg). The extension is ignored for images.
/// 2. `head` starts with a UTF-16 BOM (`FF FE` or `FE FF`) or UTF-8 BOM (`EF BB BF`):
///    Markdown if the extension is "md" or "markdown", otherwise Text.
/// 3. `is_binary(head)` -> Binary.
/// 4. Extension "md" or "markdown" -> Markdown.
/// 5. Otherwise Text (an empty file is Text).
pub fn classify(sniff: &Sniff) -> FileClass {
    let head = &sniff.head;

    if head.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return FileClass::Image(ImageFormat::Png);
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return FileClass::Image(ImageFormat::Jpeg);
    }

    if head.starts_with(&[0xFF, 0xFE])
        || head.starts_with(&[0xFE, 0xFF])
        || head.starts_with(&[0xEF, 0xBB, 0xBF])
    {
        return if is_markdown_extension(sniff.extension.as_deref()) {
            FileClass::Markdown
        } else {
            FileClass::Text
        };
    }

    if is_binary(head) {
        return FileClass::Binary;
    }

    if is_markdown_extension(sniff.extension.as_deref()) {
        return FileClass::Markdown;
    }

    FileClass::Text
}

/// Heuristic binary check on a head of bytes.
///
/// Behavior:
/// - Empty -> false.
/// - Contains a NUL byte (0x00) -> true.
/// - Otherwise count "suspicious" bytes: 0x01-0x08, 0x0E-0x1A, 0x1C-0x1F and 0x7F
///   (TAB 0x09, LF 0x0A, VT 0x0B, FF 0x0C, CR 0x0D and ESC 0x1B are allowed).
///   true if suspicious bytes are more than 10% of the length.
/// - Bytes >= 0x80 are not suspicious (they may be UTF-8 or legacy encodings).
pub fn is_binary(head: &[u8]) -> bool {
    if head.is_empty() {
        return false;
    }

    let mut suspicious = 0u64;
    for &byte in head {
        if byte == 0x00 {
            return true;
        }
        if is_suspicious_byte(byte) {
            suspicious += 1;
        }
    }

    // "More than 10%" is equivalent to suspicious * 10 > len.
    suspicious * 10 > head.len() as u64
}

fn is_markdown_extension(extension: Option<&str>) -> bool {
    matches!(extension, Some("md") | Some("markdown"))
}

fn is_suspicious_byte(byte: u8) -> bool {
    matches!(
        byte,
        0x01..=0x08 | 0x0E..=0x1A | 0x1C..=0x1F | 0x7F
    )
}
