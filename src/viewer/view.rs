//! Scroll state of an open viewer (P2-6, P2-10, P2-12). Pure logic over a `Document`;
//! the only I/O is `HexSource` reading windows of the file.

use std::path::PathBuf;

use super::styled::StyledLine;
use super::{Document, Opened, ViewerKind};
use crate::text::display_width;

/// Columns moved by one horizontal scroll step.
pub const HSCROLL_STEP: usize = 8;

#[derive(Debug)]
pub struct ViewerState {
    pub path: PathBuf,
    pub candidates: Vec<ViewerKind>,
    pub kind: ViewerKind,
    pub doc: Document,
    /// Encoding forced with `E` (None: detected). Copied from `Opened::encoding`.
    pub encoding: Option<&'static encoding_rs::Encoding>,
    /// Wrap long lines (Lines documents only). Default true.
    pub wrap: bool,
    /// First visible display column when wrap is off.
    pub hscroll: usize,
    /// Lines documents: first visible logical line.
    pub top_line: usize,
    /// Lines documents with wrap on: first visible wrapped row inside `top_line`.
    pub top_row: usize,
    /// Hex documents: first visible hex row.
    pub hex_top: u64,
    /// Size of the text area at the last `set_viewport` (0 before the first render).
    width: usize,
    height: usize,
    max_line_width: usize,
}

impl ViewerState {
    pub fn new(opened: Opened) -> Self {
        let max_line_width = match &opened.document {
            Document::Lines(doc) => doc
                .lines
                .iter()
                .map(|line| {
                    line.spans.iter().fold(0usize, |width, span| {
                        width.saturating_add(display_width(&span.text))
                    })
                })
                .max()
                .unwrap_or(0),
            _ => 0,
        };
        Self {
            path: opened.path,
            candidates: opened.candidates,
            kind: opened.kind,
            doc: opened.document,
            encoding: opened.encoding,
            wrap: true,
            hscroll: 0,
            top_line: 0,
            top_row: 0,
            hex_top: 0,
            width: 0,
            height: 0,
            max_line_width,
        }
    }

    /// The candidate after the current kind (wrapping around). None if there is only one.
    pub fn next_kind(&self) -> Option<ViewerKind> {
        if self.candidates.len() < 2 {
            return None;
        }
        let i = self
            .candidates
            .iter()
            .position(|k| *k == self.kind)
            .unwrap_or(0);
        self.candidates
            .get((i + 1) % self.candidates.len())
            .copied()
    }

    /// Records the text area size. Call before scrolling commands take effect and on every render.
    ///
    /// Behavior:
    /// - Stores width/height.
    /// - Lines + wrap: keeps `top_line`; clamps `top_row` to `row_count(top_line) - 1`
    ///   (so after a resize the same logical line stays at the top, P2-12).
    /// - Then clamps the position so the view does not start past the end (same rule as `end`).
    pub fn set_viewport(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;

        if let Document::Lines(doc) = &self.doc
            && self.wrap
            && let Some(line) = doc.lines.get(self.top_line)
        {
            let count = super::text::row_count(line, self.width);
            self.top_row = self.top_row.min(count.saturating_sub(1));
        }

        self.clamp_position();
    }

    /// Hex bytes per row for the current width (see `hex::bytes_per_row`).
    pub fn hex_per_row(&self) -> usize {
        super::hex::bytes_per_row(u16::try_from(self.width).unwrap_or(u16::MAX))
    }

