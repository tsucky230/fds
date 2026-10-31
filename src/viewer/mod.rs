//! Universal Viewer (design prompt §21-§24 / R-500 to R-507).
//!
//! Flow: `open_document` runs on a loader thread. It reads a bounded head of the file (`Sniff`),
//! asks the `Registry` which viewers fit (best first), and loads the chosen one into a `Document`.
//! The UI thread then keeps a `ViewerState` (see `view.rs`) that scrolls over the document.

pub mod detect;
pub mod encoding;
pub mod external;
pub mod hex;
pub mod image;
pub mod markdown;
pub mod registry;
pub mod styled;
pub mod text;
pub mod view;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use self::detect::Sniff;
use self::hex::HexSource;
use self::image::ImageDoc;
use self::styled::StyledLine;

/// How many bytes of the head are used for type detection (R-503).
pub const SNIFF_BYTES: usize = 8 * 1024;
/// Text and Markdown read at most this many bytes (larger files are shown truncated).
pub const TEXT_LIMIT: u64 = 8 * 1024 * 1024;
/// Images larger than this are not decoded; metadata only.
pub const IMAGE_LIMIT: u64 = 64 * 1024 * 1024;

/// Built-in viewer kinds. The order of `Registry::candidates` decides the default and Tab cycling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerKind {
    Text,
    Markdown,
    Hex,
    Image,
    /// A viewer added later through `Registry::register` (e.g. an external command provider).
    /// The name is shown in the header. Its documents use the existing `Document` variants.
    Custom(&'static str),
}

/// A loaded document, ready to be shown.
#[derive(Debug)]
pub enum Document {
    /// Text and Markdown: lines already decoded, tab-expanded and sanitized.
    Lines(TextDoc),
    /// Hex: reads windows of the file on demand; the whole file is never loaded.
    Hex(HexSource),
    /// Image: decoded image (or metadata only when decoding is impossible).
    Image(ImageDoc),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDoc {
    pub lines: Vec<StyledLine>,
    /// Name of the detected encoding, shown in the status line (e.g. "UTF-8", "Shift_JIS").
    pub encoding: String,
    /// True when only the first `TEXT_LIMIT` bytes were read.
    pub truncated: bool,
    /// Size of the whole file in bytes.
    pub file_len: u64,
}

/// Name of a viewer kind for the header and messages (a custom viewer shows its own name).
pub fn kind_label(kind: ViewerKind, catalog: &crate::i18n::Catalog) -> String {
    match kind {
        ViewerKind::Text => catalog.text("viewer.kind_text", &[]),
        ViewerKind::Markdown => catalog.text("viewer.kind_markdown", &[]),
        ViewerKind::Hex => catalog.text("viewer.kind_hex", &[]),
        ViewerKind::Image => catalog.text("viewer.kind_image", &[]),
        ViewerKind::Custom(name) => crate::text::sanitize_for_display(name),
    }
}

/// Why a file cannot be viewed. Shown to the user as a Message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    /// Not a regular file (directory, FIFO, device, socket). Never opened, so FIFOs cannot block.
    NotAFile,
    /// Open/read failed. Holds the OS error text.
    Io(String),
}

/// Something that can show a file. Built-in viewers and, later, external command providers
/// (design prompt §25) implement this. Register new ones in `Registry` only.
pub trait ViewerProvider: Send + Sync {
    fn kind(&self) -> ViewerKind;
    /// Priority for this file (higher first), or None when this viewer does not apply.
    fn accepts(&self, sniff: &Sniff) -> Option<u8>;
    /// Loads the document. Runs on the loader thread.
    fn load(&self, path: &Path, sniff: &Sniff) -> Result<Document, ViewError>;
    /// Loads the document decoding text with `encoding` instead of detecting it (`E`, M1-b).
    /// Viewers that do not decode text ignore `encoding`; by default this is `load`.
    fn load_with_encoding(
        &self,
        path: &Path,
        sniff: &Sniff,
        encoding: Option<&'static encoding_rs::Encoding>,
    ) -> Result<Document, ViewError> {
        let _ = encoding;
        self.load(path, sniff)
    }
    /// Like `load_with_encoding`, for viewers that can be slow (external commands): stop and
    /// return an error soon after `cancel` becomes true (C30). Default: `load_with_encoding`.
    fn load_cancellable(
        &self,
        path: &Path,
        sniff: &Sniff,
        encoding: Option<&'static encoding_rs::Encoding>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Document, ViewError> {
        let _ = cancel;
        self.load_with_encoding(path, sniff, encoding)
    }
}

