use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, SongStatus};

pub fn draw(f: &mut Frame, area: Rect, state: &AppState, is_focused: bool) {
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border_color))
        .title(" UP NEXT ")
        .title_style(Style::default().fg(if is_focused { Color::Cyan } else { Color::Yellow }));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.queue.is_empty() {
        let line = Line::from(Span::styled(
            "  queue is empty",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(Paragraph::new(line), inner);
        return;
    }

    let cursor = state.queue_cursor;
    let visible_height = inner.height as usize;

    // Each song takes 2-3 lines, estimate items per screen
    let lines_per_item = 3;
    let max_display = (visible_height / lines_per_item).max(1);

    // Scroll offset to keep cursor visible
    let scroll_offset = if cursor >= max_display {
        cursor - max_display + 1
    } else {
        0
    };

    let mut lines = Vec::new();

    for (i, song) in state
        .queue
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_display)
    {
        let is_selected = i == cursor;

        let max_title = (inner.width as usize).saturating_sub(8);
        let title = if max_title > 3 && song.title.len() > max_title {
            format!("{}...", &song.title[..max_title - 3])
        } else {
            song.title.clone()
        };

        let prefix = if is_selected { "> " } else { "  " };
        let title_style = if is_selected && is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };
        let num_style = Style::default().fg(Color::DarkGray);

        lines.push(Line::from(vec![
            Span::styled(format!("{}{}. ", prefix, i + 1), num_style),
            Span::styled(title, title_style),
        ]));

        // Status line
        let (status_text, status_color) = match song.status {
            SongStatus::Queued => ("queued", Color::DarkGray),
            SongStatus::Downloading => ("downloading...", Color::Yellow),
            SongStatus::Ready => ("ready", Color::Green),
            SongStatus::Playing => ("playing", Color::Magenta),
            SongStatus::Played => ("played", Color::DarkGray),
        };

        lines.push(Line::from(Span::styled(
            format!("     {}", status_text),
            Style::default().fg(status_color),
        )));

        // Spacing
        if i < state.queue.len().saturating_sub(1) {
            lines.push(Line::from(""));
        }
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}
