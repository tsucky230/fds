//! Markdown rendering to styled lines (R-505, P2-8). Pure logic using pulldown-cmark.

use super::styled::{Span, SpanStyle, StyledLine};
use crate::text::sanitize_for_display;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Renders Markdown source into logical lines for the terminal.
///
/// Behavior (lines are not wrapped here; the viewer wraps them later):
/// - Enable pulldown-cmark options: tables, strikethrough, tasklists.
/// - Heading level N: one line "<N times '#'> <text>". The '#' marks use `Dim`, the text uses
///   `Heading(N)`. An empty line follows every heading.
/// - Paragraph: its inline content on one logical line (soft breaks become a space, hard breaks
///   start a new line). An empty line follows every paragraph.
/// - Emphasis -> `Emphasis`, strong -> `Strong`, inline code -> `Code` (without backticks),
///   strikethrough text is kept as `Plain`.
/// - Link: the text as `Link`, then " (<url>)" as `Dim`. Autolinks where text == url: just `Link`.
/// - Image: "[image: <alt>]" as `Dim` (alt may be empty: "[image: ]").
/// - Unordered list item: indent 2 spaces per nesting level (outer level 0), then "• " (`Dim`),
///   then the item text. Ordered item: "<n>. " (`Dim`) with n counting from the list's start.
///   Task items: "[x] " / "[ ] " after the marker. An empty line follows the outermost list.
/// - Code block (fenced or indented): each source line becomes "    <line>" as `CodeBlock`
///   (TABs expanded with `super::text::expand_line`). An empty line follows the block.
/// - Block quote: every line inside is prefixed with "│ " (`Quote`); nested quotes repeat it.
///   Text inside keeps its styles.
/// - Thematic break: one line of 40 '─' (`Dim`), then an empty line.
/// - Table: one line per row, cells joined with " | " (`Dim`); after the header row a line of
///   '-' as `Dim` with the same length as the header line. An empty line follows the table.
/// - HTML (block or inline): the raw HTML text as `Plain` (block HTML keeps its own lines).
/// - The result never ends with an empty line (trailing empty lines are removed).
/// - All text is passed through `crate::text::sanitize_for_display` (control chars -> '?').
pub fn render(source: &str) -> Vec<StyledLine> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut renderer = Renderer::default();
    let mut events = Parser::new_ext(source, options);
    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::Image { .. }) => {
                // Consume the entire alt text, discarding its inline markup.
                let mut alt = String::new();
                let mut depth = 1usize;
                for event in events.by_ref() {
                    match event {
                        Event::Start(Tag::Image { .. }) => depth += 1,
                        Event::End(TagEnd::Image) => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Event::Text(text) | Event::Code(text) | Event::InlineHtml(text) => {
                            alt.push_str(&text);
                        }
                        Event::SoftBreak | Event::HardBreak => alt.push(' '),
                        _ => {}
                    }
                }
                renderer.push(&format!("[image: {alt}]"), SpanStyle::Dim);
            }
            Event::Start(tag) => renderer.start(tag),
            Event::End(tag) => renderer.end(tag),
            Event::Text(text) => {
                if let Some(code) = &mut renderer.code {
                    code.push_str(&text);
                } else {
                    renderer.push(&text, renderer.style());
                }
            }
            Event::Code(text) => renderer.push(&text, SpanStyle::Code),
            Event::InlineHtml(text) => renderer.push(&text, SpanStyle::Plain),
            Event::Html(text) => {
                for line in super::text::split_lines(&text) {
                    renderer.push(line, SpanStyle::Plain);
                    renderer.finish_line();
                }
            }
            Event::SoftBreak => renderer.push(" ", renderer.style()),
            Event::HardBreak => renderer.finish_line(),
            Event::Rule => {
                renderer.flush();
                renderer.push(&"─".repeat(40), SpanStyle::Dim);
                renderer.blank();
            }
            Event::TaskListMarker(checked) => {
                renderer.push(if checked { "[x] " } else { "[ ] " }, SpanStyle::Dim);
            }
            _ => {}
        }
    }
    renderer.flush();
    while renderer
        .lines
        .last()
        .is_some_and(|line| line.spans.is_empty())
    {
        renderer.lines.pop();
    }
    renderer.lines
}

