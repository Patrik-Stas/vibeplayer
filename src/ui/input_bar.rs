use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{AgentStatus, AppState, InputMode};

pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = state.input.mode == InputMode::Editing;

    let agent_indicator = match &state.agent_status {
        AgentStatus::Idle if is_focused => {
            Span::styled(" > ", Style::default().fg(Color::Green))
        }
        AgentStatus::Idle => Span::styled(" > ", Style::default().fg(Color::DarkGray)),
        AgentStatus::Thinking => {
            Span::styled(" * thinking... ", Style::default().fg(Color::Yellow))
        }
        AgentStatus::Acting(action) => {
            Span::styled(format!(" * {}... ", action), Style::default().fg(Color::Cyan))
        }
    };

    let input_text = if is_focused {
        Span::styled(&state.input.text, Style::default().fg(Color::White))
    } else if state.input.text.is_empty() {
        Span::styled(
            "press Tab to type, or use shortcuts below",
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::styled(&state.input.text, Style::default().fg(Color::DarkGray))
    };

    let cursor = if is_focused {
        Span::styled("_", Style::default().fg(Color::White))
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![agent_indicator, input_text, cursor]);

    let border_color = if is_focused {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" vibeplayer ")
        .title_style(Style::default().fg(Color::Magenta));

    let paragraph = Paragraph::new(line).block(block);
    f.render_widget(paragraph, area);

    if is_focused {
        // Offset: 1 (border) + indicator width
        let indicator_width = match &state.agent_status {
            AgentStatus::Idle => 3,
            AgentStatus::Thinking => 15,
            AgentStatus::Acting(a) => a.len() as u16 + 6,
        };
        let cursor_x = area.x + 1 + indicator_width + state.input.cursor as u16;
        let cursor_y = area.y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}
