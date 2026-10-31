//! Status messages as data. App stores a `Message`; the UI turns it into text with a Catalog,
//! so App logic never depends on the display language (R-1600).

use crate::i18n::Catalog;

/// Feature names used in "not available yet" messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    View,
    Edit,
    Copy,
    Move,
    Delete,
    OpenFile,
}

impl Feature {
    fn key(self) -> &'static str {
        match self {
            Feature::View => "feature.view",
            Feature::Edit => "feature.edit",
            Feature::Copy => "feature.copy",
            Feature::Move => "feature.move",
            Feature::Delete => "feature.delete",
            Feature::OpenFile => "feature.open_file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    HiddenShown,
    HiddenHidden,
    NotAvailable {
        feature: Feature,
        phase: u8,
    },
    BrokenSymlink {
        name: String,
    },
    AtRoot,
    CannotOpen {
        path: String,
        reason: String,
    },
    Resumed,
    /// `p` on the list switched how pictures are drawn (R-200-10); the name of the new method.
    ImageMode {
        name: String,
    },
    /// The entry is not a regular file (directory, `[..]`, FIFO, device, ...).
    NotAFile {
        name: String,
    },
    /// Opening a file in the viewer failed.
    CannotView {
        name: String,
        reason: String,
    },
    /// Copy / Move / Delete / Rename with nothing to operate on.
    NothingSelected,
    /// Another operation was requested while choosing a destination (§45 I8).
    FinishSelectionFirst,
    /// Move into the folder the entries already are in.
    SameFolder,
    /// The destination is one of the sources or inside one (P3-5).
    InsideSource {
        name: String,
    },
    /// Copy / Move / Delete / Rename / mkdir was cancelled before it started.
    Cancelled,
    /// A job finished. `op` is the catalog key suffix: "copy", "move", "delete" or "trash".
    JobDone {
        op: &'static str,
        done: u64,
        skipped: u64,
        failed: u64,
        cancelled: bool,
    },
    /// Rename / mkdir failed. `reason` is a catalog key under `ops.` (e.g. "exists") or, for OS
    /// errors, the OS text prefixed with "!" (shown as is).
    OperationFailed {
        name: String,
        reason: String,
    },
    /// The external editor could not be started or failed.
    EditorFailed {
        reason: String,
    },
    /// A standard Core archive was created or extracted.
    ArchiveDone {
        name: String,
    },
    /// The best viewer for a file failed and another one is shown (C28). `reason` is the
    /// failure text (shown as is, after sanitizing).
    ViewerFallback {
        failed: crate::viewer::ViewerKind,
        reason: String,
    },
    /// `[ops] delete` has a value FDS does not know; F8 moves to the trash (M1-a).
    /// `value` is the value as written in the config file.
    DeleteSettingUnknown {
        value: String,
    },
    /// `o` or an application from the settings did not work.
    CannotOpenExternal {
        reason: String,
    },
    /// A typed command was not run (R-412).
    CommandRejected(crate::shell_cmd::CommandError),
    /// The shell for a typed command could not start (R-412).
    CommandFailed {
        reason: String,
    },
    /// Tab: no preview pane on screen, or too low to split it (R-200-13).
    PeekUnavailable,
    /// Tab: the folder's preview has not arrived yet.
    PeekLoading,
    /// Tab: the folder has nothing to list.
    PeekEmpty,
}

impl Message {
    /// Text in the catalog's language.
    pub fn render(&self, catalog: &Catalog) -> String {
        match self {
            Message::HiddenShown => catalog.text("status.hidden_shown", &[]),
            Message::HiddenHidden => catalog.text("status.hidden_hidden", &[]),
            Message::NotAvailable { feature, phase } => {
                let feature = catalog.text(feature.key(), &[]);
                let phase = phase.to_string();
                catalog.text(
                    "status.not_available",
                    &[("feature", &feature), ("phase", &phase)],
                )
            }
            Message::BrokenSymlink { name } => {
                catalog.text("status.broken_symlink", &[("name", name)])
            }
            Message::AtRoot => catalog.text("status.at_root", &[]),
            Message::CannotOpen { path, reason } => {
                catalog.text("status.cannot_open", &[("path", path), ("reason", reason)])
            }
            Message::Resumed => catalog.text("status.resumed", &[]),
            Message::ImageMode { name } => catalog.text("status.image_mode", &[("name", name)]),
            Message::NotAFile { name } => catalog.text("status.not_a_file", &[("name", name)]),
            Message::CannotView { name, reason } => {
                catalog.text("status.cannot_view", &[("name", name), ("reason", reason)])
            }
            Message::NothingSelected => catalog.text("ops.nothing_selected", &[]),
            Message::FinishSelectionFirst => catalog.text("ops.finish_selection_first", &[]),
            Message::SameFolder => catalog.text("ops.same_folder", &[]),
            Message::InsideSource { name } => catalog.text("ops.inside_source", &[("name", name)]),
            Message::Cancelled => catalog.text("ops.cancelled", &[]),
            Message::JobDone {
                op,
                done,
                skipped,
                failed,
                cancelled,
            } => {
                let op = catalog.text(&format!("ops.name_{op}"), &[]);
                let (done, skipped, failed) =
                    (done.to_string(), skipped.to_string(), failed.to_string());
                let key = if *cancelled {
                    "ops.job_cancelled"
                } else {
                    "ops.job_done"
                };
                catalog.text(
                    key,
                    &[
                        ("op", &op),
                        ("done", &done),
                        ("skipped", &skipped),
                        ("failed", &failed),
                    ],
                )
            }
            Message::OperationFailed { name, reason } => {
                let reason = match reason.strip_prefix('!') {
                    Some(os_text) => os_text.to_string(),
                    None => catalog.text(&format!("ops.reason_{reason}"), &[]),
                };
                catalog.text("ops.failed", &[("name", name), ("reason", &reason)])
            }
            Message::EditorFailed { reason } => {
                catalog.text("ops.editor_failed", &[("reason", reason)])
            }
            Message::ArchiveDone { name } => catalog.text("ops.archive_done", &[("name", name)]),
            Message::ViewerFallback { failed, reason } => catalog.text(
                "viewer.fallback",
                &[
                    ("viewer", &crate::viewer::kind_label(*failed, catalog)),
                    ("reason", &crate::text::sanitize_for_display(reason)),
                ],
            ),
            Message::DeleteSettingUnknown { value } => {
                catalog.text("status.delete_setting_unknown", &[("value", value)])
            }
            Message::CannotOpenExternal { reason } => catalog.text(
                "status.cannot_open_external",
                &[("reason", &crate::text::sanitize_for_display(reason))],
            ),
            Message::CommandRejected(reason) => command_refusal(reason, catalog),
            Message::CommandFailed { reason } => catalog.text(
                "command.failed",
                &[("reason", &crate::text::sanitize_for_display(reason))],
            ),
            Message::PeekUnavailable => catalog.text("status.peek_unavailable", &[]),
            Message::PeekLoading => catalog.text("status.peek_loading", &[]),
            Message::PeekEmpty => catalog.text("status.peek_empty", &[]),
        }
    }
}

fn command_refusal(reason: &crate::shell_cmd::CommandError, catalog: &Catalog) -> String {
    use crate::shell_cmd::CommandError;
    let (key, name) = match reason {
        CommandError::NoCommand => ("command.no_command", None),
        CommandError::QuotedPlaceholder => ("command.quoted_placeholder", None),
        CommandError::FilesNotAWord => ("command.files_not_a_word", None),
        CommandError::NoTarget => ("command.no_target", None),
        CommandError::NonUtf8 { name } => ("command.non_utf8", Some(name)),
        CommandError::UnsafeName { name } => ("command.unsafe_name", Some(name)),
        CommandError::TooLong => ("command.too_long", None),
        CommandError::UncFolder => ("command.unc_folder", None),
    };
    let name = name.map_or_else(String::new, |n| crate::text::sanitize_for_display(n));
    catalog.text(key, &[("name", &name)])
}
