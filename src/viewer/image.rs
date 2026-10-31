//! Image loading (P2-11, R-507). Decoding happens on the loader thread; drawing (ratatui-image)
//! happens in the UI layer. Nothing here talks to the terminal.

use std::path::Path;

use image::{DynamicImage, ImageReader};

use super::detect::{FileClass, ImageFormat, Sniff, classify};

/// Images are downscaled so that neither side exceeds this before being handed to the UI.
pub const MAX_SIDE: u32 = 2048;
/// Refuse to decode images whose declared size exceeds this (decompression-bomb guard).
pub const MAX_DECODE_SIDE: u32 = 16_384;

#[derive(Debug)]
pub struct ImageDoc {
    pub format: Option<ImageFormat>,
    /// Original size in pixels, when the header could be read.
    pub dimensions: Option<(u32, u32)>,
    pub file_len: u64,
    /// Decoded (and possibly downscaled) image. None means "metadata only".
    pub image: Option<DynamicImage>,
    /// Why the image is not shown (decode error, too large). Plain text for the status line.
    pub problem: Option<String>,
}

/// Loads an image. Decode problems are not errors: they produce a metadata-only document
/// (R-502 fallback, P2-11). Only failing to open the file is an error.
pub fn load(path: &Path, sniff: &Sniff) -> std::io::Result<ImageDoc> {
    let format = match classify(sniff) {
        FileClass::Image(f) => Some(f),
        _ => None,
    };
    let mut doc = ImageDoc {
        format,
        dimensions: None,
        file_len: sniff.len,
        image: None,
        problem: None,
    };
    if sniff.len > super::IMAGE_LIMIT {
        doc.problem = Some(format!("file too large ({} bytes)", sniff.len));
        return Ok(doc);
    }
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    let mut reader = reader;
    reader.limits(limits);
    match reader.decode() {
        Ok(img) => {
            doc.dimensions = Some((img.width(), img.height()));
            doc.image = Some(if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
                img.thumbnail(MAX_SIDE, MAX_SIDE)
            } else {
                img
            });
        }
        Err(e) => {
            doc.dimensions = image::image_dimensions(path).ok();
            doc.problem = Some(e.to_string());
        }
    }
    Ok(doc)
}

/// Image protocol preference from config `[viewer] image_protocol`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProtocolSetting {
    #[default]
    Auto,
    Kitty,
    Sixel,
    Iterm2,
    Halfblocks,
    None,
}

impl ProtocolSetting {
    /// Parses the config value (case-insensitive). Unknown values fall back to Auto.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::to_ascii_lowercase).as_deref() {
            Some("kitty") => Self::Kitty,
            Some("sixel") => Self::Sixel,
            Some("iterm2") => Self::Iterm2,
            Some("halfblocks") => Self::Halfblocks,
            Some("none") => Self::None,
            _ => Self::Auto,
        }
    }
}

/// The protocol to draw with, decided without talking to the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// Kitty, Sixel, Iterm2 or Halfblocks.
    Protocol(ProtocolSetting),
    /// Never draw images; show metadata only.
    MetadataOnly,
}

/// Chooses the image protocol from the config setting and environment variables (P2-11, P2-15).
///
/// The terminal is deliberately not queried: ratatui-image's stdio query reads stdin on a thread
/// until the terminal answers, and in terminals that never answer (e.g. inside tmux) that thread
/// keeps stealing key presses and cursor-position replies forever (ADR 0002).
///
/// Order:
/// 1. Config: Kitty / Sixel / Iterm2 / Halfblocks are used as is; None -> MetadataOnly.
/// 2. Auto inside tmux (`TMUX` set) -> Halfblocks.
/// 3. Auto: `KITTY_WINDOW_ID` set, `TERM` containing "kitty", or `TERM_PROGRAM` = "ghostty" -> Kitty;
///    `TERM_PROGRAM` = "iTerm.app" or "WezTerm" -> Iterm2.
/// 4. Auto in Windows Terminal (`WT_SESSION` set) -> Sixel (confirmed in manual check M17; very old
///    Windows Terminal versions without Sixel support need `image_protocol = "halfblocks"`).
/// 5. Otherwise Halfblocks (other Sixel terminals only when configured; they cannot be detected safely).
pub fn resolve_protocol(
    setting: ProtocolSetting,
    env: &dyn Fn(&str) -> Option<String>,
) -> Resolved {
    match setting {
        ProtocolSetting::None => return Resolved::MetadataOnly,
        ProtocolSetting::Auto => {}
        fixed => return Resolved::Protocol(fixed),
    }
    let set = |key: &str| env(key).is_some_and(|v| !v.is_empty());
    if set("TMUX") {
        return Resolved::Protocol(ProtocolSetting::Halfblocks);
    }
    let term = env("TERM").unwrap_or_default();
    let program = env("TERM_PROGRAM").unwrap_or_default();
    if set("KITTY_WINDOW_ID") || term.contains("kitty") || program == "ghostty" {
        return Resolved::Protocol(ProtocolSetting::Kitty);
    }
    if program == "iTerm.app" || program == "WezTerm" {
        return Resolved::Protocol(ProtocolSetting::Iterm2);
    }
    if set("WT_SESSION") {
        return Resolved::Protocol(ProtocolSetting::Sixel);
    }
    Resolved::Protocol(ProtocolSetting::Halfblocks)
}

/// Font cell size in pixels from the window size (`crossterm::terminal::window_size`).
/// Falls back to 8x16 when the pixel size is unknown (0, e.g. on Windows) or smaller than a cell.
pub fn font_size(columns: u16, rows: u16, width_px: u16, height_px: u16) -> (u16, u16) {
    const FALLBACK: (u16, u16) = (8, 16);
    if columns == 0 || rows == 0 {
        return FALLBACK;
    }
    let w = width_px / columns;
    let h = height_px / rows;
    if w == 0 || h == 0 { FALLBACK } else { (w, h) }
}
