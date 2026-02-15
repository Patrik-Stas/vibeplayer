use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::audio_analysis::AudioFeatures;

const BAR_CHARS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

// ---------------------------------------------------------------------------
// MatrixRain — now just a tick counter for the wave animation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MatrixRain {
    tick: u64,
}

impl MatrixRain {
    pub fn new(_width: usize, _height: usize) -> Self {
        Self { tick: 0 }
    }

    pub fn resize(&mut self, _width: usize, _height: usize) {}

    pub fn update(&mut self, _features: &AudioFeatures) {
        self.tick = self.tick.wrapping_add(1);
    }
}

// ---------------------------------------------------------------------------
// draw
// ---------------------------------------------------------------------------

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

    let feat = &state.audio_features;
    let t = state.matrix_rain.tick as f64 * 0.08;

    // Center line of the wave
    let center = height as f64 / 2.0;

    // Compute wave height for each column — multiple sine waves modulated by audio
    let mut wave = vec![0.0f64; width];
    for col in 0..width {
        let x = col as f64 / width as f64;

        // Base wave: slow sine, amplitude from bass
        let w1 = (x * 4.0 + t).sin() * feat.bass as f64 * center * 0.6;
        // Mid-frequency wave from mids/rms
        let w2 = (x * 9.0 - t * 1.3).sin() * feat.rms as f64 * center * 0.4;
        // High-frequency ripple from treble
        let w3 = (x * 18.0 + t * 2.5).sin() * feat.treble as f64 * center * 0.25;

        wave[col] = w1 + w2 + w3;
    }

    // Color based on energy
    let base_g: u8 = (80.0 + feat.rms * 175.0).min(255.0) as u8;
    let base_b: u8 = (40.0 + feat.treble * 120.0).min(160.0) as u8;
    let bright = feat.is_beat;

    let mut lines = Vec::with_capacity(height);

    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        let row_y = row as f64; // 0 = top

        for col in 0..width {
            // Wave center is at `center + wave[col]`
            let wave_center = center + wave[col];

            // Distance from this row to the wave center
            let dist = (row_y - wave_center).abs();

            // Wave has a thickness proportional to energy
            let thickness = 0.8 + feat.rms as f64 * 2.0;

            if dist < thickness + 1.0 {
                // Within the wave band — compute sub-cell fill
                let fill = ((thickness - dist + 1.0) / 1.0).clamp(0.0, 1.0);
                let char_idx = (fill * (BAR_CHARS.len() - 1) as f64) as usize;
                let ch = if row_y > wave_center {
                    // Below center: normal bars (▁▂▃... growing up)
                    BAR_CHARS[char_idx]
                } else {
                    // Above center: inverted (█▇▆... growing down)
                    BAR_CHARS[BAR_CHARS.len() - 1 - char_idx]
                };

                // Color: brighter near center, dimmer at edges
                let edge_fade = (1.0 - dist / (thickness + 1.0)) as f32;
                let g = (base_g as f32 * edge_fade).min(255.0) as u8;
                let b = (base_b as f32 * edge_fade * 0.6).min(255.0) as u8;
                let r = if bright { (60.0 * edge_fade) as u8 } else { 0 };

                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(Color::Rgb(r, g, b)),
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
