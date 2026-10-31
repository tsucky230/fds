//! Hex viewer: row formatting (pure) and on-demand reading (P2-10, R-506).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes read per cache window.
pub const WINDOW_BYTES: usize = 64 * 1024;

/// Bytes per row for a given terminal width: 16 when `width >= 78`, otherwise 8.
pub fn bytes_per_row(width: u16) -> usize {
    if width >= 78 { 16 } else { 8 }
}

/// Hex digits used for offsets: 8 for files up to 4 GiB (`len <= 0x1_0000_0000`), otherwise 16.
pub fn offset_digits(len: u64) -> usize {
    if len <= 0x1_0000_0000 { 8 } else { 16 }
}

/// Formats one row.
///
/// Layout: "<offset>  <hex bytes>  |<ascii>|"
/// - offset: uppercase hex, zero padded to `digits`.
/// - hex bytes: uppercase two-digit hex separated by one space, with one extra space after
///   every 8 bytes except at the end of the row. If `bytes.len() < per_row`, the missing bytes
///   are padded with spaces so the ASCII column stays aligned.
/// - ascii: printable ASCII (0x20..=0x7E) as is, anything else as '.'; only for present bytes
///   (no padding inside the bars).
///
/// Example (per_row 16, digits 8, bytes "Hello"):
/// "00000000  48 65 6C 6C 6F" + padding + "  |Hello|"
pub fn format_row(offset: u64, bytes: &[u8], per_row: usize, digits: usize) -> String {
    let mut out = String::new();

    out.push_str(&format!("{:0>width$X}", offset, width = digits));
    out.push_str("  ");

    for i in 0..per_row {
        if i > 0 {
            if i % 8 == 0 {
                out.push(' ');
            }
            out.push(' ');
        }

        match bytes.get(i) {
            Some(&b) => out.push_str(&format!("{:02X}", b)),
            None => out.push_str("  "),
        }
    }

    out.push_str("  |");

    for &b in bytes {
        if (0x20..=0x7E).contains(&b) {
            out.push(char::from(b));
        } else {
            out.push('.');
        }
    }

    out.push('|');
    out
}

/// Reads a file in windows on demand; never loads the whole file.
#[derive(Debug)]
pub struct HexSource {
    file: File,
    /// File length when opened.
    pub len: u64,
    /// Cached window: (start offset, bytes).
    window: Option<(u64, Vec<u8>)>,
}

impl HexSource {
    pub fn open(path: &Path, len: u64) -> std::io::Result<Self> {
        Ok(Self {
            file: File::open(path)?,
            len,
            window: None,
        })
    }

    /// Number of rows for `per_row` bytes per row (0 for an empty file).
    pub fn rows(&self, per_row: usize) -> u64 {
        if per_row == 0 {
            return 0;
        }
        self.len.div_ceil(per_row as u64)
    }

    /// Bytes of row `row`. Uses the cached window when it covers the row; otherwise reads a new
    /// window starting at the row. If the file shrank, returns what could be read (possibly
    /// empty). Read errors return an empty row; never panics.
    pub fn row(&mut self, row: u64, per_row: usize) -> Vec<u8> {
        let start = row.saturating_mul(per_row as u64);
        if start >= self.len {
            return Vec::new();
        }
        let end = start.saturating_add(per_row as u64).min(self.len);
        let covered = self
            .window
            .as_ref()
            .is_some_and(|(w, bytes)| *w <= start && end <= w.saturating_add(bytes.len() as u64));
        if !covered {
            self.window = self.read_window(start).map(|bytes| (start, bytes));
        }
        match &self.window {
            Some((w, bytes)) => {
                let from = usize::try_from(start - w)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                let to = usize::try_from(end - w)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                bytes[from..to].to_vec()
            }
            None => Vec::new(),
        }
    }

    fn read_window(&mut self, start: u64) -> Option<Vec<u8>> {
        self.file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = Vec::with_capacity(WINDOW_BYTES);
        (&mut self.file)
            .take(WINDOW_BYTES as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        Some(bytes)
    }
}
