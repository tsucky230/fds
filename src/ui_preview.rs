//! The preview pane right of the list (R-200-9): what `App::preview_view` says, in a box titled
//! with the cursor item's name.
//!
//! ```text
//!  [docs]/             ┌ README.md ───────────────────┐
//!   README.md          │ # FDS                        │
//!   Cargo.toml         │ A keyboard-first filer ...   │
//! ```
//!
//! The pane keeps its width whatever the cursor is on (`[..]` leaves it empty), so the list does
//! not jump while moving. Only the top of a document is shown; the viewer (Enter / v) scrolls.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, ScrollbarState};
use ratatui_image::picker::ProtocolType;
use ratatui_image::{Resize, StatefulImage};

use crate::app::App;
use crate::app_preview::PreviewView;
use crate::i18n::Catalog;
use crate::layout;
use crate::preview::{PreviewDoc, PreviewError};
use crate::text::{sanitize_for_display, truncate_to_width};
use crate::ui_viewer::ImageSlot;
use crate::viewer::detect::ImageFormat;
use crate::viewer::text::slice_columns;

/// Splits `area` into the list and, when the preview is on and fits, the pane right of it.
pub fn split(app: &App, area: Rect) -> (Rect, Option<Rect>) {
    let split = app
        .show_preview
        .then(|| layout::preview_split(area.width))
        .flatten();
    let Some((list, pane)) = split else {
        return (area, None);
    };
    (
        Rect {
            width: list,
            ..area
        },
        Some(Rect {
            x: area.x + list,
            width: pane,
            ..area
        }),
    )
}

/// Lowest body that can split the pane: three rows of content in each half, plus borders.
pub const PEEK_MIN_HEIGHT: u16 = 10;

/// The pane split into the folder list (top) and the content (bottom) (R-200-13).
pub fn split_peek(pane: Rect) -> (Rect, Rect) {
    let top = pane.height / 2;
    (
        Rect {
            height: top,
            ..pane
        },
        Rect {
            y: pane.y + top,
            height: pane.height - top,
            ..pane
        },
    )
}

