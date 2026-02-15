mod input_bar;
mod library_panel;
mod now_playing;
mod queue;
pub mod visualizer;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;

use crate::app::{AppState, FocusedPanel};

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // input bar
            Constraint::Min(10),   // main content
            Constraint::Length(1), // status bar
        ])
        .split(f.area());

    // Input bar
    input_bar::draw(f, chunks[0], state);

    // Main content: visualizer + now_playing (left) | library + queue (right)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(65), // visualizer + now playing
            Constraint::Percentage(35), // library + queue
        ])
        .split(chunks[1]);

    // Left side: visualizer on top, now_playing on bottom
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),    // visualizer
            Constraint::Length(4), // now playing + progress
        ])
        .split(main_chunks[0]);

    visualizer::draw(f, left_chunks[0], state);
    now_playing::draw(f, left_chunks[1], state);

    // Right side: library (top) + queue (bottom)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // library
            Constraint::Percentage(50), // queue
        ])
        .split(main_chunks[1]);

    let lib_focused = state.focused_panel == FocusedPanel::Library;
    library_panel::draw(f, right_chunks[0], state, lib_focused);
    queue::draw(f, right_chunks[1], state, !lib_focused);

    // Status bar
    draw_status_bar(f, chunks[2], state);
}

fn draw_status_bar(f: &mut Frame, area: Rect, state: &AppState) {
    use crate::app::InputMode;
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let vol_filled = (state.volume as usize * 6) / 100;
    let vol_empty = 6 - vol_filled;
    let vol_bar = format!(
        "{}{}",
        "\u{2588}".repeat(vol_filled),
        "\u{2591}".repeat(vol_empty)
    );

    let key = |k: &str| Span::styled(format!(" [{}]", k), Style::default().fg(Color::Yellow));
    let label = |l: &str| Span::styled(format!(" {} ", l), Style::default().fg(Color::DarkGray));

    let mut spans = Vec::new();

    match state.input.mode {
        InputMode::Editing => {
            spans.push(Span::styled(
                " INPUT ",
                Style::default().fg(Color::Black).bg(Color::Magenta),
            ));
            spans.push(key("Tab"));
            spans.push(label("controls"));
            spans.push(key("Esc"));
            spans.push(label("controls"));
            spans.push(key("Enter"));
            spans.push(label("send"));
        }
        InputMode::Normal => {
            spans.push(Span::styled(
                " CONTROLS ",
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ));
            spans.push(key("Space"));
            spans.push(label("play"));
            spans.push(key("\u{2191}\u{2193}"));
            spans.push(label("nav"));
            spans.push(key("\u{2190}\u{2192}"));
            spans.push(label("panel"));
            spans.push(key("Tab"));
            spans.push(label("input"));
            spans.push(key("n"));
            spans.push(label("next"));
            spans.push(key("f/b"));
            spans.push(label("seek"));
            spans.push(key("+/-"));
            spans.push(label("vol"));
            spans.push(key("q"));
            spans.push(label("quit"));
        }
    }

    spans.push(Span::raw("    vol "));
    spans.push(Span::styled(vol_bar, Style::default().fg(Color::Cyan)));
    spans.push(Span::styled(
        format!(" {}%", state.volume),
        Style::default().fg(Color::DarkGray),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
