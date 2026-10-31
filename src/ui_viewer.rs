//! Viewer screen rendering (P2-1, P2-5, P2-7, P2-10, P2-11). Called from `ui::render`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use crate::i18n::Catalog;
use crate::layout::Regions;
use crate::text::{sanitize_for_display, truncate_to_width};
use crate::viewer::hex::{format_row, offset_digits};
use crate::viewer::styled::{SpanStyle, StyledLine};
use crate::viewer::view::ViewerState;
use crate::viewer::{Document, ViewerKind};

/// Image drawing state owned by the event loop (terminal specific, so not part of App).
#[derive(Default)]
pub struct ImageSlot {
    /// Protocol state for the image currently shown.
    pub protocol: Option<StatefulProtocol>,
    /// Identifies which viewer the protocol belongs to (path + kind as text).
    pub owner: Option<String>,
    /// Name of the protocol in use, for the status line ("sixel", "kitty", "iterm2", "halfblocks").
    pub protocol_name: Option<&'static str>,
    /// The preview pane's picture (R-200-10), built by `ui_preview::prepare_preview_image`.
    pub preview_protocol: Option<StatefulProtocol>,
    /// The item the pane's picture belongs to.
    pub preview_owner: Option<std::path::PathBuf>,
    /// The pane's picture uses a graphics protocol (not halfblocks): dropping it needs a clear.
    pub preview_graphics: bool,
}

/// Layout and overlays decided by `ui::render`.
pub struct Screen<'a> {
    /// [header, body, status, key guide].
    pub rows: &'a [Rect],
    pub regions: Regions,
    /// Show the viewer key list instead of the document.
    pub help: bool,
    /// A status message that overrides the viewer's own status text.
    pub status: Option<String>,
    /// `[viewer] vi_keys`: which key list the help shows.
    pub vi_keys: bool,
}

