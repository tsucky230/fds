//! Packs: folders that add external command providers (Phase 4, R-800 to R-803, ADR 0006).
//!
//! No dynamic libraries (R-801). A pack declares commands; FDS runs them without a shell and
//! shows their output (see `viewer::external`). Community packs load without a license (R-803);
//! official packs need a valid signature and a license that allows their pack id.
//!
//! Layout: `<config dir>/packs/<folder>/pack.toml` (+ `pack.toml.sig` for official packs).
//!
//! ```toml
//! name = "pdf-text"            # required, [A-Za-z0-9._-], 1..=64 chars
//! version = "1.0.0"            # required, non-empty
//! official = false             # optional, default false
//! pack_id = "viewer"           # required when official, one of license::KNOWN_PACKS
//!
//! [[provider]]                 # at least one
//! id = "pdf-text"              # required, [A-Za-z0-9._-], unique in the pack
//! label = "PDF (text)"         # required, 1..=40 chars, no control characters; shown in the header
//! extensions = ["pdf"]         # required, at least one; see `valid_extension`
//! command = ["pdftotext", "-layout", "{path}", "-"]   # required, see below
//! ```
//!
//! `command`: at least one element; the first (the program) is non-empty and is not `{path}`;
//! at least one element is exactly `{path}`. A program with a path separator that is relative
//! is relative to the pack folder (packs may ship their own tool); a bare name is looked up in
//! PATH. Unknown keys are ignored.
//!
//! Signature file `pack.toml.sig` (same keys as licenses):
//!
//! ```text
//! fds-pack-signature-v1
//! key: <key id>
//! signature: <base64 of the Ed25519 signature over the exact bytes of pack.toml>
//! ```

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, VerifyingKey};

use crate::license::{self, KNOWN_PACKS, LicenseStatus};

pub const PACKS_DIR: &str = "packs";
pub const MANIFEST_FILE: &str = "pack.toml";
pub const SIGNATURE_FILE: &str = "pack.toml.sig";
pub const SIGNATURE_FORMAT_LINE: &str = "fds-pack-signature-v1";
/// Larger manifests and signature files are rejected without parsing.
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub official: bool,
    pub pack_id: Option<String>,
    pub providers: Vec<ProviderSpec>,
}

/// One external command provider (R-802).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    pub id: String,
    pub label: String,
    /// Lowercase, without the dot.
    pub extensions: Vec<String>,
    /// Program and arguments. After loading, a relative program with a path separator is
    /// already resolved against the pack folder (see `load_packs`).
    pub command: Vec<String>,
}

/// Whether `ext` may be used in `extensions` (C38): 1..=16 ASCII alphanumeric characters,
/// `_` or `-`, after ASCII lowercasing. Rejects "", "*", "?", ".pdf", "a.b", "a/b", "a\\b",
/// names with spaces and non-ASCII names, so that a pack cannot claim every file.
pub fn valid_extension(ext: &str) -> bool {
    (1..=16).contains(&ext.len())
        && ext
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn name_is_valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn required_string<'a>(table: &'a toml::Table, name: &str) -> Result<&'a str, String> {
    table
        .get(name)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{name} must be a string"))
}

