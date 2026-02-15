use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::Duration;

use crate::app::AppState;

pub fn draw(f: &mut Frame, area: Rect, state: &mut AppState) {
    let Some(ref np) = state.current else {
        return;
    };

    let mut lines = Vec::new();

    // Song title - artist
    let title_line = if np.song.artist.is_empty() {
        Line::from(Span::styled(
            format!("  {}", np.song.title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                format!("  {}", np.song.title),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" - {}", np.song.artist),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    };
    lines.push(title_line);

    // Progress bar
    let duration = np.song.duration.unwrap_or(Duration::ZERO);
    let elapsed = if duration > Duration::ZERO {
        state.playback_position.min(duration)
    } else {
        state.playback_position
    };
    let progress = if duration.as_secs() > 0 {
        elapsed.as_secs_f64() / duration.as_secs_f64()
    } else {
        0.0
    };

    let play_icon = if state.paused { "||" } else { ">>" };
    let prefix = format!("  [{}] ", play_icon); // 7 chars
    let time_str = format!(" {} / {}", format_duration(elapsed), format_duration(duration));
    let overhead = prefix.len() + 1 + time_str.len(); // +1 for the dot
    let bar_width = (area.width as usize).saturating_sub(overhead);
    let filled = (progress * bar_width as f64).min(bar_width as f64) as usize;
    let empty = bar_width.saturating_sub(filled);

    // Store the clickable region for mouse seeking
    // Progress bar is on the second line of this area (area.y + 1)
    let bar_col_start = area.x + prefix.len() as u16;
    let bar_col_end = bar_col_start + bar_width as u16;
    state.progress_bar_area = Some((area.y + 1, bar_col_start, bar_col_end));

    let progress_line = Line::from(vec![
        Span::styled(prefix, Style::default().fg(Color::Green)),
        Span::styled(
            "\u{2501}".repeat(filled),
            Style::default().fg(Color::Magenta),
        ),
        Span::styled("\u{25CF}", Style::default().fg(Color::White)),
        Span::styled(
            "\u{2501}".repeat(empty),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(time_str),
    ]);
    lines.push(progress_line);

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}