/// Draws the folder list in the pane. Takes `&mut App` to keep the cursor in view.
pub fn render_peek(frame: &mut Frame, app: &mut App, catalog: &Catalog, area: Rect) {
    let Some(peek) = app.peek.as_mut() else {
        return;
    };
    let title = format!(" {} ", peek.dir_name);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(fit(&title, area.width.saturating_sub(2)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // The last row stays for "... and more".
    let height = usize::from(inner.height).saturating_sub(usize::from(peek.more));
    if peek.cursor < peek.scroll {
        peek.scroll = peek.cursor;
    } else if height > 0 && peek.cursor >= peek.scroll + height {
        peek.scroll = peek.cursor + 1 - height;
    }
    let overflow = peek.entries.len() > height && inner.width > 2;
    let width = inner.width - u16::from(overflow);
    let mut lines: Vec<Line> = peek
        .entries
        .iter()
        .enumerate()
        .skip(peek.scroll)
        .take(height)
        .map(|(i, entry)| {
            let mut style = crate::ui::entry_style(entry);
            if i == peek.cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }
            Line::styled(fit(&crate::ui::entry_text(entry), width), style)
        })
        .collect();
    if peek.more {
        lines.push(Line::styled(
            fit(&catalog.text("preview.more", &[]), width),
            Style::new(),
        ));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    if overflow {
        let mut state = ScrollbarState::new(peek.entries.len().saturating_sub(height))
            .viewport_content_length(height)
            .position(peek.scroll);
        frame.render_stateful_widget(
            crate::ui::scrollbar(),
            Rect {
                x: inner.x + width,
                width: 1,
                ..inner
            },
            &mut state,
        );
    }
}

/// Builds or drops the pane's picture before a frame (R-200-10). `picker` is None when images are
/// off (`[viewer] image_protocol = "none"`).
///
/// Behavior:
/// - Keeps a picture only when the last frame drew the pane (`preview_shown`) and `preview_view`
///   is `Ready(Picture)` for the cursor item: `preview_owner` is then that item's path and
///   `preview_protocol` its picture built with `picker` (None without a picker). The same owner
///   is not rebuilt. `preview_graphics` tells whether the picker draws with a graphics protocol
///   (anything but halfblocks).
/// - Otherwise the three are cleared.
/// - Returns true when a picture drawn with a graphics protocol was dropped or replaced: the
///   caller clears the terminal before the next frame, since Sixel, Kitty and iTerm2 pictures are
///   not always erased by drawing text over them. Halfblocks pictures are ordinary cells: false.
pub fn prepare_preview_image(
    app: &App,
    images: &mut ImageSlot,
    picker: Option<&ratatui_image::picker::Picker>,
) -> bool {
    let wanted = match app.preview_view() {
        PreviewView::Ready(PreviewDoc::Picture { thumbnail, .. }) if app.preview_shown => {
            app.preview_target().map(|path| (path, thumbnail))
        }
        _ => None,
    };
    let had_graphics = images.preview_graphics && images.preview_protocol.is_some();
    match wanted {
        Some((path, _)) if images.preview_owner.as_ref() == Some(path) => false,
        Some((path, thumbnail)) => {
            images.preview_owner = Some(path.clone());
            images.preview_protocol = picker.map(|p| p.new_resize_protocol((*thumbnail.0).clone()));
            images.preview_graphics =
                picker.is_some_and(|p| p.protocol_type() != ProtocolType::Halfblocks);
            had_graphics
        }
        None => {
            images.preview_owner = None;
            images.preview_protocol = None;
            images.preview_graphics = false;
            had_graphics
        }
    }
}

/// Draws the pane into `area` (from `split`). A `Picture` is drawn under its caption when
/// `images` holds the picture for the cursor item.
pub fn render(frame: &mut Frame, app: &App, catalog: &Catalog, images: &mut ImageSlot, area: Rect) {
    let mut block = Block::default().borders(Borders::ALL);
    if let Some(entry) = app.preview_entry() {
        let title = format!(" {} ", entry.display_name);
        block = block.title(fit(&title, area.width.saturating_sub(2)));
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width;
    let height = usize::from(inner.height);
    let lines = match app.preview_view() {
        PreviewView::Off | PreviewView::Empty => Vec::new(),
        PreviewView::Loading | PreviewView::Failed(PreviewError::Cancelled) => {
            vec![Line::from(fit(
                &catalog.text("preview.loading", &[]),
                width,
            ))]
        }
        PreviewView::Failed(PreviewError::NotAFile) => {
            vec![Line::from(fit(
                &catalog.text("preview.not_a_file", &[]),
                width,
            ))]
        }
        PreviewView::Failed(PreviewError::Io(reason)) => {
            let text = catalog.text("preview.error", &[("reason", reason)]);
            vec![Line::from(fit(&text, width))]
        }
        PreviewView::Ready(doc) => doc_lines(doc, catalog, width, height),
    };
    let picture = matches!(
        app.preview_view(),
        PreviewView::Ready(PreviewDoc::Picture { .. })
    ) && images
        .preview_owner
        .as_ref()
        .is_some_and(|owner| Some(owner) == app.preview_target());
    match images.preview_protocol.as_mut() {
        // The caption takes the first row; the picture fills the rest of the pane.
        Some(protocol) if picture && inner.height > 1 => {
            frame.render_widget(
                Paragraph::new(lines.into_iter().take(1).collect::<Vec<_>>()),
                inner,
            );
            let below = Rect {
                y: inner.y + 1,
                height: inner.height - 1,
                ..inner
            };
            let widget = StatefulImage::default().resize(Resize::Scale(None));
            frame.render_stateful_widget(widget, below, protocol);
        }
        _ => frame.render_widget(Paragraph::new(lines), inner),
    }
}

/// The first `height` rows of a document, cut at `width` columns.
fn doc_lines(doc: &PreviewDoc, catalog: &Catalog, width: u16, height: usize) -> Vec<Line<'static>> {
    match doc {
        PreviewDoc::Lines { lines, .. } => lines
            .iter()
            .take(height)
            .map(|l| crate::ui_viewer::to_line(&slice_columns(l, 0, usize::from(width))))
            .collect(),
        PreviewDoc::Binary { file_len } => {
            let size = crate::info::format_size(*file_len);
            let text = catalog.text("preview.binary", &[("size", &size)]);
            vec![Line::from(fit(&text, width))]
        }
        // The caption; `render` draws the picture under it when there is one to draw.
        PreviewDoc::Picture {
            format,
            dimensions,
            file_len,
            ..
        } => image_caption(Some(*dimensions), *format, *file_len, catalog, width),
        PreviewDoc::Image {
            format,
            dimensions,
            file_len,
        } => image_caption(*dimensions, *format, *file_len, catalog, width),
        PreviewDoc::Folder { entries, more } => {
            // Keep the last row for "... and more" when entries were left out.
            let room = if *more {
                height.saturating_sub(1)
            } else {
                height
            };
            let mut lines: Vec<Line> = entries
                .iter()
                .take(room)
                .map(|e| {
                    Line::styled(
                        fit(&crate::ui::entry_text(e), width),
                        crate::ui::entry_style(e),
                    )
                })
                .collect();
            if *more && height > 0 {
                lines.push(Line::from(fit(&catalog.text("preview.more", &[]), width)));
            }
            lines
        }
    }
}

/// "PNG  40x20  93 bytes" and the hint to open the viewer.
fn image_caption(
    dimensions: Option<(u32, u32)>,
    format: ImageFormat,
    file_len: u64,
    catalog: &Catalog,
    width: u16,
) -> Vec<Line<'static>> {
    let status = crate::ui_viewer::image_status(Some(format), dimensions, file_len, catalog);
    [status, catalog.text("preview.image_hint", &[])]
        .iter()
        .map(|t| Line::from(fit(t, width)))
        .collect()
}

fn fit(s: &str, width: u16) -> String {
    truncate_to_width(&sanitize_for_display(s), usize::from(width))
}
