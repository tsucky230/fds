//! Supporter license: offline verification of a signed license file (Phase 4, R-1000 to R-1004,
//! design prompt §29, ADR 0005).
//!
//! The license file is text. Its payload (TOML) is carried as base64 of the exact signed bytes,
//! so verification never depends on re-serializing TOML. Nothing here touches the network, and a
//! missing or broken license never stops the Core (R-900, R-1000).
//!
//! File format (`<config dir>/supporter.license`, UTF-8, LF or CRLF):
//!
//! ```text
//! fds-license-v1
//! key: <key id>
//! payload: <base64 (standard alphabet, padded) of the payload TOML bytes>
//! signature: <base64 of the 64-byte Ed25519 signature over those payload bytes>
//! ```
//!
//! Blank lines and lines starting with `#` are ignored. The first other line must be
//! `fds-license-v1`; then each of `key:`, `payload:`, `signature:` exactly once, in any order
//! (the value after the colon is trimmed and must not be empty). Any other line is an error.
//!
//! Payload TOML:
//!
//! ```toml
//! license_id = "FDS1-0001"   # required, 1..=64 chars, no control characters; never an e-mail (R-1003)
//! product = "fds"            # required
//! series = 1                 # required, integer >= 1: the major version series it is for
//! packs = ["all"]            # required, array of non-empty strings (may be empty)
//! issued = "2026-10-20"      # required, non-empty string (shown, not interpreted)
//! holder = "Name"            # optional display name
//! ```
//!
//! Unknown payload keys are ignored (they are covered by the signature anyway).

use std::io::{ErrorKind, Read};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, VerifyingKey};

/// File name inside the config directory.
pub const LICENSE_FILE: &str = "supporter.license";
/// Larger files are rejected without parsing.
pub const MAX_LICENSE_BYTES: u64 = 64 * 1024;
/// First line of a license file.
pub const FORMAT_LINE: &str = "fds-license-v1";
/// Pack ids an official pack or a license may name (R-1004; requirements §14).
pub const KNOWN_PACKS: &[&str] = &["viewer", "archive", "developer", "remote", "power"];

/// Official public keys: (key id, Ed25519 public key bytes).
///
/// Empty until the production key pair exists. The private key is never in this repository or
/// in any binary (R-1002); while this is empty no license can be valid.
pub const OFFICIAL_KEYS: &[(&str, [u8; 32])] = &[];

// The test key must never reach a release build (C37).
#[cfg(all(feature = "dev-license-key", not(debug_assertions)))]
compile_error!("the dev-license-key feature is for debug builds only; never ship it");

/// Public key of the TEST key pair (seed `[7; 32]`, known to the tests). Accepted only when the
/// `dev-license-key` feature is on, which is allowed in debug builds only (C37).
#[cfg(feature = "dev-license-key")]
pub const DEV_KEY: (&str, [u8; 32]) = (
    "fds-dev-test",
    [
        0xEA, 0x4A, 0x6C, 0x63, 0xE2, 0x9C, 0x52, 0x0A, 0xBE, 0xF5, 0x50, 0x7B, 0x13, 0x2E, 0xC5,
        0xF9, 0x95, 0x47, 0x76, 0xAE, 0xBE, 0xBE, 0x7B, 0x92, 0x42, 0x1E, 0xEA, 0x69, 0x14, 0x46,
        0xD2, 0x2C,
    ],
);

/// Whether this build accepts the test key (shown as a warning on the Help screen, C37).
pub const fn accepts_dev_key() -> bool {
    cfg!(feature = "dev-license-key")
}

/// Keys this build verifies with: `OFFICIAL_KEYS`, plus `DEV_KEY` when `accepts_dev_key()`.
pub fn build_keys() -> Vec<(&'static str, [u8; 32])> {
    #[allow(unused_mut)]
    let mut keys = OFFICIAL_KEYS.to_vec();
    #[cfg(feature = "dev-license-key")]
    if accepts_dev_key() {
        keys.push(DEV_KEY);
    }
    keys
}

/// What a valid license grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entitlement {
    pub license_id: String,
    pub series: u32,
    pub packs: Vec<String>,
    pub issued: String,
    pub holder: Option<String>,
}

