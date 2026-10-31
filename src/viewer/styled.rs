//! Styled text lines shared by the Text and Markdown viewers. Pure data, no terminal types.

/// Semantic style of a span. The UI maps these to colors/modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpanStyle {
    Plain,
    /// Heading text; the value is the level (1-6).
    Heading(u8),
    Emphasis,
    Strong,
    /// Inline code.
    Code,
    /// A line inside a fenced/indented code block.
    CodeBlock,
    /// Quote prefix and text.
    Quote,
    /// Link text.
    Link,
    /// Secondary text: URLs, list markers, rules, table separators.
    Dim,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

/// One logical line (before wrapping).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyledLine {
    pub spans: Vec<Span>,
}

impl StyledLine {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span {
                text: text.into(),
                style: SpanStyle::Plain,
            }],
        }
    }

    /// Concatenated text of all spans (used by tests and search).
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}