    /// Scrolls down by `n` visual rows.
    ///
    /// - Lines + wrap: moves through wrapped rows (row by row across logical lines).
    /// - Lines + no wrap: moves `n` logical lines.
    /// - Hex: moves `n` hex rows.
    /// - Image: no-op.
    /// - Never goes past the position of `end()`; never panics on empty documents.
    pub fn scroll_down(&mut self, n: usize) {
        if let Document::Lines(doc) = &self.doc {
            let height = self.height.max(1);
            if self.wrap {
                let mut remaining = n;
                while let Some(line) = doc.lines.get(self.top_line) {
                    let count = super::text::row_count(line, self.width);
                    self.top_row = self.top_row.min(count.saturating_sub(1));
                    let available = count.saturating_sub(self.top_row);
                    if remaining < available {
                        self.top_row = self.top_row.saturating_add(remaining);
                        break;
                    }
                    remaining = remaining.saturating_sub(available);
                    self.top_line = self.top_line.saturating_add(1);
                    self.top_row = 0;
                }
                self.clamp_position();
            } else {
                let max_line = doc.lines.len().saturating_sub(height);
                self.top_line = self.top_line.saturating_add(n).min(max_line);
                self.top_row = 0;
            }
        } else if let Document::Hex(source) = &mut self.doc {
            let per_row = super::hex::bytes_per_row(u16::try_from(self.width).unwrap_or(u16::MAX));
            let height = self.height.max(1) as u64;
            let total = source.rows(per_row);
            let max_top = total.saturating_sub(height);
            let n_u64 = u64::try_from(n).unwrap_or(u64::MAX);
            self.hex_top = self.hex_top.saturating_add(n_u64).min(max_top);
        }
        // Image: no-op.
    }

    /// Scrolls up by `n` visual rows (same units as `scroll_down`), stopping at the top.
    pub fn scroll_up(&mut self, n: usize) {
        if let Document::Lines(doc) = &self.doc {
            if self.wrap {
                self.top_line = self.top_line.min(doc.lines.len().saturating_sub(1));
                self.top_row = doc.lines.get(self.top_line).map_or(0, |line| {
                    self.top_row
                        .min(super::text::row_count(line, self.width).saturating_sub(1))
                });
                let mut remaining = n;
                while remaining > self.top_row && self.top_line > 0 {
                    remaining = remaining.saturating_sub(self.top_row).saturating_sub(1);
                    self.top_line = self.top_line.saturating_sub(1);
                    self.top_row = doc.lines.get(self.top_line).map_or(0, |line| {
                        super::text::row_count(line, self.width).saturating_sub(1)
                    });
                }
                self.top_row = self.top_row.saturating_sub(remaining);
            } else {
                self.top_line = self.top_line.saturating_sub(n);
                self.top_row = 0;
            }
        } else if let Document::Hex(_) = &self.doc {
            let n_u64 = u64::try_from(n).unwrap_or(u64::MAX);
            self.hex_top = self.hex_top.saturating_sub(n_u64);
        }
        // Image: no-op.
    }

    /// One page = the last known height (at least 1).
    pub fn page_down(&mut self) {
        self.scroll_down(self.height.max(1));
    }

    pub fn page_up(&mut self) {
        self.scroll_up(self.height.max(1));
    }

    /// Goes to the top.
    pub fn home(&mut self) {
        self.top_line = 0;
        self.top_row = 0;
        self.hex_top = 0;
    }

    /// Goes to the last page: the last row of the document is on the last visible row
    /// (or the top when the document is shorter than the height). Uses the last known viewport;
    /// before the first render (height 0) it treats the height as 1.
    pub fn end(&mut self) {
        if let Document::Lines(doc) = &self.doc {
            let height = self.height.max(1);
            if self.wrap {
                (self.top_line, self.top_row) = self.end_position();
            } else {
                self.top_line = doc.lines.len().saturating_sub(height);
                self.top_row = 0;
            }
        } else if let Document::Hex(source) = &mut self.doc {
            let per_row = super::hex::bytes_per_row(u16::try_from(self.width).unwrap_or(u16::MAX));
            let height = self.height.max(1) as u64;
            let total = source.rows(per_row);
            self.hex_top = total.saturating_sub(height);
        }
        // Image: no-op.
    }

