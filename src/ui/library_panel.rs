use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn draw(f: &mut Frame, area: Rect, state: &AppState, is_focused: bool) {
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border_color))
        .title(" LIBRARY ")
        .title_style(Style::default().fg(if is_focused { Color::Cyan } else { Color::Yellow }));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.library.is_empty() {
        let line = Line::from(Span::styled(
            "  no songs yet",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(Paragraph::new(line), inner);
        return;
    }

    let visible_height = inner.height as usize;
    let cursor = state.library_cursor;

    // Scroll offset to keep cursor visible
    let scroll_offset = if cursor >= visible_height {
        cursor - visible_height + 1
    } else {
        0
    };

    let mut lines = Vec::new();

    for (i, song) in state
        .library
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
    {
        let is_selected = i == cursor;

        let max_title = (inner.width as usize).saturating_sub(4);
        let title = if max_title > 3 && song.title.len() > max_title {
            format!("{}...", &song.title[..max_title - 3])
        } else {
            song.title.clone()
        };

        let prefix = if is_selected { "> " } else { "  " };

        let style = if is_selected && is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(Span::styled(format!("{}{}", prefix, title), style)));
    }

    f.render_widget(Paragraph::new(lines), inner);
}
