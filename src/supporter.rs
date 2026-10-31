//! What the Help screen says about the Supporter license and the packs (C32, C37).
//!
//! Loaded once at startup by `run` (license::load, pack::load_packs); the App only shows it.

use crate::i18n::Catalog;
use crate::license::{InvalidReason, LicenseStatus};
use crate::pack::{PackInfo, PackState, SkipReason};
use crate::text::sanitize_for_display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupporterInfo {
    pub license: LicenseStatus,
    pub packs: Vec<PackInfo>,
    /// This build accepts the test key (`license::accepts_dev_key()`).
    pub dev_key: bool,
}

impl Default for SupporterInfo {
    fn default() -> Self {
        Self {
            license: LicenseStatus::NotInstalled,
            packs: Vec::new(),
            dev_key: false,
        }
    }
}

/// Lines for the end of the Help screen, in the catalog's language. Catalog keys under
/// `[supporter]`:
///
/// 1. The license, always one line:
///    - NotInstalled -> `supporter.none`
///    - Valid -> `supporter.valid` with `{id}`, `{series}`, `{packs}` (pack names joined with
///      ", ", or `supporter.no_packs` when empty)
///    - Invalid -> `supporter.invalid` with `{reason}` from `supporter.reason_<kind>`:
///      `malformed` {detail}, `unknown_key` {key}, `bad_signature`, `wrong_product` {product},
///      `series` {license} {app}, `unreadable` {detail}
/// 2. When `dev_key`: `supporter.dev_key` (a warning that never goes away, C37).
/// 3. One line per pack, in the given order:
///    - Loaded -> `supporter.pack_loaded` with `{name}`, `{kind}`, `{count}` (providers)
///    - Skipped -> `supporter.pack_skipped` with `{name}`, `{kind}`, `{reason}` from
///      `supporter.skip_<kind>`: `disabled`, `malformed` {detail}, `unsigned`, `bad_signature`,
///      `unknown_key` {key}, `no_license`, `not_entitled` {pack}, `duplicate` {id}
///
///    `{name}` is the manifest name, or the folder name when the manifest could not be read;
///    `{kind}` is `supporter.kind_official` or `supporter.kind_community`.
///
/// Every value from files (ids, names, details) goes through `text::sanitize_for_display`, so
/// a crafted license or pack cannot put control characters on the screen.
pub fn help_lines(info: &SupporterInfo, catalog: &Catalog) -> Vec<String> {
    let license = match &info.license {
        LicenseStatus::NotInstalled => catalog.text("supporter.none", &[]),
        LicenseStatus::Valid(entitlement) => {
            let id = sanitize_for_display(&entitlement.license_id);
            let series = entitlement.series.to_string();
            let packs = if entitlement.packs.is_empty() {
                catalog.text("supporter.no_packs", &[])
            } else {
                entitlement
                    .packs
                    .iter()
                    .map(|pack| sanitize_for_display(pack))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            catalog.text(
                "supporter.valid",
                &[("id", &id), ("series", &series), ("packs", &packs)],
            )
        }
        LicenseStatus::Invalid(reason) => {
            let reason = match reason {
                InvalidReason::Malformed(detail) => catalog.text(
                    "supporter.reason_malformed",
                    &[("detail", &sanitize_for_display(detail))],
                ),
                InvalidReason::UnknownKey(key) => catalog.text(
                    "supporter.reason_unknown_key",
                    &[("key", &sanitize_for_display(key))],
                ),
                InvalidReason::BadSignature => catalog.text("supporter.reason_bad_signature", &[]),
                InvalidReason::WrongProduct(product) => catalog.text(
                    "supporter.reason_wrong_product",
                    &[("product", &sanitize_for_display(product))],
                ),
                InvalidReason::SeriesMismatch { license, app } => catalog.text(
                    "supporter.reason_series",
                    &[("license", &license.to_string()), ("app", &app.to_string())],
                ),
                InvalidReason::Unreadable(detail) => catalog.text(
                    "supporter.reason_unreadable",
                    &[("detail", &sanitize_for_display(detail))],
                ),
            };
            catalog.text("supporter.invalid", &[("reason", &reason)])
        }
    };
    let mut lines = vec![license];
    if info.dev_key {
        lines.push(catalog.text("supporter.dev_key", &[]));
    }
    for pack in &info.packs {
        let name = sanitize_for_display(pack.name.as_deref().unwrap_or(&pack.folder));
        let kind = catalog.text(
            if pack.official {
                "supporter.kind_official"
            } else {
                "supporter.kind_community"
            },
            &[],
        );
        let line = match &pack.state {
            PackState::Loaded => catalog.text(
                "supporter.pack_loaded",
                &[
                    ("name", &name),
                    ("kind", &kind),
                    ("count", &pack.providers.len().to_string()),
                ],
            ),
            PackState::Skipped(reason) => {
                let reason = match reason {
                    SkipReason::Disabled => catalog.text("supporter.skip_disabled", &[]),
                    SkipReason::Malformed(detail) => catalog.text(
                        "supporter.skip_malformed",
                        &[("detail", &sanitize_for_display(detail))],
                    ),
                    SkipReason::Unsigned => catalog.text("supporter.skip_unsigned", &[]),
                    SkipReason::BadSignature => catalog.text("supporter.skip_bad_signature", &[]),
                    SkipReason::UnknownKey(key) => catalog.text(
                        "supporter.skip_unknown_key",
                        &[("key", &sanitize_for_display(key))],
                    ),
                    SkipReason::NoLicense => catalog.text("supporter.skip_no_license", &[]),
                    SkipReason::NotEntitled(pack) => catalog.text(
                        "supporter.skip_not_entitled",
                        &[("pack", &sanitize_for_display(pack))],
                    ),
                    SkipReason::Duplicate(id) => catalog.text(
                        "supporter.skip_duplicate",
                        &[("id", &sanitize_for_display(id))],
                    ),
                };
                catalog.text(
                    "supporter.pack_skipped",
                    &[("name", &name), ("kind", &kind), ("reason", &reason)],
                )
            }
        };
        lines.push(line);
    }
    lines
}