    /// Toggles wrapping. Resets `hscroll` and `top_row`, keeps `top_line`.
    pub fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
        self.hscroll = 0;
        self.top_row = 0;
    }

    /// Horizontal scroll when wrap is off (no-op when wrap is on). Right stops so that at least one
    /// column of the widest line stays visible; left stops at 0.
    pub fn scroll_right(&mut self) {
        if self.wrap {
            return;
        }
        let Document::Lines(_) = &self.doc else {
            return;
        };
        let max_hscroll = self.max_line_width.saturating_sub(self.width);
        self.hscroll = self.hscroll.saturating_add(HSCROLL_STEP).min(max_hscroll);
    }

    pub fn scroll_left(&mut self) {
        if !self.wrap {
            self.hscroll = self.hscroll.saturating_sub(HSCROLL_STEP);
        }
    }

    /// Rows to draw for a Lines document: at most `height` rows starting at the current position,
    /// wrapped (`wrap_line`) or sliced (`slice_columns` from `hscroll`) to `width`.
    /// Empty for non-Lines documents.
    pub fn visible_lines(&self) -> Vec<StyledLine> {
        let Document::Lines(doc) = &self.doc else {
            return Vec::new();
        };
        if self.height == 0 {
            return Vec::new();
        }

        let mut out = Vec::new();

        if self.wrap {
            let mut line_idx = self.top_line;
            let mut skip = self.top_row;
            while out.len() < self.height && line_idx < doc.lines.len() {
                let rows = super::text::wrap_line(&doc.lines[line_idx], self.width);
                for row in rows.into_iter().skip(skip) {
                    if out.len() >= self.height {
                        break;
                    }
                    out.push(row);
                }
                skip = 0;
                line_idx += 1;
            }
        } else {
            for line in doc.lines.iter().skip(self.top_line).take(self.height) {
                out.push(super::text::slice_columns(line, self.hscroll, self.width));
            }
        }

        out
    }

    /// Rows to draw for a Hex document: (offset, bytes) for at most `height` rows from `hex_top`.
    /// Empty for non-Hex documents.
    pub fn visible_hex(&mut self) -> Vec<(u64, Vec<u8>)> {
        let Document::Hex(source) = &mut self.doc else {
            return Vec::new();
        };
        if self.height == 0 {
            return Vec::new();
        }

        let per_row = super::hex::bytes_per_row(u16::try_from(self.width).unwrap_or(u16::MAX));
        let per_row_u64 = u64::try_from(per_row).unwrap_or(u64::MAX);
        let total_rows = source.rows(per_row);
        let mut out = Vec::new();

        for i in 0..self.height {
            let row_idx = self.hex_top.saturating_add(i as u64);
            if row_idx >= total_rows {
                break;
            }
            let offset = row_idx.saturating_mul(per_row_u64);
            let bytes = source.row(row_idx, per_row);
            out.push((offset, bytes));
        }

        out
    }

    pub fn viewport(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    // -----------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------

    /// Find the last page by counting backward through at most one viewport of lines.
    /// Empty and short documents start at (0, 0); no global row total is needed.
    fn end_position(&self) -> (usize, usize) {
        let Document::Lines(doc) = &self.doc else {
            return (0, 0);
        };
        let mut remaining = self.height.max(1);
        for (index, line) in doc.lines.iter().enumerate().rev() {
            let count = super::text::row_count(line, self.width);
            if count >= remaining {
                return (index, count.saturating_sub(remaining));
            }
            remaining = remaining.saturating_sub(count);
        }
        (0, 0)
    }
    /// Clamp the current position so the viewport does not start past the end.
    fn clamp_position(&mut self) {
        if let Document::Lines(doc) = &self.doc {
            let height = self.height.max(1);
            if self.wrap {
                let end = self.end_position();
                if (self.top_line, self.top_row) > end {
                    (self.top_line, self.top_row) = end;
                }
            } else {
                let max_line = doc.lines.len().saturating_sub(height);
                self.top_line = self.top_line.min(max_line);
                self.top_row = 0;
            }
            return;
        }

        if let Document::Hex(_) = &self.doc {
            let per_row = super::hex::bytes_per_row(u16::try_from(self.width).unwrap_or(u16::MAX));
            let height = self.height.max(1) as u64;
            let max_top = match &mut self.doc {
                Document::Hex(source) => source.rows(per_row).saturating_sub(height),
                _ => 0,
            };
            self.hex_top = self.hex_top.min(max_top);
        }
        // Image: nothing to clamp.
    }
}
