//! Preview pane loader (R-200-9): what the right side of the list shows for the item under the
//! cursor. Runs on a loader thread (`run.rs`); the state lives in `app_preview.rs`.
//!
//! Only built-in viewers are used: moving the cursor must never start an external command
//! (pack providers run only when the user opens the viewer). Every read is bounded, so a huge
//! file or folder costs the same as a small one.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::entry::FileEntry;
use crate::viewer::detect::{FileClass, ImageFormat, Sniff, classify};
use crate::viewer::styled::StyledLine;
use crate::viewer::{self, ViewError, ViewerKind};

/// Text and Markdown previews read at most this many bytes of the file.
pub const PREVIEW_TEXT_LIMIT: u64 = 64 * 1024;
/// Binary previews keep at most this many bytes of the head.
/// Folder previews list at most this many entries.
pub const PREVIEW_DIR_LIMIT: usize = 1000;
/// Image previews decode files up to this size; larger ones show the format and size only.
pub const PREVIEW_IMAGE_LIMIT: u64 = 16 * 1024 * 1024;
/// Decoded images are shrunk so that the longer side is at most this many pixels.
pub const PREVIEW_THUMB_SIDE: u32 = 512;

/// A decoded, shrunk picture for the pane (R-200-10). Shared, so handing it to the UI is cheap.
/// Equal when the color type, the size and the pixels are equal.
#[derive(Clone)]
pub struct Thumbnail(pub Arc<image::DynamicImage>);

impl PartialEq for Thumbnail {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&*self.0, &*other.0);
        a.color() == b.color()
            && (a.width(), a.height()) == (b.width(), b.height())
            && a.as_bytes() == b.as_bytes()
    }
}

impl Eq for Thumbnail {}

impl std::fmt::Debug for Thumbnail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Thumbnail({}x{})", self.0.width(), self.0.height())
    }
}

/// A loaded preview, ready to be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewDoc {
    /// Text or Markdown: decoded, tab-expanded, sanitized lines (as the viewer makes them).
    Lines {
        /// `ViewerKind::Text` or `ViewerKind::Markdown`.
        kind: ViewerKind,
        lines: Vec<StyledLine>,
        /// Name of the detected encoding (e.g. "UTF-8", "Shift_JIS").
        encoding: String,
        /// True when the file is longer than `PREVIEW_TEXT_LIMIT`.
        truncated: bool,
    },
    /// Binary: only its size. Enter shows it in the hex viewer.
    Binary { file_len: u64 },
    /// Image: format and size in pixels read from the header only (None when unreadable).
    /// Never decoded here; the viewer shows the picture.
    Image {
        format: ImageFormat,
        dimensions: Option<(u32, u32)>,
        file_len: u64,
    },
    /// Image decoded for the pane (`load_preview_with(.., thumbnails: true, ..)`, R-200-10):
    /// format, the original size in pixels and the shrunk picture.
    Picture {
        format: ImageFormat,
        dimensions: (u32, u32),
        file_len: u64,
        thumbnail: Thumbnail,
    },
    /// Folder: its entries, folders first in natural order (`sort::sort_entries`), no `[..]`.
    /// `more` is true when entries beyond `PREVIEW_DIR_LIMIT` were left out.
    Folder { entries: Vec<FileEntry>, more: bool },
}

/// Why there is no preview. Shown inside the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    /// FIFO, device or socket: never opened, so a FIFO cannot block the loader.
    NotAFile,
    /// The OS error text (missing, permission denied, broken symlink, ...).
    Io(String),
    /// `cancel` was set; the caller drops this result.
    Cancelled,
}

/// Like `load_preview`; with `thumbnails` an image is also decoded for the pane (R-200-10).
///
/// Behavior:
/// - `thumbnails` false: exactly `load_preview`.
/// - `thumbnails` true and the file classifies as an image (as in `load_preview`):
///   - length at most `PREVIEW_IMAGE_LIMIT`, declared size at most
///     `viewer::image::MAX_DECODE_SIDE` on each side, and decoding succeeds -> `Picture` with the
///     original size and a thumbnail whose longer side is at most `PREVIEW_THUMB_SIDE` (aspect
///     ratio kept; a picture already within it is kept as it is).
///   - otherwise (too large, broken, unsupported) -> exactly what `load_preview` gives (`Image`
///     with the header's size or None), not an error.
///   - `cancel` is checked before and after decoding.
/// - Every other file and folder: exactly `load_preview`.
pub fn load_preview_with(
    path: &Path,
    show_hidden: bool,
    thumbnails: bool,
    cancel: &AtomicBool,
) -> Result<PreviewDoc, PreviewError> {
    let preview = load_preview(path, show_hidden, cancel)?;
    if !thumbnails {
        return Ok(preview);
    }

    let max = viewer::image::MAX_DECODE_SIDE;
    match preview {
        PreviewDoc::Image {
            format,
            dimensions: Some((width, height)),
            file_len,
        } if file_len <= PREVIEW_IMAGE_LIMIT && width <= max && height <= max => {
            check_cancel(cancel)?;
            let decoded = decode_thumbnail(path);
            check_cancel(cancel)?;
            Ok(match decoded {
                Some(thumbnail) => PreviewDoc::Picture {
                    format,
                    dimensions: (width, height),
                    file_len,
                    thumbnail,
                },
                None => PreviewDoc::Image {
                    format,
                    dimensions: Some((width, height)),
                    file_len,
                },
            })
        }
        other => Ok(other),
    }
}