pub fn render(
    frame: &mut Frame,
    viewer: &mut ViewerState,
    catalog: &Catalog,
    images: &mut ImageSlot,
    screen: Screen<'_>,
) {
    let Screen {
        rows,
        regions,
        help,
        status,
        vi_keys,
    } = screen;
    let width = rows[1].width;
    if regions.show_path {
        let name = viewer
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let header = format!(" {name}  [{}]", kind_label(viewer.kind, catalog));
        let line = Line::styled(
            fit(&header, width),
            Style::new().add_modifier(Modifier::BOLD),
        );
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    let body = rows[1];
    viewer.set_viewport(usize::from(body.width), usize::from(body.height));
    if help {
        render_help(frame, body, catalog, vi_keys);
    } else {
        render_body(frame, viewer, catalog, images, body);
    }

    if regions.show_status {
        let text = status.unwrap_or_else(|| status_text(viewer, catalog, images));
        let bar = Paragraph::new(fit(&format!(" {text}"), width))
            .style(Style::new().add_modifier(Modifier::REVERSED));
        frame.render_widget(bar, rows[2]);
    }
    if regions.show_key_guide {
        let key = if regions.short_key_guide {
            "viewer.guide_short"
        } else {
            "viewer.guide_full"
        };
        frame.render_widget(Paragraph::new(fit(&catalog.text(key, &[]), width)), rows[3]);
    }
}

fn render_body(
    frame: &mut Frame,
    viewer: &mut ViewerState,
    catalog: &Catalog,
    images: &mut ImageSlot,
    area: Rect,
) {
    match &viewer.doc {
        Document::Lines(_) => {
            let lines: Vec<Line> = viewer.visible_lines().iter().map(to_line).collect();
            frame.render_widget(Paragraph::new(lines), area);
        }
        Document::Hex(source) => {
            let digits = offset_digits(source.len);
            let per_row = viewer.hex_per_row();
            let lines: Vec<Line> = viewer
                .visible_hex()
                .iter()
                .map(|(offset, bytes)| {
                    Line::from(fit(
                        &format_row(*offset, bytes, per_row, digits),
                        area.width,
                    ))
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        }
        Document::Image(doc) => {
            if let (Some(protocol), Some(_)) = (images.protocol.as_mut(), doc.image.as_ref()) {
                // Scale (not Fit) so small images fill the area like a regular image viewer.
                let widget = StatefulImage::default().resize(Resize::Scale(None));
                frame.render_stateful_widget(widget, area, protocol);
            } else {
                let mut lines = vec![Line::from(fit(&image_summary(viewer, catalog), area.width))];
                if let Document::Image(doc) = &viewer.doc
                    && let Some(problem) = &doc.problem
                {
                    let text = catalog.text("viewer.image_problem", &[("reason", problem)]);
                    lines.push(Line::from(fit(&text, area.width)));
                }
                lines.push(Line::from(fit(
                    &catalog.text("viewer.image_hint", &[]),
                    area.width,
                )));
                frame.render_widget(Paragraph::new(lines), area);
            }
        }
    }
}

fn render_help(frame: &mut Frame, area: Rect, catalog: &Catalog, vi_keys: bool) {
    let mut lines = vec![Line::from(fit(
        &format!(
            " FDS {}  {}",
            env!("CARGO_PKG_VERSION"),
            catalog.text("help.title", &[])
        ),
        area.width,
    ))];
    for (keys, action) in crate::command::viewer_key_help(vi_keys) {
        let action = catalog.text(action, &[]);
        lines.push(Line::from(fit(
            &format!("  {keys:<14} {action}"),
            area.width,
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn status_text(viewer: &ViewerState, catalog: &Catalog, images: &ImageSlot) -> String {
    match &viewer.doc {
        Document::Lines(doc) => {
            let line = (viewer.top_line + 1)
                .min(doc.lines.len().max(1))
                .to_string();
            let total = doc.lines.len().to_string();
            let encoding = if viewer.encoding.is_some() {
                catalog.text("viewer.encoding_manual", &[("encoding", &doc.encoding)])
            } else {
                doc.encoding.clone()
            };
            let mut text = catalog.text(
                "viewer.status_text",
                &[("encoding", &encoding), ("line", &line), ("total", &total)],
            );
            if doc.truncated {
                text.push_str("  ");
                text.push_str(&catalog.text("viewer.truncated", &[]));
            }
            if !viewer.wrap {
                text.push_str("  ");
                text.push_str(&catalog.text("viewer.nowrap", &[]));
            }
            text
        }
        Document::Hex(source) => {
            let offset = viewer.hex_top.saturating_mul(viewer.hex_per_row() as u64);
            let offset = format!("{offset:X}");
            let size = source.len.to_string();
            catalog.text("viewer.status_hex", &[("offset", &offset), ("size", &size)])
        }
        Document::Image(_) => {
            let mut text = image_summary(viewer, catalog);
            if let Some(name) = images.protocol_name {
                text.push_str("  ");
                text.push_str(name);
            }
            text
        }
    }
}

fn image_summary(viewer: &ViewerState, catalog: &Catalog) -> String {
    let Document::Image(doc) = &viewer.doc else {
        return String::new();
    };
    image_status(doc.format, doc.dimensions, doc.file_len, catalog)
}

/// "PNG  40x20  2048 bytes" (unknown parts as "?"). Also used by the preview pane.
pub(crate) fn image_status(
    format: Option<crate::viewer::detect::ImageFormat>,
    dimensions: Option<(u32, u32)>,
    file_len: u64,
    catalog: &Catalog,
) -> String {
    let format = match format {
        Some(crate::viewer::detect::ImageFormat::Png) => "PNG",
        Some(crate::viewer::detect::ImageFormat::Jpeg) => "JPEG",
        None => "?",
    };
    let (w, h) = dimensions
        .map(|(w, h)| (w.to_string(), h.to_string()))
        .unwrap_or_else(|| ("?".into(), "?".into()));
    let size = file_len.to_string();
    catalog.text(
        "viewer.status_image",
        &[
            ("format", format),
            ("width", &w),
            ("height", &h),
            ("size", &size),
        ],
    )
}

fn kind_label(kind: ViewerKind, catalog: &Catalog) -> String {
    crate::viewer::kind_label(kind, catalog)
}

/// Maps semantic styles to terminal styles. Text is already sanitized by the viewer modules;
/// sanitize again here so a custom provider cannot inject escape sequences.
pub(crate) fn to_line(line: &StyledLine) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|s| Span::styled(sanitize_for_display(&s.text), style_of(s.style)))
            .collect::<Vec<_>>(),
    )
}

fn style_of(style: SpanStyle) -> Style {
    let s = Style::new();
    match style {
        SpanStyle::Plain => s,
        SpanStyle::Heading(_) => s.add_modifier(Modifier::BOLD).fg(Color::Cyan),
        SpanStyle::Emphasis => s.add_modifier(Modifier::ITALIC),
        SpanStyle::Strong => s.add_modifier(Modifier::BOLD),
        SpanStyle::Code | SpanStyle::CodeBlock => s.fg(Color::Yellow),
        SpanStyle::Quote => s.fg(Color::Green),
        SpanStyle::Link => s.add_modifier(Modifier::UNDERLINED).fg(Color::Blue),
        SpanStyle::Dim => s.add_modifier(Modifier::DIM),
    }
}

fn fit(s: &str, width: u16) -> String {
    truncate_to_width(&sanitize_for_display(s), usize::from(width))
}