/// Like `open_document_with`, but passes `cancel` to `ViewerProvider::load_cancellable`
/// (C30). When `cancel` is already set or becomes set while a provider runs, the result is an
/// error (`ViewError::Io`); the caller drops it anyway. `open_document_with` is this with a
/// flag that is never set.
pub fn open_document_cancellable(
    registry: &registry::Registry,
    path: &Path,
    kind: Option<ViewerKind>,
    encoding: Option<&'static encoding_rs::Encoding>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Opened, ViewError> {
    use std::sync::atomic::Ordering;

    let cancelled = || ViewError::Io("cancelled".to_string());
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled());
    }
    let metadata = std::fs::metadata(path).map_err(|e| ViewError::Io(e.to_string()))?;
    if !metadata.is_file() {
        return Err(ViewError::NotAFile);
    }
    let sniff = read_sniff(path, metadata.len())?;
    let candidates = registry.candidates(&sniff);
    let start = kind
        .and_then(|k| candidates.iter().position(|c| *c == k))
        .unwrap_or(0);
    let mut last_error = ViewError::Io("no viewer".to_string());
    // The first viewer that failed, reported when a later one is used (C28).
    let mut first_failure: Option<(ViewerKind, String)> = None;
    for &candidate in candidates.iter().skip(start) {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let Some(provider) = registry.provider(candidate) else {
            continue;
        };
        match provider.load_cancellable(path, &sniff, encoding, cancel) {
            Ok(document) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(cancelled());
                }
                let encoding = if matches!(&document, Document::Lines(_)) {
                    encoding
                } else {
                    None
                };
                return Ok(Opened {
                    path: path.to_path_buf(),
                    candidates,
                    kind: candidate,
                    document,
                    encoding,
                    fallback: first_failure,
                });
            }
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(cancelled());
                }
                if first_failure.is_none() {
                    let text = match &error {
                        ViewError::Io(reason) => reason.clone(),
                        ViewError::NotAFile => "not a file".to_string(),
                    };
                    first_failure = Some((candidate, text));
                }
                last_error = error;
            }
        }
    }
    Err(last_error)
}

/// Result of opening a file on the loader thread.
#[derive(Debug)]
pub struct Opened {
    pub path: PathBuf,
    /// Viewer kinds for this file, best first. `kind` is one of them.
    pub candidates: Vec<ViewerKind>,
    pub kind: ViewerKind,
    pub document: Document,
    /// The encoding the user forced with `E`. None when the encoding was detected, and always
    /// None unless `document` is `Document::Lines` (Hex and Image do not decode text).
    pub encoding: Option<&'static encoding_rs::Encoding>,
    /// When the first viewer tried failed and a later candidate was used: that viewer and its
    /// error text (e.g. an external provider that timed out, C28). None otherwise.
    pub fallback: Option<(ViewerKind, String)>,
}

/// Like `open_document`, but Text and Markdown decode with `encoding` when it is Some (M1-b).
///
/// - `encoding: None` behaves exactly like `open_document` (detection, `Opened.encoding` None).
/// - The fallback to the next candidate on a load failure is the same as `open_document`.
/// - `Opened.encoding` is `encoding` only when the loaded document is `Document::Lines`.
pub fn open_document_with(
    registry: &registry::Registry,
    path: &Path,
    kind: Option<ViewerKind>,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> Result<Opened, ViewError> {
    open_document_cancellable(
        registry,
        path,
        kind,
        encoding,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

/// Opens `path` with `kind`, or with the best candidate when `kind` is None.
///
/// - Checks `metadata().is_file()` before opening, so FIFOs and devices are never read (P2-2).
/// - Reads only the head for detection (`SNIFF_BYTES`).
/// - If the requested kind fails to load, falls back to the next candidate (e.g. Image -> Hex).
pub fn open_document(
    registry: &registry::Registry,
    path: &Path,
    kind: Option<ViewerKind>,
) -> Result<Opened, ViewError> {
    open_document_with(registry, path, kind, None)
}

/// Reads the head of the file for detection.
pub fn read_sniff(path: &Path, len: u64) -> Result<Sniff, ViewError> {
    let file = File::open(path).map_err(|e| ViewError::Io(e.to_string()))?;
    let mut head = Vec::with_capacity(SNIFF_BYTES);
    file.take(SNIFF_BYTES as u64)
        .read_to_end(&mut head)
        .map_err(|e| ViewError::Io(e.to_string()))?;
    Ok(Sniff::new(head, len, path.extension()))
}

/// Reads at most `limit` bytes. Returns (bytes, truncated).
pub fn read_limited(path: &Path, limit: u64) -> Result<(Vec<u8>, bool), ViewError> {
    let file = File::open(path).map_err(|e| ViewError::Io(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ViewError::Io(e.to_string()))?;
    let truncated = bytes.len() as u64 > limit;
    bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    Ok((bytes, truncated))
}