impl Entitlement {
    /// True when `packs` contains `pack_id` or `"all"` (exact, case-sensitive match).
    /// Unknown names in `packs` grant nothing and are not an error.
    pub fn allows(&self, pack_id: &str) -> bool {
        self.packs
            .iter()
            .any(|pack| pack == "all" || pack == pack_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseStatus {
    /// No license file. The normal case for Core users.
    NotInstalled,
    Valid(Entitlement),
    Invalid(InvalidReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidReason {
    /// The file exists but could not be read (OS error text).
    Unreadable(String),
    /// Not a license file of this format, or its payload is not the expected TOML. The text
    /// says what is wrong (for the Help screen), e.g. "missing signature line".
    Malformed(String),
    /// Signed with a key this build does not know (the key id).
    UnknownKey(String),
    /// The signature does not match the payload (changed or forged).
    BadSignature,
    /// Signed and valid, but for another product (the product name).
    WrongProduct(String),
    /// Signed and valid, but for another major series. `license`: the license's series;
    /// `app`: this app's major version (as passed to `verify`).
    SeriesMismatch { license: u32, app: u32 },
}

/// The series a license must have for an app whose major version is `app_major`:
/// 0 (pre-1.0 builds) -> 1, otherwise `app_major` (R-901 to R-903).
pub fn required_series(app_major: u32) -> u32 {
    app_major.max(1)
}

/// Parses the shared line-based header format without interpreting its values.
pub(crate) fn parse_headers<'a>(
    content: &'a [u8],
    format: &str,
    names: &[&str],
) -> Result<Vec<&'a str>, String> {
    let text = std::str::from_utf8(content).map_err(|_| "file is not UTF-8".to_string())?;
    let mut lines = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'));
    if lines.next() != Some(format) {
        return Err("invalid format line".into());
    }

    let mut values = vec![None; names.len()];
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "invalid header line".to_string())?;
        let Some(position) = names.iter().position(|expected| *expected == name) else {
            return Err("unknown header line".into());
        };
        let value = value.trim();
        if value.is_empty() {
            return Err(format!("empty {name} line"));
        }
        let Some(slot) = values.get_mut(position) else {
            return Err("invalid header line".into());
        };
        if slot.replace(value).is_some() {
            return Err(format!("duplicate {name} line"));
        }
    }

    values
        .into_iter()
        .zip(names)
        .map(|(value, name)| value.ok_or_else(|| format!("missing {name} line")))
        .collect()
}

fn parse_payload(bytes: &[u8]) -> Result<(Entitlement, String), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "payload is not UTF-8".to_string())?;
    let table =
        toml::from_str::<toml::Table>(text).map_err(|_| "payload is not valid TOML".to_string())?;

    let license_id = table
        .get("license_id")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "license_id must be a string".to_string())?;
    if !(1..=64).contains(&license_id.chars().count()) || license_id.chars().any(char::is_control) {
        return Err("license_id must be 1..=64 characters without controls".into());
    }
    let product = table
        .get("product")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "product must be a string".to_string())?;
    let series = table
        .get("series")
        .and_then(toml::Value::as_integer)
        .and_then(|number| u32::try_from(number).ok())
        .filter(|number| *number >= 1)
        .ok_or_else(|| "series must be an integer >= 1".to_string())?;
    let packs = table
        .get("packs")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "packs must be an array".to_string())?
        .iter()
        .map(|value| match value.as_str() {
            Some(pack) if !pack.is_empty() => Ok(pack.to_string()),
            _ => Err("packs must contain non-empty strings".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let issued = table
        .get("issued")
        .and_then(toml::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "issued must be a non-empty string".to_string())?;
    let holder = table
        .get("holder")
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "holder must be a string".to_string())
        })
        .transpose()?;

    Ok((
        Entitlement {
            license_id: license_id.to_string(),
            series,
            packs,
            issued: issued.to_string(),
            holder,
        },
        product.to_string(),
    ))
}

