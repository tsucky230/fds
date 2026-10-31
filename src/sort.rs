//! Listing order (design prompt §12 / R-302). Pure logic, OS independent.

use std::cmp::Ordering;

use crate::entry::{EntryKind, FileEntry};

/// Case-insensitive natural-order comparison (R-302-2, R-302-3).
///
/// Behavior:
/// - Split each string into chunks of "digits" and "non-digits" and compare chunk by chunk.
///   Digits are ASCII 0-9 only.
/// - Two non-digit chunks: compare as sequences of chars lowercased with `char::to_lowercase`.
/// - Two digit chunks: compare numerically, with no length limit (may exceed u64).
///   Compare the digit count after stripping leading zeros, then lexicographically.
///   Equal values (e.g. "01" and "1") are treated as equal at this stage.
/// - A digit chunk vs a non-digit chunk: the digit chunk is smaller.
/// - The side that runs out of chunks first is smaller.
/// - If everything above is equal, compare the original strings as bytes (`a.cmp(b)`)
///   to make it a total order. So the result is `Equal` only when `a == b`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a_chunks = chunks(a);
    let b_chunks = chunks(b);
    let mut a_iter = a_chunks.iter();
    let mut b_iter = b_chunks.iter();

    loop {
        match (a_iter.next(), b_iter.next()) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                let ord = match (ca, cb) {
                    (Chunk::Digits(da), Chunk::Digits(db)) => digit_cmp(da, db),
                    (Chunk::NonDigits(na), Chunk::NonDigits(nb)) => non_digit_cmp(na, nb),
                    (Chunk::Digits(_), Chunk::NonDigits(_)) => Ordering::Less,
                    (Chunk::NonDigits(_), Chunk::Digits(_)) => Ordering::Greater,
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }

    a.as_bytes().cmp(b.as_bytes())
}

/// Sorts entries (R-302-1).
///
/// Behavior:
/// - Group order: 1. ParentLink (`[..]`) 2. `EntryKind::is_dir_like()` is true 3. everything else.
/// - Within a group, compare `display_name` with `natural_cmp`.
/// - If `display_name`s are equal (lossy-conversion collision), compare `name` (OsString).
/// - `marked` and `hidden` do not affect the order.
pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        let rank = |e: &FileEntry| -> u8 {
            if e.kind == EntryKind::ParentLink {
                0
            } else if e.kind.is_dir_like() {
                1
            } else {
                2
            }
        };

        match rank(a).cmp(&rank(b)) {
            Ordering::Equal => match natural_cmp(&a.display_name, &b.display_name) {
                Ordering::Equal => a.name.cmp(&b.name),
                other => other,
            },
            other => other,
        }
    });
}

/// One chunk of a string used for natural ordering.
#[derive(Debug, Clone, Copy)]
enum Chunk<'a> {
    Digits(&'a str),
    NonDigits(&'a str),
}

/// Split `s` into alternating digit / non-digit chunks. Digits are ASCII `0-9` only.
fn chunks(s: &str) -> Vec<Chunk<'_>> {
    let mut result = Vec::new();
    let mut chars = s.char_indices().peekable();

    while let Some((start, c)) = chars.next() {
        let is_digit = c.is_ascii_digit();
        let mut end = start + c.len_utf8();

        while let Some(&(idx, c2)) = chars.peek() {
            if c2.is_ascii_digit() == is_digit {
                end = idx + c2.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let slice = &s[start..end];
        result.push(if is_digit {
            Chunk::Digits(slice)
        } else {
            Chunk::NonDigits(slice)
        });
    }

    result
}

/// Compare two digit chunks numerically, with no artificial length limit.
fn digit_cmp(a: &str, b: &str) -> Ordering {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    // Position of the first non-zero digit, or the end if the chunk is all zeros.
    let a_pos = a_bytes
        .iter()
        .position(|&b| b != b'0')
        .unwrap_or(a_bytes.len());
    let b_pos = b_bytes
        .iter()
        .position(|&b| b != b'0')
        .unwrap_or(b_bytes.len());

    let a_len = a_bytes.len() - a_pos;
    let b_len = b_bytes.len() - b_pos;

    match a_len.cmp(&b_len) {
        Ordering::Equal if a_len == 0 => Ordering::Equal,
        Ordering::Equal => a_bytes[a_pos..].cmp(&b_bytes[b_pos..]),
        other => other,
    }
}

/// Compare two non-digit chunks case-insensitively using `char::to_lowercase`.
fn non_digit_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().flat_map(char::to_lowercase);
    let mut b_chars = b.chars().flat_map(char::to_lowercase);

    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                let ord = ca.cmp(&cb);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}