/// Decodes the image (with the viewer's size limits) and shrinks it to `PREVIEW_THUMB_SIDE`.
/// None when it cannot be opened or decoded.
fn decode_thumbnail(path: &Path) -> Option<Thumbnail> {
    let mut reader = image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(viewer::image::MAX_DECODE_SIDE);
    limits.max_image_height = Some(viewer::image::MAX_DECODE_SIDE);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = if image.width().max(image.height()) > PREVIEW_THUMB_SIDE {
        image.thumbnail(PREVIEW_THUMB_SIDE, PREVIEW_THUMB_SIDE)
    } else {
        image
    };
    Some(Thumbnail(Arc::new(image)))
}

/// Loads the preview of `path`.
///
/// Behavior:
/// - `cancel` is checked before any I/O and between steps (and at least every 100 folder
///   entries); once it is set the result is `Err(Cancelled)`.
/// - Symlinks are followed (`fs::metadata`). Metadata failing (missing, broken link, no
///   permission) -> `Err(Io(os error text))`.
/// - Folder -> `Folder`. Entries are read like the list reads them (`fs::to_entry`: kind,
///   hidden flag, lossy display name; an entry that fails to read is skipped; the UI sanitizes the
///   name when drawing). Hidden entries are left out
///   unless `show_hidden`. At most `PREVIEW_DIR_LIMIT` entries are kept; `more` is true when at
///   least one more entry that would be shown exists. The folder failing to open ->
///   `Err(Io(..))`.
/// - Not a regular file (FIFO, device, socket) -> `Err(NotAFile)` without opening it.
/// - Regular file: reads at most `PREVIEW_TEXT_LIMIT` bytes and classifies the head with
///   `viewer::detect::classify` (as the viewer does):
///   - Image -> `Image` (dimensions from the header; a broken header gives `None`, not an error)
///   - Binary -> `Binary` with the file's length
///   - Markdown -> `Lines { kind: Markdown }` rendered like the Markdown viewer
///   - Text (also an empty file) -> `Lines { kind: Text }` like the Text viewer
///   - Text and Markdown are decoded with the viewer's encoding detection (UTF-8, Shift_JIS, ...)
///     and sanitized: no control characters reach the lines.
///   - Opening or reading failing -> `Err(Io(..))`.
/// - Never uses the viewer `Registry`, so external providers are never started.
pub fn load_preview(
    path: &Path,
    show_hidden: bool,
    cancel: &AtomicBool,
) -> Result<PreviewDoc, PreviewError> {
    check_cancel(cancel)?;
    let metadata = fs::metadata(path);
    check_cancel(cancel)?;
    let metadata = metadata.map_err(|error| PreviewError::Io(error.to_string()))?;

    if metadata.is_dir() {
        let directory = fs::read_dir(path);
        check_cancel(cancel)?;
        let directory = directory.map_err(|error| PreviewError::Io(error.to_string()))?;
        let mut entries = Vec::new();
        let mut more = false;

        for item in directory {
            check_cancel(cancel)?;
            let Ok(item) = item else {
                continue;
            };
            let entry = crate::fs::to_entry(&item);
            if !show_hidden && entry.hidden {
                continue;
            }
            if entries.len() == PREVIEW_DIR_LIMIT {
                more = true;
                break;
            }
            entries.push(entry);
        }

        crate::sort::sort_entries(&mut entries);
        return Ok(PreviewDoc::Folder { entries, more });
    }

    if !metadata.is_file() {
        return Err(PreviewError::NotAFile);
    }

    let read = viewer::read_limited(path, PREVIEW_TEXT_LIMIT);
    check_cancel(cancel)?;
    let (bytes, truncated) = read.map_err(|error| match error {
        ViewError::NotAFile => PreviewError::NotAFile,
        ViewError::Io(message) => PreviewError::Io(message),
    })?;
    let sniff = Sniff::new(
        bytes[..bytes.len().min(viewer::SNIFF_BYTES)].to_vec(),
        metadata.len(),
        path.extension(),
    );
    let class = classify(&sniff);

    match class {
        FileClass::Image(format) => {
            // Header only; the picture is decoded by the viewer.
            let dimensions = image::ImageReader::open(path)
                .ok()
                .and_then(|reader| reader.with_guessed_format().ok())
                .and_then(|reader| reader.into_dimensions().ok());
            Ok(PreviewDoc::Image {
                format,
                dimensions,
                file_len: metadata.len(),
            })
        }
        FileClass::Binary => Ok(PreviewDoc::Binary {
            file_len: metadata.len(),
        }),
        FileClass::Markdown | FileClass::Text => {
            let (source, encoding) = crate::viewer::encoding::decode(&bytes);
            check_cancel(cancel)?;
            let (kind, lines) = if class == FileClass::Markdown {
                (ViewerKind::Markdown, viewer::markdown::render(&source))
            } else {
                (ViewerKind::Text, viewer::text::to_lines(&source))
            };
            Ok(PreviewDoc::Lines {
                kind,
                lines,
                encoding: encoding.to_string(),
                truncated,
            })
        }
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), PreviewError> {
    if cancel.load(Ordering::Relaxed) {
        Err(PreviewError::Cancelled)
    } else {
        Ok(())
    }
}