#[derive(Default)]
struct Renderer {
    lines: Vec<StyledLine>,
    line: StyledLine,
    styles: Vec<SpanStyle>,
    lists: Vec<Option<u64>>,
    links: Vec<(String, String)>,
    quotes: usize,
    code: Option<String>,
    cells: usize,
}

impl Renderer {
    fn style(&self) -> SpanStyle {
        self.styles.last().copied().unwrap_or(SpanStyle::Plain)
    }

    // All emitted text, including generated markers, passes through this boundary.
    fn push(&mut self, text: &str, style: SpanStyle) {
        let text = sanitize_for_display(text);
        for (_, label) in &mut self.links {
            label.push_str(&text);
        }
        if self.line.spans.is_empty() && self.quotes > 0 {
            self.line.spans.push(Span {
                text: sanitize_for_display(&"│ ".repeat(self.quotes)),
                style: SpanStyle::Quote,
            });
        }
        if !text.is_empty() {
            self.line.spans.push(Span { text, style });
        }
    }

    fn finish_line(&mut self) {
        self.push("", SpanStyle::Plain);
        self.lines.push(std::mem::take(&mut self.line));
    }

    fn flush(&mut self) {
        if !self.line.spans.is_empty() {
            self.finish_line();
        }
    }

    fn blank(&mut self) {
        self.flush();
        self.finish_line();
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.push(&format!("{} ", "#".repeat(level as usize)), SpanStyle::Dim);
                self.styles.push(SpanStyle::Heading(level as u8));
            }
            Tag::Emphasis => self.styles.push(SpanStyle::Emphasis),
            Tag::Strong => self.styles.push(SpanStyle::Strong),
            Tag::Strikethrough => self.styles.push(SpanStyle::Plain),
            Tag::Link { dest_url, .. } => {
                self.links
                    .push((sanitize_for_display(&dest_url), String::new()));
                self.styles.push(SpanStyle::Link);
            }
            Tag::List(start) => {
                self.flush();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let indent = "  ".repeat(self.lists.len().saturating_sub(1));
                let marker = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}. ");
                        *number = number.saturating_add(1);
                        marker
                    }
                    _ => "• ".to_owned(),
                };
                self.push(&format!("{indent}{marker}"), SpanStyle::Dim);
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quotes += 1;
            }
            Tag::Table(_) | Tag::HtmlBlock => self.flush(),
            Tag::TableHead | Tag::TableRow => {
                self.flush();
                self.cells = 0;
            }
            Tag::TableCell => {
                if self.cells > 0 {
                    self.push(" | ", SpanStyle::Dim);
                }
                self.cells += 1;
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.styles.pop();
                self.blank();
            }
            TagEnd::Paragraph => self.blank(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                if let Some((url, label)) = self.links.pop()
                    && label != url
                {
                    self.push(&format!(" ({url})"), SpanStyle::Dim);
                }
            }
            TagEnd::Item => self.flush(),
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    for line in super::text::split_lines(&code) {
                        self.push(
                            &format!("    {}", super::text::expand_line(line)),
                            SpanStyle::CodeBlock,
                        );
                        self.finish_line();
                    }
                }
                self.blank();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quotes = self.quotes.saturating_sub(1);
            }
            TagEnd::TableHead => {
                let length = self.line.text().chars().count();
                self.finish_line();
                self.push(&"-".repeat(length), SpanStyle::Dim);
                self.finish_line();
            }
            TagEnd::TableRow => self.finish_line(),
            TagEnd::Table => self.blank(),
            TagEnd::HtmlBlock => self.flush(),
            _ => {}
        }
    }
}
