//! File name validation and unique-name generation (P3-7, P3-12). Pure logic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    /// "." or "..".
    Dot,
    /// Contains a path separator or NUL.
    Separator,
    /// Windows only: one of `<>:"|?*` or a control character (0x01-0x1F).
    InvalidChar,
    /// Windows only: ends with a space or a period.
    TrailingDotOrSpace,
    /// Windows only: a reserved device name (CON, PRN, AUX, NUL, COM1-9, LPT1-9), with or without
    /// an extension, case-insensitive ("con.txt", "Com1").
    Reserved,
}

/// Validates a single file name (not a path) for Rename and mkdir.
///
/// Behavior:
/// - Empty -> Empty. "." or ".." -> Dot.
/// - '/' or NUL -> Separator. When `windows` is true, '\\' is also Separator.
/// - When `windows` is true, additionally: InvalidChar, TrailingDotOrSpace, Reserved (checked in
///   this order after the checks above).
/// - Otherwise Ok.
pub fn validate(name: &str, windows: bool) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if matches!(name, "." | "..") {
        return Err(NameError::Dot);
    }
    if name.contains(['/', '\0']) || (windows && name.contains('\\')) {
        return Err(NameError::Separator);
    }
    if windows {
        if name
            .chars()
            .any(|c| "<>:\"|?*".contains(c) || ('\u{1}'..='\u{1f}').contains(&c))
        {
            return Err(NameError::InvalidChar);
        }
        if name.ends_with(['.', ' ']) {
            return Err(NameError::TrailingDotOrSpace);
        }
        let base = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ["COM", "LPT"].iter().any(|prefix| {
                base.strip_prefix(prefix).is_some_and(|n| {
                    matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
                })
            })
        {
            return Err(NameError::Reserved);
        }
    }
    Ok(())
}

/// Returns `name` if `exists(name)` is false, otherwise the first free "stem (n).ext" for
/// n = 2, 3, ... (P3-7).
///
/// Behavior:
/// - The extension is the part after the last '.', unless that '.' is the first character
///   (".bashrc" has no extension -> ".bashrc (2)"). "archive.tar.gz" -> "archive.tar (2).gz".
/// - A name without extension: "dir" -> "dir (2)".
/// - Never loops forever: gives up after n = 9999 and returns "stem (9999).ext" even if taken
///   (the caller then gets a normal conflict/error).
pub fn unique_name(name: &str, exists: &dyn Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.').filter(|&i| i > 0) {
        Some(i) => (
            name.get(..i).unwrap_or(name),
            name.get(i..).unwrap_or_default(),
        ),
        None => (name, ""),
    };
    for n in 2..=9999 {
        let candidate = format!("{stem} ({n}){ext}");
        if !exists(&candidate) || n == 9999 {
            return candidate;
        }
    }
    name.to_string()
}

/// True when `old` and `new` differ only in letter case (and are not identical).
/// Uses Unicode lowercase comparison: "Foo.TXT" vs "foo.txt" -> true; "a" vs "a" -> false.
pub fn is_case_only_change(old: &str, new: &str) -> bool {
    old != new && old.to_lowercase() == new.to_lowercase()
}
