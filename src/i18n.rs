//! UI text catalogs (R-1600). Every user-visible string comes from a language file.
//!
//! - Built-in languages (`lang/*.toml`) are embedded in the binary.
//! - A user file `<config dir>/lang/<code>.toml` adds a language or overrides built-in keys.
//! - Missing keys fall back to English, then to the key itself (never panic, never blank).
//!
//! Language file format (TOML). Nested tables are flattened with dots, so these are equivalent:
//!
//! ```toml
//! [status]
//! counts = "{files} files   {dirs} dirs"
//! ```
//! ```toml
//! "status.counts" = "{files} files   {dirs} dirs"
//! ```

use std::collections::HashMap;

/// Built-in languages: (code, file content).
pub const BUILTIN: &[(&str, &str)] = &[
    ("en", include_str!("../lang/en.toml")),
    ("ja", include_str!("../lang/ja.toml")),
];

/// Language used when nothing else matches, and the source of fallback text.
pub const FALLBACK_LANGUAGE: &str = "en";

/// Parses a language file into a flat `key -> text` map.
///
/// Behavior:
/// - Parse `source` as TOML. On a syntax error return `Err` with a short message.
/// - Flatten nested tables by joining keys with `.` (e.g. `[status] counts` -> `status.counts`).
/// - Keep only string values. Non-string values (numbers, arrays, ...) are ignored.
pub fn parse_catalog(source: &str) -> Result<HashMap<String, String>, String> {
    let table = source.parse::<toml::Table>().map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    flatten_table("", &table, &mut out);
    Ok(out)
}

fn flatten_table(prefix: &str, table: &toml::Table, out: &mut HashMap<String, String>) {
    for (key, value) in table {
        let full_key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };

        match value {
            toml::Value::String(s) => {
                out.insert(full_key, s.clone());
            }
            toml::Value::Table(t) => {
                flatten_table(&full_key, t, out);
            }
            _ => {}
        }
    }
}

/// Replaces `{name}` placeholders in `template` with values from `args`.
///
/// Behavior:
/// - Each `{name}` whose name is in `args` is replaced with the value (first match wins).
/// - `{{` and `}}` produce literal `{` and `}`.
/// - Unknown placeholders and unmatched braces are left as they are.
/// - Values are inserted verbatim (a value containing `{x}` is not expanded again).
pub fn format_template(template: &str, args: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut i = 0;
    let chars: Vec<(usize, char)> = template.char_indices().collect();

    while i < chars.len() {
        let (pos, ch) = chars[i];

        if ch == '{' {
            // Literal `{{` -> `{`
            if i + 1 < chars.len() && chars[i + 1].1 == '{' {
                result.push('{');
                i += 2;
                continue;
            }

            // Find closing `}`
            if let Some((j, _)) = chars
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, c)| c.1 == '}')
            {
                let name = &template[pos + 1..chars[j].0];
                if let Some((_, value)) = args.iter().find(|(k, _)| *k == name) {
                    result.push_str(value);
                } else {
                    // Unknown placeholder: leave it unchanged.
                    result.push_str(&template[pos..chars[j].0 + 1]);
                }
                i = j + 1;
                continue;
            }
        }

        if ch == '}' {
            // Literal `}}` -> `}`
            if i + 1 < chars.len() && chars[i + 1].1 == '}' {
                result.push('}');
                i += 2;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

    result
}

/// Normalizes a locale string to a language code used for file lookup.
///
/// Behavior:
/// - Strip an encoding (`.UTF-8`) and modifier (`@euro`) suffix.
/// - Treat `_` and `-` alike; lowercase everything. `ja_JP.UTF-8` -> `ja-jp`, `en-US` -> `en-us`.
/// - `C`, `POSIX` and the empty string return None.
pub fn normalize_locale(locale: &str) -> Option<String> {
    let trimmed = locale.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strip encoding or modifier suffixes.
    let without_suffix = trimmed
        .split_once('.')
        .map(|(left, _)| left)
        .unwrap_or(trimmed);
    let without_modifier = without_suffix
        .split_once('@')
        .map(|(left, _)| left)
        .unwrap_or(without_suffix);

    let normalized = without_modifier.replace('_', "-").to_lowercase();

    if normalized.is_empty() || normalized == "c" || normalized == "posix" {
        None
    } else {
        Some(normalized)
    }
}

/// Chooses the display language (R-1600-2).
///
/// Behavior:
/// - Candidates in priority order: `configured`, then `os_locale`. Each is normalized with
///   `normalize_locale`; invalid ones are skipped.
/// - For each candidate, return the full code if it is in `available` (e.g. `pt-br`), otherwise the
///   part before `-` if that is available (e.g. `ja-jp` -> `ja`).
/// - If nothing matches, return `FALLBACK_LANGUAGE`.
pub fn resolve_language(
    configured: Option<&str>,
    os_locale: Option<&str>,
    available: &[String],
) -> String {
    let candidates: Vec<Option<&str>> = vec![configured, os_locale];

    for candidate in candidates.into_iter().flatten() {
        if let Some(code) = normalize_locale(candidate) {
            if available.contains(&code) {
                return code;
            }

            if let Some((base, _)) = code.split_once('-')
                && available.iter().any(|a| a == base)
            {
                return base.to_string();
            }
        }
    }

    FALLBACK_LANGUAGE.to_string()
}

/// Text lookup for one language, with English fallback.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    language: String,
    texts: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl Catalog {
    /// Builds a catalog from already-parsed maps. `texts` wins over `fallback`.
    pub fn new(
        language: impl Into<String>,
        texts: HashMap<String, String>,
        fallback: HashMap<String, String>,
    ) -> Self {
        Self {
            language: language.into(),
            texts,
            fallback,
        }
    }

    /// English built-in catalog only. Used by tests and as a last resort.
    pub fn english() -> Self {
        let en = builtin(FALLBACK_LANGUAGE)
            .and_then(|s| parse_catalog(s).ok())
            .unwrap_or_default();
        Self::new(FALLBACK_LANGUAGE, en.clone(), en)
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Looks up `key` and fills placeholders. Order: this language, English, then the key itself.
    pub fn text(&self, key: &str, args: &[(&str, &str)]) -> String {
        let template = self
            .texts
            .get(key)
            .or_else(|| self.fallback.get(key))
            .map(String::as_str)
            .unwrap_or(key);
        format_template(template, args)
    }
}

/// Built-in file content for a language code.
pub fn builtin(code: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|(c, _)| *c == code).map(|(_, s)| *s)
}
