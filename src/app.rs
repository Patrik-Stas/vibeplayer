use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::audio_analysis::AudioFeatures;
use crate::ui::visualizer::MatrixRain;

#[derive(Debug, Clone, PartialEq)]
pub enum SongStatus {
    Queued,
    Downloading,
    Ready,
    Playing,
    Played,
}

#[derive(Debug, Clone)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub url: String,
    pub file_path: Option<PathBuf>,
    pub status: SongStatus,
    pub duration: Option<Duration>,
}

impl Song {
    pub fn new_queued(title: &str, artist: &str, url: &str) -> Self {
        Self {
            title: title.to_string(),
            artist: artist.to_string(),
            url: url.to_string(),
            file_path: None,
            status: SongStatus::Queued,
            duration: None,
        }
    }

    pub fn new_downloading(url: &str) -> Self {
        Self {
            title: "Loading...".to_string(),
            artist: String::new(),
            url: url.to_string(),
            file_path: None,
            status: SongStatus::Downloading,
            duration: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub song: Song,
    pub started_at: Instant,
    pub paused_elapsed: Duration,
    pub paused_at: Option<Instant>,
}

impl NowPlaying {
    pub fn elapsed(&self) -> Duration {
        if let Some(paused_at) = self.paused_at {
            self.paused_elapsed + (paused_at - self.started_at) - self.paused_elapsed
        } else {
            self.started_at.elapsed() - self.paused_elapsed
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Acting(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusedPanel {
    Library,
    Queue,
}

#[derive(Debug, Clone)]
pub struct InputState {
    pub text: String,
    pub cursor: usize,
    pub mode: InputMode,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            mode: InputMode::Normal,
        }
    }
}

impl InputState {
    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.remove(prev);
            self.cursor = prev;
        }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn submit(&mut self) -> String {
        let text = self.text.clone();
        self.clear();
        text
    }
}

/// Command from agent to the main loop (which owns the player)
#[derive(Debug, Clone)]
pub enum PlayerCommand {
    PlayFile {
        path: PathBuf,
        title: String,
        artist: String,
        url: String,
        duration_secs: f64,
    },
    Skip,
    Pause,
    Resume,
    SetVolume(u8),
}

pub struct AppState {
    pub queue: Vec<Song>,
    pub library: Vec<Song>,
    pub current: Option<NowPlaying>,
    pub input: InputState,
    pub agent_status: AgentStatus,
    pub volume: u8,
    pub paused: bool,
    pub audio_features: AudioFeatures,
    pub matrix_rain: MatrixRain,
    pub should_quit: bool,
    pub pending_commands: Vec<PlayerCommand>,
    /// Status message shown in the visualizer area (buffering, errors, etc.)
    pub status_message: Option<String>,
    pub focused_panel: FocusedPanel,
    pub library_cursor: usize,
    pub queue_cursor: usize,
    pub playback_position: Duration,
    /// Progress bar clickable region: (row, col_start, col_end)
    pub progress_bar_area: Option<(u16, u16, u16)>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            library: Vec::new(),
            current: None,
            input: InputState::default(),
            agent_status: AgentStatus::Idle,
            volume: 70,
            paused: false,
            audio_features: AudioFeatures::default(),
            matrix_rain: MatrixRain::new(80, 24),
            should_quit: false,
            pending_commands: Vec::new(),
            status_message: None,
            focused_panel: FocusedPanel::Library,
            library_cursor: 0,
            queue_cursor: 0,
            playback_position: Duration::ZERO,
            progress_bar_area: None,
        }
    }

    pub fn next_ready_song(&mut self) -> Option<Song> {
        if let Some(pos) = self.queue.iter().position(|s| s.status == SongStatus::Ready) {
            let song = self.queue.remove(pos);
            self.clamp_cursors();
            Some(song)
        } else {
            None
        }
    }

    pub fn move_cursor_up(&mut self) {
        match self.focused_panel {
            FocusedPanel::Library => {
                if self.library_cursor > 0 {
                    self.library_cursor -= 1;
                }
            }
            FocusedPanel::Queue => {
                if self.queue_cursor > 0 {
                    self.queue_cursor -= 1;
                }
            }
        }
    }

    pub fn move_cursor_down(&mut self) {
        match self.focused_panel {
            FocusedPanel::Library => {
                if !self.library.is_empty() {
                    self.library_cursor = (self.library_cursor + 1).min(self.library.len() - 1);
                }
            }
            FocusedPanel::Queue => {
                if !self.queue.is_empty() {
                    self.queue_cursor = (self.queue_cursor + 1).min(self.queue.len() - 1);
                }
            }
        }
    }

    pub fn switch_panel_left(&mut self) {
        self.focused_panel = FocusedPanel::Library;
    }

    pub fn switch_panel_right(&mut self) {
        self.focused_panel = FocusedPanel::Queue;
    }

    pub fn clamp_cursors(&mut self) {
        if self.library.is_empty() {
            self.library_cursor = 0;
        } else {
            self.library_cursor = self.library_cursor.min(self.library.len() - 1);
        }
        if self.queue.is_empty() {
            self.queue_cursor = 0;
        } else {
            self.queue_cursor = self.queue_cursor.min(self.queue.len() - 1);
        }
    }
}
