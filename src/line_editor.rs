//! One-line text input for Rename / mkdir / search (P3-12, P3-14). Pure logic.
//!
//! The cursor is a grapheme index (0 = before the first grapheme), so it never splits a
//! combining sequence or an emoji.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEditor {
    text: String,
    /// Grapheme index of the cursor, 0..=grapheme count.
    cursor: usize,
}

impl LineEditor {
    /// Starts with `text` and the cursor at the end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.graphemes(true).count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cursor position as a grapheme index.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Inserts `c` at the cursor and moves the cursor after it. Control characters
    /// (`char::is_control`) are ignored.
    /// If `c` combines with the previous grapheme (e.g. a combining accent), the grapheme count
    /// may not grow; the cursor then stays right after the modified grapheme.
    pub fn insert(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        let byte = self.byte_at(self.cursor);
        self.text.insert(byte, c);
        let end = byte.saturating_add(c.len_utf8());
        self.cursor = self
            .text
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < end)
            .count();
    }

    /// Deletes the grapheme before the cursor (Backspace). No-op at the start.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor = self
            .text
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < start)
            .count();
    }

    /// Deletes the grapheme at the cursor (Delete). No-op at the end.
    pub fn delete(&mut self) {
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor.saturating_add(1));
        self.text.replace_range(start..end, "");
        self.cursor = self.cursor.min(self.text.graphemes(true).count());
    }

    /// Moves one grapheme left / right, stopping at the ends.
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = self
            .cursor
            .saturating_add(1)
            .min(self.text.graphemes(true).count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.graphemes(true).count();
    }

    /// Places the cursor before the extension ("name|.txt"), used when starting a Rename so that
    /// typing replaces the name, not the extension. Same extension rule as
    /// `ops::name::unique_name` (a leading dot is not an extension). Without an extension: end.
    pub fn cursor_before_extension(&mut self) {
        let byte = self
            .text
            .rfind('.')
            .filter(|&i| i > 0)
            .unwrap_or(self.text.len());
        self.cursor = self
            .text
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < byte)
            .count();
    }

    fn byte_at(&self, cursor: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }
}
