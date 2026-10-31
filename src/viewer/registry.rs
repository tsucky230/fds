//! Viewer registry: the single place where viewers are registered (P2-9, P2-14).

use std::path::Path;

use super::detect::{FileClass, Sniff, classify};
use super::hex::HexSource;
use super::{
    Document, TEXT_LIMIT, TextDoc, ViewError, ViewerKind, ViewerProvider, encoding, image,
    markdown, read_limited, text,
};

pub struct Registry {
    providers: Vec<Box<dyn ViewerProvider>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kinds: Vec<_> = self.providers.iter().map(|p| p.kind()).collect();
        f.debug_struct("Registry")
            .field("providers", &kinds)
            .finish()
    }
}

impl Default for Registry {
    /// Built-in viewers. To add a viewer, implement `ViewerProvider` and push it here.
    fn default() -> Self {
        Self {
            providers: vec![
                Box::new(MarkdownProvider),
                Box::new(TextProvider),
                Box::new(ImageProvider),
                Box::new(HexProvider),
            ],
        }
    }
}

impl Registry {
    /// Adds a provider (used by tests and, later, by external providers).
    /// A provider with the same kind as an existing one replaces it.
    pub fn register(&mut self, provider: Box<dyn ViewerProvider>) {
        self.providers.retain(|p| p.kind() != provider.kind());
        self.providers.push(provider);
    }

    /// Viewer kinds that accept this file, highest priority first. Equal priorities keep
    /// registration order. Never empty for a readable file because Hex accepts everything.
    pub fn candidates(&self, sniff: &Sniff) -> Vec<ViewerKind> {
        let mut scored: Vec<(u8, usize, ViewerKind)> = self
            .providers
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.accepts(sniff).map(|score| (score, i, p.kind())))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, k)| k).collect()
    }

    pub fn provider(&self, kind: ViewerKind) -> Option<&dyn ViewerProvider> {
        self.providers
            .iter()
            .find(|p| p.kind() == kind)
            .map(|p| p.as_ref())
    }
}

fn load_text(
    path: &Path,
    sniff: &Sniff,
    markdown: bool,
    chosen_encoding: Option<&'static encoding_rs::Encoding>,
) -> Result<Document, ViewError> {
    let (bytes, truncated) = read_limited(path, TEXT_LIMIT)?;
    let (decoded, name) = encoding::decode_with(&bytes, chosen_encoding);
    let lines = if markdown {
        markdown::render(&decoded)
    } else {
        text::to_lines(&decoded)
    };
    Ok(Document::Lines(TextDoc {
        lines,
        encoding: name.to_string(),
        truncated,
        file_len: sniff.len,
    }))
}

struct TextProvider;

impl ViewerProvider for TextProvider {
    fn kind(&self) -> ViewerKind {
        ViewerKind::Text
    }
    fn accepts(&self, sniff: &Sniff) -> Option<u8> {
        match classify(sniff) {
            FileClass::Text => Some(100),
            FileClass::Markdown => Some(50),
            _ => None,
        }
    }
    fn load(&self, path: &Path, sniff: &Sniff) -> Result<Document, ViewError> {
        load_text(path, sniff, false, None)
    }
    fn load_with_encoding(
        &self,
        path: &Path,
        sniff: &Sniff,
        encoding: Option<&'static encoding_rs::Encoding>,
    ) -> Result<Document, ViewError> {
        load_text(path, sniff, false, encoding)
    }
}

struct MarkdownProvider;

impl ViewerProvider for MarkdownProvider {
    fn kind(&self) -> ViewerKind {
        ViewerKind::Markdown
    }
    fn accepts(&self, sniff: &Sniff) -> Option<u8> {
        (classify(sniff) == FileClass::Markdown).then_some(100)
    }
    fn load(&self, path: &Path, sniff: &Sniff) -> Result<Document, ViewError> {
        load_text(path, sniff, true, None)
    }
    fn load_with_encoding(
        &self,
        path: &Path,
        sniff: &Sniff,
        encoding: Option<&'static encoding_rs::Encoding>,
    ) -> Result<Document, ViewError> {
        load_text(path, sniff, true, encoding)
    }
}

struct ImageProvider;

impl ViewerProvider for ImageProvider {
    fn kind(&self) -> ViewerKind {
        ViewerKind::Image
    }
    fn accepts(&self, sniff: &Sniff) -> Option<u8> {
        matches!(classify(sniff), FileClass::Image(_)).then_some(100)
    }
    fn load(&self, path: &Path, sniff: &Sniff) -> Result<Document, ViewError> {
        image::load(path, sniff)
            .map(Document::Image)
            .map_err(|e| ViewError::Io(e.to_string()))
    }
}

struct HexProvider;

impl ViewerProvider for HexProvider {
    fn kind(&self) -> ViewerKind {
        ViewerKind::Hex
    }
    fn accepts(&self, _sniff: &Sniff) -> Option<u8> {
        Some(10)
    }
    fn load(&self, path: &Path, sniff: &Sniff) -> Result<Document, ViewError> {
        HexSource::open(path, sniff.len)
            .map(Document::Hex)
            .map_err(|e| ViewError::Io(e.to_string()))
    }
}
