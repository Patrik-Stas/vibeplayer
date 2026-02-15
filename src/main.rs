mod agent;
mod app;
mod config;
mod downloader;
mod library;
mod player;
mod ui;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing::{debug, error, info, warn};

use app::{AgentStatus, AppState, FocusedPanel, InputMode, NowPlaying, PlayerCommand, Song, SongStatus};
use config::Config;

fn setup_logging(config: &Config) {
    use tracing_subscriber::fmt;
    use tracing_subscriber::EnvFilter;

    let log_path = config.cache_dir.parent().unwrap_or(&config.cache_dir);
    let file_appender = tracing_appender::rolling::never(log_path, "vibeplayer.log");

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("vibeplayer=debug"));

    fmt()
        .with_env_filter(filter)
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Arc::new(Config::load()?);

    setup_logging(&config);
    info!("vibeplayer starting up");
    info!(cache_dir = %config.cache_dir.display(), model = %config.model, "config loaded");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    info!("TUI initialized, entering main loop");
    let result = run_app(&mut terminal, config).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    if let Err(ref e) = result {
        error!(?e, "app exited with error");
        eprintln!("Error: {:?}", e);
    } else {
        info!("app exited cleanly");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: Arc<Config>,
) -> Result<()> {
    let lib = library::Library::load(config.library_path.clone())?;
    let library = Arc::new(Mutex::new(lib));
    info!(path = %config.library_path.display(), "library loaded");

    let state = Arc::new(Mutex::new(AppState::new()));

    // Populate library panel with previously downloaded entries
    {
        let lib = library.lock().unwrap();
        let mut s = state.lock().unwrap();
        for entry in lib.entries() {
            let cached_path = config.cache_dir.join(&entry.file_path);
            if cached_path.exists() {
                let mut song = Song::new_queued(&entry.title, &entry.artist, &entry.url);
                song.file_path = Some(cached_path);
                song.duration = Some(Duration::from_secs_f64(entry.duration_secs));
                song.status = SongStatus::Ready;
                s.library.push(song);
            }
        }
        info!(count = s.library.len(), "restored songs to library panel");
    }

    let agent = Arc::new(agent::Agent::new(config.clone(), library));
    let mut player = player::Player::new()?;
    player.set_volume(config.default_volume);
    info!(volume = config.default_volume, "player initialized");

    let app_start = Instant::now();
    let tick_rate = Duration::from_millis(50);

    loop {
        // Update visualizer
        {
            let mut s = state.lock().unwrap();
            let is_playing = s.current.is_some() && !s.paused;
            let time = app_start.elapsed().as_secs_f64();
            s.visualizer_data =
                ui::visualizer::generate_visualizer_data(60, time, is_playing);
        }

        // Update playback position from player
        {
            let mut s = state.lock().unwrap();
            if s.current.is_some() {
                s.playback_position = player.get_position();
            }
        }

        // Draw
        {
            let mut s = state.lock().unwrap();
            terminal.draw(|f| ui::draw(f, &mut s))?;
        }

        // Process pending player commands from agent
        {
            let commands: Vec<PlayerCommand> = {
                let mut s = state.lock().unwrap();
                s.pending_commands.drain(..).collect()
            };

            for cmd in &commands {
                info!(?cmd, "processing player command");
            }

            for cmd in commands {
                match cmd {
                    PlayerCommand::PlayFile { path, title, artist, url, duration_secs } => {
                        info!(%url, %title, "playing downloaded file");
                        player.play_file(&path, Some(duration_secs))?;
                        let mut s = state.lock().unwrap();
                        let mut song = Song::new_queued(&title, &artist, &url);
                        song.file_path = Some(path);
                        song.duration = Some(Duration::from_secs_f64(duration_secs));
                        s.current = Some(NowPlaying {
                            song,
                            started_at: Instant::now(),
                            paused_elapsed: Duration::ZERO,
                            paused_at: None,
                        });
                        s.paused = false;
                    }
                    PlayerCommand::Skip => {
                        info!("skip requested");
                        player.stop();
                        state.lock().unwrap().current = None;
                    }
                    PlayerCommand::Pause => {
                        info!("pause requested");
                        player.pause();
                        state.lock().unwrap().paused = true;
                    }
                    PlayerCommand::Resume => {
                        info!("resume requested");
                        player.resume();
                        state.lock().unwrap().paused = false;
                    }
                    PlayerCommand::SetVolume(level) => {
                        info!(level, "volume change");
                        player.set_volume(level);
                        state.lock().unwrap().volume = level;
                    }
                }
            }
        }

        // Auto-advance: if current song stream ended, play next from queue
        {
            let should_advance = {
                let s = state.lock().unwrap();
                s.current.is_some() && player.is_empty()
            };

            if should_advance {
                let next = state.lock().unwrap().next_ready_song();
                if let Some(song) = next {
                    if let Some(ref path) = song.file_path {
                        info!(title = %song.title, url = %song.url, "auto-advancing to next song");
                        let dur = song.duration.map(|d| d.as_secs_f64());
                        player.play_file(path, dur)?;
                        let mut s = state.lock().unwrap();
                        s.current = Some(NowPlaying {
                            song,
                            started_at: Instant::now(),
                            paused_elapsed: Duration::ZERO,
                            paused_at: None,
                        });
                        s.paused = false;
                    } else {
                        info!(title = %song.title, "song not downloaded yet, skipping");
                    }
                } else {
                    info!("queue empty, stopping playback");
                    state.lock().unwrap().current = None;
                }
            }
        }

        // Handle input events
        if event::poll(tick_rate)? {
            let ev = event::read()?;

            // Mouse click on progress bar → seek
            if let Event::Mouse(mouse) = ev {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    let s = state.lock().unwrap();
                    if let (Some((bar_row, col_start, col_end)), Some(ref np)) =
                        (s.progress_bar_area, &s.current)
                    {
                        if mouse.row == bar_row
                            && mouse.column >= col_start
                            && mouse.column < col_end
                        {
                            let duration = np.song.duration.unwrap_or(Duration::ZERO);
                            if duration > Duration::ZERO {
                                let frac = (mouse.column - col_start) as f64
                                    / (col_end - col_start) as f64;
                                let position = Duration::from_secs_f64(
                                    frac * duration.as_secs_f64(),
                                );
                                drop(s);
                                info!(?position, "user: mouse seek");
                                player.seek(position);
                            }
                        }
                    }
                }
                continue;
            }

            if let Event::Key(key) = ev {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                let in_edit_mode = state.lock().unwrap().input.mode == InputMode::Editing;

                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        info!("user: Ctrl+C quit");
                        state.lock().unwrap().should_quit = true;
                    }

                    // Editing mode
                    KeyCode::Enter if in_edit_mode => {
                        let input_text = state.lock().unwrap().input.submit();
                        if !input_text.is_empty() {
                            info!(%input_text, "user submitted input");
                            let agent = agent.clone();
                            let state_clone = state.clone();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    agent.handle_input(&input_text, &state_clone).await
                                {
                                    error!(?e, "agent error");
                                    let mut s = state_clone.lock().unwrap();
                                    s.agent_status = AgentStatus::Idle;
                                    s.status_message =
                                        Some(format!("Agent error: {}", e));
                                }
                            });
                        }
                    }

                    KeyCode::Char(c) if in_edit_mode => {
                        state.lock().unwrap().input.insert(c);
                    }

                    KeyCode::Backspace if in_edit_mode => {
                        state.lock().unwrap().input.backspace();
                    }

                    KeyCode::Esc if in_edit_mode => {
                        debug!("user: Esc -> normal mode");
                        state.lock().unwrap().input.mode = InputMode::Normal;
                    }

                    // Tab toggles between input and normal mode
                    KeyCode::Tab => {
                        let mut s = state.lock().unwrap();
                        s.input.mode = match s.input.mode {
                            InputMode::Editing => {
                                debug!("user: Tab -> normal mode");
                                InputMode::Normal
                            }
                            InputMode::Normal => {
                                debug!("user: Tab -> editing mode");
                                InputMode::Editing
                            }
                        };
                    }

                    // Normal mode — '/' or 'i' also enters input
                    KeyCode::Char('i') | KeyCode::Char('/') if !in_edit_mode => {
                        debug!("user: enter editing mode");
                        state.lock().unwrap().input.mode = InputMode::Editing;
                    }

                    KeyCode::Char('q') if !in_edit_mode => {
                        info!("user: q quit");
                        state.lock().unwrap().should_quit = true;
                    }

                    KeyCode::Char('p') if !in_edit_mode => {
                        let mut s = state.lock().unwrap();
                        s.paused = !s.paused;
                        if s.paused {
                            info!("user: pause");
                            player.pause();
                        } else {
                            info!("user: resume");
                            player.resume();
                        }
                    }

                    KeyCode::Char('n') if !in_edit_mode => {
                        info!("user: skip/next");
                        player.stop();
                        state.lock().unwrap().current = None;
                    }

                    KeyCode::Char('f') if !in_edit_mode => {
                        let s = state.lock().unwrap();
                        if s.current.is_some() {
                            let pos = s.playback_position + Duration::from_secs(10);
                            drop(s);
                            info!(?pos, "user: seek forward 10s");
                            player.seek(pos);
                        }
                    }

                    KeyCode::Char('b') if !in_edit_mode => {
                        let s = state.lock().unwrap();
                        if s.current.is_some() {
                            let pos = s.playback_position.saturating_sub(Duration::from_secs(10));
                            drop(s);
                            info!(?pos, "user: seek backward 10s");
                            player.seek(pos);
                        }
                    }

                    KeyCode::Char('+') | KeyCode::Char('=') if !in_edit_mode => {
                        let mut s = state.lock().unwrap();
                        s.volume = (s.volume + 5).min(100);
                        debug!(volume = s.volume, "user: volume up");
                        player.set_volume(s.volume);
                    }

                    KeyCode::Char('-') if !in_edit_mode => {
                        let mut s = state.lock().unwrap();
                        s.volume = s.volume.saturating_sub(5);
                        debug!(volume = s.volume, "user: volume down");
                        player.set_volume(s.volume);
                    }

                    KeyCode::Up if !in_edit_mode => {
                        state.lock().unwrap().move_cursor_up();
                    }

                    KeyCode::Down if !in_edit_mode => {
                        state.lock().unwrap().move_cursor_down();
                    }

                    KeyCode::Left if !in_edit_mode => {
                        state.lock().unwrap().switch_panel_left();
                    }

                    KeyCode::Right if !in_edit_mode => {
                        state.lock().unwrap().switch_panel_right();
                    }

                    KeyCode::Char(' ') if !in_edit_mode => {
                        let mut s = state.lock().unwrap();
                        // Try to play selected song first
                        let played = match s.focused_panel {
                            FocusedPanel::Library => {
                                let idx = s.library_cursor;
                                if idx < s.library.len() && s.library[idx].status == SongStatus::Ready {
                                    let song = s.library[idx].clone();
                                    if let Some(ref path) = song.file_path {
                                        info!(title = %song.title, "user: play from library");
                                        let dur = song.duration.map(|d| d.as_secs_f64());
                                        match player.play_file(path, dur) {
                                            Ok(()) => {
                                                s.current = Some(NowPlaying {
                                                    song,
                                                    started_at: Instant::now(),
                                                    paused_elapsed: Duration::ZERO,
                                                    paused_at: None,
                                                });
                                                s.paused = false;
                                                true
                                            }
                                            Err(e) => { error!(?e, "failed to play file"); false }
                                        }
                                    } else { false }
                                } else { false }
                            }
                            FocusedPanel::Queue => {
                                let idx = s.queue_cursor;
                                if idx < s.queue.len() && s.queue[idx].status == SongStatus::Ready {
                                    let song = s.queue.remove(idx);
                                    s.clamp_cursors();
                                    if let Some(ref path) = song.file_path {
                                        info!(title = %song.title, "user: play from queue");
                                        let dur = song.duration.map(|d| d.as_secs_f64());
                                        match player.play_file(path, dur) {
                                            Ok(()) => {
                                                s.current = Some(NowPlaying {
                                                    song,
                                                    started_at: Instant::now(),
                                                    paused_elapsed: Duration::ZERO,
                                                    paused_at: None,
                                                });
                                                s.paused = false;
                                                true
                                            }
                                            Err(e) => { error!(?e, "failed to play file"); false }
                                        }
                                    } else { false }
                                } else { false }
                            }
                        };
                        // Fall back to pause/resume if no song was played
                        if !played && s.current.is_some() {
                            s.paused = !s.paused;
                            if s.paused {
                                info!("user: space pause");
                                player.pause();
                            } else {
                                info!("user: space resume");
                                player.resume();
                            }
                        }
                    }

                    _ => {}
                }
            }
        }

        if state.lock().unwrap().should_quit {
            info!("quit flag set, exiting main loop");
            break;
        }
    }

    Ok(())
}
