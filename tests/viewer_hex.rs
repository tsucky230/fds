//! Hex formatting and on-demand reading (P2-10).

use std::fs;
use std::io::Write;

use fds::viewer::hex::{HexSource, bytes_per_row, format_row, offset_digits};

#[test]
fn row_width_and_offset_digits() {
    assert_eq!(bytes_per_row(78), 16);
    assert_eq!(bytes_per_row(200), 16);
    assert_eq!(bytes_per_row(77), 8);
    assert_eq!(bytes_per_row(0), 8);
    assert_eq!(offset_digits(0), 8);
    assert_eq!(offset_digits(0x1_0000_0000), 8);
    assert_eq!(offset_digits(0x1_0000_0001), 16);
}

#[test]
fn full_row_layout() {
    let bytes: Vec<u8> = (0x41..0x51).collect(); // "ABCDEFGHIJKLMNOP"
    assert_eq!(
        format_row(0x10, &bytes, 16, 8),
        "00000010  41 42 43 44 45 46 47 48  49 4A 4B 4C 4D 4E 4F 50  |ABCDEFGHIJKLMNOP|"
    );
}

#[test]
fn short_row_is_padded_and_nonprintables_are_dots() {
    let row = format_row(0, b"Hi\x00\xff\n", 16, 8);
    let full = format_row(0, &[0u8; 16], 16, 8);
    assert!(row.starts_with("00000000  48 69 00 FF 0A "));
    assert!(row.ends_with("  |Hi...|"));
    // The ASCII column starts at the same position as in a full row.
    assert_eq!(row.find('|'), full.find('|'));
}

#[test]
fn eight_byte_rows_and_wide_offsets() {
    assert_eq!(
        format_row(8, b"abcdefgh", 8, 8),
        "00000008  61 62 63 64 65 66 67 68  |abcdefgh|"
    );
    assert!(format_row(0x1_0000_0000, b"a", 16, 16).starts_with("0000000100000000  61"));
}

#[test]
fn source_reads_rows_on_demand_including_the_end_of_large_files() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("big.bin");
    let len: u64 = 20 * 1024 * 1024 + 5;
    {
        let f = fs::File::create(&path).unwrap();
        f.set_len(len - 5).unwrap(); // sparse zeros
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"TAIL!").unwrap();
    }
    let mut src = HexSource::open(&path, len).unwrap();
    assert_eq!(src.rows(16), len.div_ceil(16));
    let last = src.rows(16) - 1;
    assert_eq!(src.row(last, 16), b"TAIL!");
    assert_eq!(src.row(0, 16), vec![0u8; 16]);
    assert!(src.row(last + 1, 16).is_empty());
}

#[test]
fn empty_and_shrunk_files_do_not_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("f.bin");
    fs::write(&path, b"").unwrap();
    let mut src = HexSource::open(&path, 0).unwrap();
    assert_eq!(src.rows(16), 0);
    assert!(src.row(0, 16).is_empty());

    fs::write(&path, vec![1u8; 100]).unwrap();
    let mut src = HexSource::open(&path, 100).unwrap();
    fs::write(&path, vec![1u8; 10]).unwrap(); // shrinks after opening
    assert_eq!(src.row(0, 16).len(), 10);
    assert!(src.row(5, 16).is_empty());
}
