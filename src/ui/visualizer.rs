use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

const BAR_CHARS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const SHADE_CHARS: &[char] = &[' ', '░', '▒', '▓', '█'];

pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.current.is_none() {
        let center_y = inner.height / 2;
        let msg = if let Some(ref status) = state.status_message {
            status.as_str()
        } else {
            "paste a link or describe a vibe to start"
        };
        let color = if state.status_message.is_some() {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let display_width = (msg.len() as u16).min(inner.width);
        let x = inner.x + (inner.width.saturating_sub(display_width)) / 2;
        let line = Line::from(Span::styled(msg, Style::default().fg(color)));
        let msg_area = Rect::new(x, inner.y + center_y, display_width, 1);
        f.render_widget(Paragraph::new(line), msg_area);
        return;
    }

    let width = inner.width as usize;
    let height = inner.height as usize;

    if height == 0 || width == 0 {
        return;
    }

    // Build visualization lines
    let data = &state.visualizer_data;
    let mut lines = Vec::with_capacity(height);

    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        let row_factor = 1.0 - (row as f32 / height as f32);

        for col in 0..width {
            let data_idx = (col * data.len()) / width.max(1);
            let val = data.get(data_idx).copied().unwrap_or(0.0);

            if val >= row_factor {
                let intensity = ((val - row_factor) * 4.0).min(4.0) as usize;
                let ch = SHADE_CHARS[intensity.min(SHADE_CHARS.len() - 1)];
                let color = bar_color(val);
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(color),
                ));
            } else {
                spans.push(Span::raw(" "));
            }
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn bar_color(intensity: f32) -> Color {
    if intensity > 0.8 {
        Color::Magenta
    } else if intensity > 0.6 {
        Color::LightRed
    } else if intensity > 0.4 {
        Color::Yellow
    } else if intensity > 0.2 {
        Color::Cyan
    } else {
        Color::Blue
    }
}

/// Generate fake visualizer data based on time. Cheap and looks decent.
pub fn generate_visualizer_data(bars: usize, time_secs: f64, is_playing: bool) -> Vec<f32> {
    if !is_playing {
        return vec![0.0; bars];
    }

    let mut data = Vec::with_capacity(bars);
    for i in 0..bars {
        let x = i as f64 / bars as f64;
        // Combine a few sine waves at different frequencies for organic feel
        let v1 = ((x * 3.0 + time_secs * 2.3).sin() * 0.5 + 0.5) as f32;
        let v2 = ((x * 7.0 + time_secs * 1.7).sin() * 0.3 + 0.3) as f32;
        let v3 = ((x * 13.0 + time_secs * 3.1).sin() * 0.2 + 0.2) as f32;
        let combined = (v1 + v2 + v3) / 1.0;
        data.push(combined.clamp(0.0, 1.0));
    }
    data
}
