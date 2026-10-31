//! Read-only archive member list (R-407).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, ScrollbarState};

use crate::app::App;
use crate::text::{sanitize_for_display, truncate_to_width};

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.archive.as_mut() else {
        return;
    };
    let height = usize::from(area.height);
    if height == 0 {
        return;
    }
    let len = view.children().len();
    view.cursor = view.cursor.min(len.saturating_sub(1));
    if view.cursor < view.scroll {
        view.scroll = view.cursor;
    }
    if view.cursor >= view.scroll + height {
        view.scroll = view.cursor + 1 - height;
    }
    let overflow = len > height && area.width > 2;
    let list_width = area.width - u16::from(overflow);
    let lines: Vec<Line> = view
        .children()
        .iter()
        .enumerate()
        .skip(view.scroll)
        .take(height)
        .map(|(i, member)| {
            let name = member
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let name = sanitize_for_display(&name);
            let (label, mut style) = if member.is_dir {
                (format!(" [{name}]/"), crate::ui::dir_style())
            } else {
                (format!("  {name}"), Style::new())
            };
            if i == view.cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }
            Line::styled(truncate_to_width(&label, usize::from(list_width)), style)
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines),
        Rect {
            width: list_width,
            ..area
        },
    );
    if overflow {
        let mut state = ScrollbarState::new(len.saturating_sub(height))
            .viewport_content_length(height)
            .position(view.scroll);
        frame.render_stateful_widget(
            crate::ui::scrollbar(),
            Rect {
                x: area.x + list_width,
                width: 1,
                ..area
            },
            &mut state,
        );
    }
}
