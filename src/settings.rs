//! Config file and language catalog loading (R-1100, R-1600). OS-dependent paths live here.
//!
//! Phase 1 reads only `[ui] language`. A missing or broken config file is not an error:
//! FDS starts with defaults (Graceful Degradation, R-007).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::i18n::{self, BUILTIN, Catalog, FALLBACK_LANGUAGE};

#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub viewer: ViewerSettings,
    #[serde(default)]
    pub editor: EditorSettings,
    #[serde(default)]
    pub ops: OpsSettings,
    #[serde(default)]
    pub packs: PacksSettings,
    #[serde(default)]
    pub providers: ProvidersSettings,
    #[serde(default)]
    pub open: OpenSettings,
}

/// `[open.by_extension]` (R-110-4): `pdf = "evince"` or `pdf = { command = "evince", wait = false }`.
#[derive(Debug, Default, Deserialize)]
pub struct OpenSettings {
    #[serde(default)]
    pub by_extension: std::collections::HashMap<String, OpenRule>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum OpenRule {
    Command(String),
    Detailed {
        command: String,
        /// Default true: FDS gives the application the terminal and waits, like the editor.
        #[serde(default = "default_wait")]
        wait: bool,
    },
}

fn default_wait() -> bool {
    true
}

/// `[packs]` (Phase 4): `disabled = ["name or folder", ...]` packs are not loaded (C38).
#[derive(Debug, Default, Deserialize)]
pub struct PacksSettings {
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// `[providers]` (Phase 4): `timeout_secs` for external commands
/// (see `viewer::external::timeout_from_setting`).
#[derive(Debug, Default, Deserialize)]
pub struct ProvidersSettings {
    pub timeout_secs: Option<u64>,
}

/// `[ops]` (M1-a). `delete` is kept as a raw TOML value so that a wrong type does not make the
/// whole config file invalid; `delete_mode` interprets it.
#[derive(Debug, Default, Deserialize)]
pub struct OpsSettings {
    pub delete: Option<toml::Value>,
}

/// Interprets `[ops] delete` (R-402-3, M1-a).
///
/// - Missing -> `(Permanent, None)`: permanent deletion stays the default (R-402-2).
/// - The strings "permanent" / "trash", ASCII case-insensitive, surrounding spaces ignored ->
///   that mode, None.
/// - Anything else (another string, "", a number, a boolean, an array, a table) ->
///   `(Trash, Some(value as written in TOML, e.g. "\"trsh\"" or "1"))`: an unclear setting must
///   not lead to permanent deletion; the caller reports the value at startup.
pub fn delete_mode(value: Option<&toml::Value>) -> (crate::ops::DeleteMode, Option<String>) {
    use crate::ops::DeleteMode;

    match value {
        None => (DeleteMode::Permanent, None),
        Some(toml::Value::String(setting)) if setting.trim().eq_ignore_ascii_case("permanent") => {
            (DeleteMode::Permanent, None)
        }
        Some(toml::Value::String(setting)) if setting.trim().eq_ignore_ascii_case("trash") => {
            (DeleteMode::Trash, None)
        }
        Some(value) => (DeleteMode::Trash, Some(value.to_string())),
    }
}

/// `[editor]` (P3-15): `command = "nvim"` and `[editor.by_extension] md = "code --wait"`.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct EditorSettings {
    pub command: Option<String>,
    #[serde(default)]
    pub by_extension: std::collections::HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ViewerSettings {
    /// "auto" (default), "kitty", "sixel", "iterm2", "halfblocks" or "none" (P2-11).
    pub image_protocol: Option<String>,
    /// vi-style keys in the viewer: h / l scroll sideways instead of h closing (R-500-6).
    /// None means the default (off).
    pub vi_keys: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UiSettings {
    /// Language code such as "ja" or "pt-BR". None means "follow the OS".
    pub language: Option<String>,
    /// Draw a frame around the whole screen. None means the default (on).
    pub frame: Option<bool>,
    /// Show the preview pane right of the list (R-200-9). None means the default (on).
    pub preview: Option<bool>,
    /// Draw pictures in the preview pane (R-200-10). None means the default (on).
    pub preview_images: Option<bool>,
}

/// FDS config directory (R-1100).
/// Windows: `%APPDATA%\fds`. Others (incl. macOS): `$XDG_CONFIG_HOME/fds`, else `~/.config/fds`.
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("fds"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("fds"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("fds"))
}

/// Reads `<dir>/config.toml`. Missing or invalid files yield defaults.
pub fn load_settings(dir: Option<&Path>) -> Settings {
    dir.and_then(|d| fs::read_to_string(d.join("config.toml")).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Builds the catalog for the resolved language (R-1600-2, R-1600-3).
///
/// Each language = built-in file (if any) overlaid with `<dir>/lang/<code>.toml` (if any).
/// Unreadable or invalid user files are ignored.
pub fn load_catalog(settings: &Settings, dir: Option<&Path>) -> Catalog {
    let user_dir = dir.map(|d| d.join("lang"));
    let mut available: Vec<String> = BUILTIN.iter().map(|(code, _)| code.to_string()).collect();
    available.extend(user_languages(user_dir.as_deref()));

    let os_locale = sys_locale::get_locale();
    let language = i18n::resolve_language(
        settings.ui.language.as_deref(),
        os_locale.as_deref(),
        &available,
    );

    let fallback = language_texts(FALLBACK_LANGUAGE, user_dir.as_deref());
    let texts = language_texts(&language, user_dir.as_deref());
    Catalog::new(language, texts, fallback)
}

/// Language codes of user files (`<code>.toml`, lowercased).
fn user_languages(user_dir: Option<&Path>) -> Vec<String> {
    let Some(entries) = user_dir.and_then(|d| fs::read_dir(d).ok()) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase)
        })
        .collect()
}

fn language_texts(code: &str, user_dir: Option<&Path>) -> HashMap<String, String> {
    let mut texts = i18n::builtin(code)
        .and_then(|s| i18n::parse_catalog(s).ok())
        .unwrap_or_default();
    if let Some(user) = user_dir.and_then(|d| find_user_file(d, code))
        && let Ok(overrides) = fs::read_to_string(user)
            .map_err(|e| e.to_string())
            .and_then(|s| i18n::parse_catalog(&s))
    {
        texts.extend(overrides);
    }
    texts
}

/// Finds `<code>.toml` in `dir`, ignoring case of the file stem.
fn find_user_file(dir: &Path, code: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|x| x == "toml")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(code))
        })
}
