//! Markdown rendering (P2-8).

use fds::viewer::markdown::render;
use fds::viewer::styled::{SpanStyle, StyledLine};

fn texts(lines: &[StyledLine]) -> Vec<String> {
    lines.iter().map(StyledLine::text).collect()
}

fn has(line: &StyledLine, text: &str, style: SpanStyle) -> bool {
    line.spans
        .iter()
        .any(|s| s.text == text && s.style == style)
}

#[test]
fn headings_and_paragraphs() {
    let lines = render("# Title\n\nSome *em* and **strong** and `code`.\nnext line\n\n## Sub");
    assert_eq!(
        texts(&lines),
        [
            "# Title",
            "",
            "Some em and strong and code. next line",
            "",
            "## Sub"
        ]
    );
    assert!(has(&lines[0], "#", SpanStyle::Dim) || has(&lines[0], "# ", SpanStyle::Dim));
    assert!(
        lines[0]
            .spans
            .iter()
            .any(|s| s.text.contains("Title") && s.style == SpanStyle::Heading(1))
    );
    assert!(has(&lines[2], "em", SpanStyle::Emphasis));
    assert!(has(&lines[2], "strong", SpanStyle::Strong));
    assert!(has(&lines[2], "code", SpanStyle::Code));
    assert!(
        lines[4]
            .spans
            .iter()
            .any(|s| s.style == SpanStyle::Heading(2))
    );
}

#[test]
fn lists_nested_ordered_and_tasks() {
    let src = "- a\n  - b\n- c\n\n3. x\n4. y\n\n- [x] done\n- [ ] todo\n";
    let t = texts(&render(src));
    assert_eq!(
        t,
        [
            "• a",
            "  • b",
            "• c",
            "",
            "3. x",
            "4. y",
            "",
            "• [x] done",
            "• [ ] todo"
        ]
    );
}

#[test]
fn code_blocks_quotes_rules() {
    let src = "```rust\nfn main() {\n\tx();\n}\n```\n\n> quoted **bold**\n> more\n\n---\n";
    let lines = render(src);
    let t = texts(&lines);
    assert_eq!(t[0], "    fn main() {");
    assert_eq!(t[1], "            x();");
    assert_eq!(t[2], "    }");
    assert!(
        lines[0]
            .spans
            .iter()
            .all(|s| s.style == SpanStyle::CodeBlock)
    );
    assert_eq!(t[3], "");
    assert!(t[4].starts_with("│ quoted bold"));
    assert!(
        lines[4]
            .spans
            .iter()
            .any(|s| s.text == "bold" && s.style == SpanStyle::Strong)
    );
    assert!(t.iter().any(|l| l == &"─".repeat(40)));
    assert_ne!(
        t.last().map(String::as_str),
        Some(""),
        "no trailing empty line"
    );
}

#[test]
fn links_images_and_html() {
    let lines =
        render("[FDS](https://example.com) ![logo](x.png) <b>raw</b>\n\n<div>\nblock\n</div>\n");
    let t = texts(&lines);
    assert!(t[0].contains("FDS (https://example.com)"));
    assert!(has(&lines[0], "FDS", SpanStyle::Link));
    assert!(t[0].contains("[image: logo]"));
    assert!(t[0].contains("<b>raw</b>"));
    assert!(t.iter().any(|l| l.contains("<div>")));
}

#[test]
fn tables() {
    let t = texts(&render("| a | b |\n|---|---|\n| 1 | 2 |\n"));
    assert_eq!(t[0], "a | b");
    assert_eq!(t[1], "-----");
    assert_eq!(t[2], "1 | 2");
}

#[test]
fn control_chars_are_sanitized_and_empty_input_is_ok() {
    assert!(render("").is_empty());
    let t = texts(&render("evil \u{1b}[2J text"));
    assert!(!t[0].contains('\u{1b}'));
}