/// Parses and validates `pack.toml` (rules in the module docs). Extensions are lowercased.
/// Errors: a short English description for the Help screen, e.g. "provider pdf: invalid
/// extension \"*\"" or "command has no {path}". Never panics.
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, String> {
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("pack.toml is too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "pack.toml is not UTF-8".to_string())?;
    let table = toml::from_str::<toml::Table>(text)
        .map_err(|_| "pack.toml is not valid TOML".to_string())?;
    let name = required_string(&table, "name")?;
    if name.len() > 64 || !name_is_valid(name) {
        return Err("name must be 1..=64 characters from [A-Za-z0-9._-]".into());
    }
    let version = required_string(&table, "version")?;
    if version.is_empty() {
        return Err("version must be non-empty".into());
    }
    let official = match table.get("official") {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| "official must be a boolean".to_string())?,
        None => false,
    };
    let pack_id = table
        .get("pack_id")
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "pack_id must be a string".to_string())
        })
        .transpose()?;
    if official
        && !pack_id
            .as_deref()
            .is_some_and(|id| KNOWN_PACKS.contains(&id))
    {
        return Err("official pack_id must name a known pack".into());
    }
    let entries = table
        .get("provider")
        .and_then(toml::Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or_else(|| "provider must be a non-empty array".to_string())?;
    let mut providers = Vec::with_capacity(entries.len());
    let mut ids = HashSet::new();
    for entry in entries {
        let provider = entry
            .as_table()
            .ok_or_else(|| "provider must be a table".to_string())?;
        let id = required_string(provider, "id")?;
        if !name_is_valid(id) {
            return Err("provider id must use [A-Za-z0-9._-]".into());
        }
        if !ids.insert(id) {
            return Err(format!("duplicate provider id {id}"));
        }
        let label = required_string(provider, "label")?;
        if !(1..=40).contains(&label.chars().count()) || label.chars().any(char::is_control) {
            return Err(format!("provider {id}: invalid label"));
        }
        let extensions = provider
            .get("extensions")
            .and_then(toml::Value::as_array)
            .filter(|values| !values.is_empty())
            .ok_or_else(|| format!("provider {id}: extensions must be a non-empty array"))?
            .iter()
            .map(|value| match value.as_str() {
                Some(extension) if valid_extension(extension) => Ok(extension.to_ascii_lowercase()),
                Some(extension) => Err(format!("provider {id}: invalid extension {extension:?}")),
                None => Err(format!("provider {id}: extension must be a string")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let command = provider
            .get("command")
            .and_then(toml::Value::as_array)
            .filter(|values| !values.is_empty())
            .ok_or_else(|| format!("provider {id}: command must be a non-empty array"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("provider {id}: command must contain strings"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let Some(program) = command.first() else {
            return Err(format!("provider {id}: command is empty"));
        };
        if program.is_empty() || program == "{path}" {
            return Err(format!("provider {id}: invalid program"));
        }
        if !command.iter().any(|argument| argument == "{path}") {
            return Err(format!("provider {id}: command has no {{path}}"));
        }
        providers.push(ProviderSpec {
            id: id.to_string(),
            label: label.to_string(),
            extensions,
            command,
        });
    }

    Ok(Manifest {
        name: name.to_string(),
        version: version.to_string(),
        official,
        pack_id,
        providers,
    })
}

enum LimitedReadError {
    Io(std::io::Error),
    TooLarge,
}

fn read_limited(path: &Path) -> Result<Vec<u8>, LimitedReadError> {
    let metadata = fs::metadata(path).map_err(LimitedReadError::Io)?;
    if metadata.is_dir() {
        return Err(LimitedReadError::Io(std::io::Error::from(
            ErrorKind::IsADirectory,
        )));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(LimitedReadError::TooLarge);
    }
    let file = File::open(path).map_err(LimitedReadError::Io)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(LimitedReadError::Io)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(LimitedReadError::TooLarge);
    }
    Ok(bytes)
}

fn verify_pack_signature(
    folder_path: &Path,
    manifest_bytes: &[u8],
    keys: &[(&str, [u8; 32])],
) -> Result<(), SkipReason> {
    let signature_bytes = match read_limited(&folder_path.join(SIGNATURE_FILE)) {
        Ok(bytes) => bytes,
        Err(LimitedReadError::Io(error)) if error.kind() == ErrorKind::NotFound => {
            return Err(SkipReason::Unsigned);
        }
        Err(_) => return Err(SkipReason::BadSignature),
    };
    let headers = license::parse_headers(
        &signature_bytes,
        SIGNATURE_FORMAT_LINE,
        &["key", "signature"],
    )
    .map_err(|_| SkipReason::BadSignature)?;
    let [key_id, signature_text] = headers.as_slice() else {
        return Err(SkipReason::BadSignature);
    };
    let signature: [u8; 64] = B64
        .decode(signature_text)
        .map_err(|_| SkipReason::BadSignature)?
        .try_into()
        .map_err(|_| SkipReason::BadSignature)?;
    let Some((_, key_bytes)) = keys.iter().find(|(id, _)| id == key_id) else {
        return Err(SkipReason::UnknownKey((*key_id).to_string()));
    };
    let verifying_key =
        VerifyingKey::from_bytes(key_bytes).map_err(|_| SkipReason::BadSignature)?;
    verifying_key
        .verify_strict(manifest_bytes, &Signature::from_bytes(&signature))
        .map_err(|_| SkipReason::BadSignature)
}

fn inspect_folder(
    folder: String,
    folder_path: &Path,
    license: &LicenseStatus,
    keys: &[(&str, [u8; 32])],
    disabled: &[String],
    used_ids: &HashSet<String>,
    used_labels: &HashSet<String>,
) -> PackInfo {
    let mut info = PackInfo {
        folder,
        name: None,
        official: false,
        state: PackState::Loaded,
        providers: Vec::new(),
    };
    if disabled.contains(&info.folder) {
        info.state = PackState::Skipped(SkipReason::Disabled);
        return info;
    }
    let manifest_bytes = match read_limited(&folder_path.join(MANIFEST_FILE)) {
        Ok(bytes) => bytes,
        Err(LimitedReadError::Io(error)) if error.kind() == ErrorKind::NotFound => {
            info.state = PackState::Skipped(SkipReason::Malformed("pack.toml is missing".into()));
            return info;
        }
        Err(LimitedReadError::Io(error)) => {
            info.state = PackState::Skipped(SkipReason::Malformed(error.to_string()));
            return info;
        }
        Err(LimitedReadError::TooLarge) => {
            info.state = PackState::Skipped(SkipReason::Malformed("pack.toml is too large".into()));
            return info;
        }
    };
    let mut manifest = match parse_manifest(&manifest_bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            info.state = PackState::Skipped(SkipReason::Malformed(error));
            return info;
        }
    };
    info.name = Some(manifest.name.clone());
    info.official = manifest.official;
    if disabled.contains(&manifest.name) {
        info.state = PackState::Skipped(SkipReason::Disabled);
        return info;
    }
    if manifest.official {
        if let Err(reason) = verify_pack_signature(folder_path, &manifest_bytes, keys) {
            info.state = PackState::Skipped(reason);
            return info;
        }
        let LicenseStatus::Valid(entitlement) = license else {
            info.state = PackState::Skipped(SkipReason::NoLicense);
            return info;
        };
        let Some(pack_id) = manifest.pack_id.as_deref() else {
            info.state = PackState::Skipped(SkipReason::Malformed("pack_id is missing".into()));
            return info;
        };
        if !entitlement.allows(pack_id) {
            info.state = PackState::Skipped(SkipReason::NotEntitled(pack_id.to_string()));
            return info;
        }
    }
    if let Some(provider) = manifest
        .providers
        .iter()
        .find(|provider| used_ids.contains(&provider.id))
    {
        info.state = PackState::Skipped(SkipReason::Duplicate(provider.id.clone()));
        return info;
    }
    if let Some(provider) = manifest
        .providers
        .iter()
        .find(|provider| used_labels.contains(&provider.label))
    {
        info.state = PackState::Skipped(SkipReason::Duplicate(provider.label.clone()));
        return info;
    }
    for provider in &mut manifest.providers {
        if let Some(program) = provider.command.first_mut()
            && Path::new(program).is_relative()
            && (program.contains('/') || program.contains('\\'))
        {
            *program = folder_path.join(&*program).to_string_lossy().into_owned();
        }
    }
    info.providers = manifest.providers;
    info
}

/// Why a pack was not loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Named in `[packs] disabled` (by manifest name or folder name).
    Disabled,
    /// pack.toml missing, unreadable, too large or invalid (`parse_manifest` text).
    Malformed(String),
    /// Official pack without `pack.toml.sig`.
    Unsigned,
    /// The signature does not match pack.toml (or the signature file is malformed).
    BadSignature,
    /// Signed with a key this build does not know.
    UnknownKey(String),
    /// Official pack, but no valid license.
    NoLicense,
    /// Official pack, valid license, but it does not allow this pack id.
    NotEntitled(String),
    /// A provider id or label is already used by a pack loaded earlier.
    Duplicate(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackState {
    Loaded,
    Skipped(SkipReason),
}

/// One folder under `packs/`, loaded or not (shown on the Help screen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    /// The folder name.
    pub folder: String,
    /// From the manifest when it could be read.
    pub name: Option<String>,
    pub official: bool,
    pub state: PackState,
    /// Providers to register; empty unless `state` is `Loaded`.
    pub providers: Vec<ProviderSpec>,
}

/// Loads the packs under `<config_dir>/packs`.
///
/// - `config_dir` None or no `packs` folder -> empty. Entries that are not folders are ignored.
///   Folders are processed in name order (byte order of the file name).
/// - Per folder, the first failing check decides: disabled (folder name or manifest name in
///   `disabled`, exact match) -> `Disabled`; manifest -> `Malformed`; for official packs:
///   signature file -> `Unsigned` / `BadSignature` / `UnknownKey`, then the license:
///   not `Valid` -> `NoLicense`, does not allow `pack_id` -> `NotEntitled(pack_id)`;
///   a provider id or label used by an earlier loaded pack -> `Duplicate(id or label)`.
/// - A skipped pack never stops the others (C26). Never panics.
/// - Loaded providers have their relative programs (with a separator) resolved against the
///   pack folder.
pub fn load_packs(
    config_dir: Option<&Path>,
    license: &LicenseStatus,
    keys: &[(&str, [u8; 32])],
    disabled: &[String],
) -> Vec<PackInfo> {
    let Some(config_dir) = config_dir else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(packs_dir(config_dir)) else {
        return Vec::new();
    };
    let mut folders = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| (entry.file_name(), entry.path()))
        .collect::<Vec<_>>();
    folders.sort_by(|(left, _), (right, _)| left.as_encoded_bytes().cmp(right.as_encoded_bytes()));

    let mut packs = Vec::with_capacity(folders.len());
    let mut used_ids = HashSet::new();
    let mut used_labels = HashSet::new();
    for (folder, folder_path) in folders {
        let info = inspect_folder(
            folder.to_string_lossy().into_owned(),
            &folder_path,
            license,
            keys,
            disabled,
            &used_ids,
            &used_labels,
        );
        if info.state == PackState::Loaded {
            for provider in &info.providers {
                used_ids.insert(provider.id.clone());
                used_labels.insert(provider.label.clone());
            }
        }
        packs.push(info);
    }
    packs
}

/// The pack folder of `config_dir` (for messages and docs).
pub fn packs_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(PACKS_DIR)
}