/// Verifies license file content against `keys` for an app with major version `app_major`.
///
/// Pure (no I/O). Never returns `NotInstalled` or `Unreadable`. Checks, in this order; the
/// first failure decides the result:
/// 1. UTF-8 text, the format line, the three header lines, base64 of payload and signature,
///    signature length 64 -> otherwise `Malformed`.
/// 2. The key id is one of `keys` -> otherwise `UnknownKey(id)`.
/// 3. `verify_strict` of the signature over the decoded payload bytes -> otherwise `BadSignature`.
/// 4. The payload is UTF-8 TOML with the fields above -> otherwise `Malformed`.
/// 5. `product == "fds"` -> otherwise `WrongProduct(product)`.
/// 6. `series == required_series(app_major)` -> otherwise `SeriesMismatch`.
pub fn verify(content: &[u8], keys: &[(&str, [u8; 32])], app_major: u32) -> LicenseStatus {
    let malformed = |message: String| LicenseStatus::Invalid(InvalidReason::Malformed(message));
    let headers = match parse_headers(content, FORMAT_LINE, &["key", "payload", "signature"]) {
        Ok(headers) => headers,
        Err(message) => return malformed(message),
    };
    let [key_id, payload_text, signature_text] = headers.as_slice() else {
        return malformed("invalid header lines".into());
    };
    let payload = match B64.decode(payload_text) {
        Ok(payload) => payload,
        Err(_) => return malformed("payload is not base64".into()),
    };
    let signature_bytes: [u8; 64] = match B64.decode(signature_text) {
        Ok(bytes) => match bytes.try_into() {
            Ok(bytes) => bytes,
            Err(_) => return malformed("signature must be 64 bytes".into()),
        },
        Err(_) => return malformed("signature is not base64".into()),
    };
    let Some((_, key_bytes)) = keys.iter().find(|(id, _)| id == key_id) else {
        return LicenseStatus::Invalid(InvalidReason::UnknownKey((*key_id).to_string()));
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(key_bytes) else {
        return LicenseStatus::Invalid(InvalidReason::BadSignature);
    };
    let signature = Signature::from_bytes(&signature_bytes);
    if verifying_key.verify_strict(&payload, &signature).is_err() {
        return LicenseStatus::Invalid(InvalidReason::BadSignature);
    }
    let (entitlement, product) = match parse_payload(&payload) {
        Ok(parsed) => parsed,
        Err(message) => return malformed(message),
    };
    if product != "fds" {
        return LicenseStatus::Invalid(InvalidReason::WrongProduct(product));
    }
    if entitlement.series != required_series(app_major) {
        return LicenseStatus::Invalid(InvalidReason::SeriesMismatch {
            license: entitlement.series,
            app: app_major,
        });
    }
    LicenseStatus::Valid(entitlement)
}

/// Reads `<config_dir>/supporter.license` and verifies it.
///
/// - `config_dir` None, or no such file -> `NotInstalled`.
/// - Larger than `MAX_LICENSE_BYTES` -> `Invalid(Malformed)` without reading it all.
/// - Any other I/O error -> `Invalid(Unreadable(os text))`.
/// - Otherwise `verify`.
pub fn load(config_dir: Option<&Path>, keys: &[(&str, [u8; 32])], app_major: u32) -> LicenseStatus {
    let Some(config_dir) = config_dir else {
        return LicenseStatus::NotInstalled;
    };
    let path = config_dir.join(LICENSE_FILE);
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return LicenseStatus::NotInstalled,
        Err(error) => return LicenseStatus::Invalid(InvalidReason::Unreadable(error.to_string())),
    };
    if metadata.is_dir() {
        let error = std::io::Error::from(ErrorKind::IsADirectory);
        return LicenseStatus::Invalid(InvalidReason::Unreadable(error.to_string()));
    }
    if metadata.len() > MAX_LICENSE_BYTES {
        return LicenseStatus::Invalid(InvalidReason::Malformed(
            "license file is too large".into(),
        ));
    }
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return LicenseStatus::NotInstalled,
        Err(error) => return LicenseStatus::Invalid(InvalidReason::Unreadable(error.to_string())),
    };
    let mut content = Vec::new();
    if let Err(error) = file.take(MAX_LICENSE_BYTES + 1).read_to_end(&mut content) {
        return LicenseStatus::Invalid(InvalidReason::Unreadable(error.to_string()));
    }
    if content.len() as u64 > MAX_LICENSE_BYTES {
        return LicenseStatus::Invalid(InvalidReason::Malformed(
            "license file is too large".into(),
        ));
    }
    verify(&content, keys, app_major)
}
