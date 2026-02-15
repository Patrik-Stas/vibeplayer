use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::app::{AgentStatus, AppState, PlayerCommand, Song, SongStatus};
use crate::config::Config;
use crate::downloader;
use crate::library::Library;

const SYSTEM_PROMPT: &str = r#"You are the AI brain of vibeplayer, a TUI-based YouTube music player. Your job is to interpret user commands and control the player using tools.

You receive the current player state (now playing, queue) with each message. Use the tools to respond to the user's intent. Always use tools — never respond with just text.

Guidelines:
- For YouTube URLs, use play_url
- For song/artist names, use search_and_queue with good search queries
- For vibe/mood requests, translate the mood into multiple specific search queries
- When replacing the queue, pick 4-6 diverse but fitting search queries
- Keep search queries specific: include artist names, song names, or descriptive terms like "chill lo-fi beats" rather than vague terms"#;

fn tool_definitions() -> Value {
    json!([
        {
            "name": "play_url",
            "description": "Download and play a YouTube URL immediately. Use when the user provides a direct YouTube link.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "YouTube URL to play" }
                },
                "required": ["url"]
            }
        },
        {
            "name": "search_and_queue",
            "description": "Search YouTube and add results to the queue. Use for song names, artist requests, or mood-based queries.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "YouTube search query" },
                    "count": { "type": "integer", "description": "Number of results to queue (1-5)", "default": 3 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "replace_queue",
            "description": "Clear the current queue and populate with new searches. Use when the user wants to change the vibe or mood entirely.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of YouTube search queries to populate the new queue"
                    }
                },
                "required": ["queries"]
            }
        },
        {
            "name": "skip",
            "description": "Skip the currently playing song.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "pause",
            "description": "Pause playback.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "resume",
            "description": "Resume playback.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "set_volume",
            "description": "Set the playback volume.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "level": { "type": "integer", "description": "Volume level 0-100" }
                },
                "required": ["level"]
            }
        }
    ])
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

pub struct Agent {
    client: reqwest::Client,
    config: Arc<Config>,
    library: Arc<Mutex<Library>>,
}

impl Agent {
    pub fn new(config: Arc<Config>, library: Arc<Mutex<Library>>) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
            library,
        }
    }

    pub async fn handle_input(
        &self,
        input: &str,
        state: &Arc<Mutex<AppState>>,
    ) -> Result<()> {
        info!(%input, "agent handling input");

        // 1. Snapshot state
        let context = {
            let s = state.lock().unwrap();
            build_context(&s)
        };
        debug!(%context, "agent context snapshot");

        // 2. Mark as thinking
        state.lock().unwrap().agent_status = AgentStatus::Thinking;
        info!("agent status: thinking");

        // 3. Call Claude API
        info!(model = %self.config.model, "calling Claude API");
        let tool_calls = self.call_api(input, &context).await?;
        info!(count = tool_calls.len(), "received tool calls from API");

        // 4. Execute tool calls
        for (name, input_val) in &tool_calls {
            info!(tool = %name, input = %input_val, "executing tool call");
            state.lock().unwrap().agent_status =
                AgentStatus::Acting(name.clone());
            self.execute_tool(name, input_val.clone(), state).await?;
            info!(tool = %name, "tool call completed");
        }

        // 5. Done
        state.lock().unwrap().agent_status = AgentStatus::Idle;
        info!("agent status: idle");
        Ok(())
    }

    async fn call_api(
        &self,
        user_input: &str,
        context: &str,
    ) -> Result<Vec<(String, Value)>> {
        let body = json!({
            "model": self.config.model,
            "max_tokens": 1024,
            "system": format!("{}\n\nCurrent state:\n{}", SYSTEM_PROMPT, context),
            "tools": tool_definitions(),
            "messages": [
                { "role": "user", "content": user_input }
            ]
        });

        debug!("sending API request");
        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to reach Claude API")?;

        let status = resp.status();
        info!(%status, "API response received");

        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            error!(%status, %err_text, "Claude API error");
            anyhow::bail!("Claude API error ({}): {}", status, err_text);
        }

        let raw_body = resp.text().await.context("Failed to read API response body")?;
        debug!(body_len = raw_body.len(), "API response body received");

        let api_resp: ApiResponse = serde_json::from_str(&raw_body)
            .context("Failed to parse API response JSON")?;

        let tool_calls: Vec<(String, Value)> = api_resp
            .content
            .into_iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { name, input, .. } => {
                    info!(tool = %name, %input, "parsed tool call from response");
                    Some((name, input))
                }
                ContentBlock::Text { text } => {
                    debug!(%text, "LLM text response (non-tool)");
                    None
                }
            })
            .collect();

        if tool_calls.is_empty() {
            warn!("API returned no tool calls — LLM may have responded with text only");
        }

        Ok(tool_calls)
    }

    async fn execute_tool(
        &self,
        name: &str,
        input: Value,
        state: &Arc<Mutex<AppState>>,
    ) -> Result<()> {
        match name {
            "play_url" => {
                let url = input["url"].as_str().unwrap_or_default().to_string();

                // Check library for cached entry
                {
                    let lib = self.library.lock().unwrap();
                    if let Some(entry) = lib.find_by_url(&url) {
                        let cached_path = self.config.cache_dir.join(&entry.file_path);
                        if cached_path.exists() {
                            info!(%url, title = %entry.title, "using cached library entry");
                            let mut s = state.lock().unwrap();
                            s.pending_commands.push(PlayerCommand::PlayFile {
                                path: cached_path,
                                title: entry.title.clone(),
                                artist: entry.artist.clone(),
                                url: url.clone(),
                                duration_secs: entry.duration_secs,
                            });
                            return Ok(());
                        }
                    }
                }

                info!(%url, "play_url: downloading");
                {
                    let mut s = state.lock().unwrap();
                    s.status_message = Some("Downloading...".to_string());
                }
                let config = self.config.clone();
                let state_clone = state.clone();
                let library = self.library.clone();
                tokio::spawn(async move {
                    match downloader::download_song(&url, &config).await {
                        Ok((path, meta)) => {
                            info!(%url, title = %meta.title, "download complete, queueing playback");
                            persist_to_library(&library, &meta, &url, &config, &state_clone);
                            let mut s = state_clone.lock().unwrap();
                            s.status_message = None;
                            s.pending_commands.push(PlayerCommand::PlayFile {
                                path,
                                title: meta.title,
                                artist: meta.artist,
                                url: url.clone(),
                                duration_secs: meta.duration_secs,
                            });
                        }
                        Err(e) => {
                            error!(%url, ?e, "download failed");
                            let mut s = state_clone.lock().unwrap();
                            s.status_message = Some(format!("Download error: {}", e));
                        }
                    }
                });
            }

            "search_and_queue" => {
                let query = input["query"].as_str().unwrap_or_default().to_string();
                let count = input["count"].as_u64().unwrap_or(3) as u32;
                info!(%query, %count, "search_and_queue");

                let results = downloader::search_youtube(&query, count).await?;
                info!(results_count = results.len(), "search returned results");

                let config = self.config.clone();
                let state_clone = state.clone();

                for result in results {
                    // Check library for cached entry
                    let cached = {
                        let lib = self.library.lock().unwrap();
                        lib.find_by_url(&result.url).and_then(|entry| {
                            let cached_path = config.cache_dir.join(&entry.file_path);
                            if cached_path.exists() {
                                Some((cached_path, entry.title.clone(), entry.artist.clone(), entry.duration_secs))
                            } else {
                                None
                            }
                        })
                    };

                    if let Some((path, title, artist, duration_secs)) = cached {
                        info!(url = %result.url, %title, "using cached library entry");
                        let mut s = state_clone.lock().unwrap();
                        let mut song = Song::new_queued(&title, &artist, &result.url);
                        song.file_path = Some(path);
                        song.duration = Some(Duration::from_secs_f64(duration_secs));
                        song.status = SongStatus::Ready;
                        s.queue.push(song);
                        continue;
                    }

                    info!(title = %result.title, url = %result.url, "queueing song for download");
                    {
                        let mut s = state_clone.lock().unwrap();
                        let mut song = Song::new_queued(
                            &result.title,
                            "",
                            &result.url,
                        );
                        song.status = SongStatus::Downloading;
                        s.queue.push(song);
                    }

                    let url = result.url.clone();
                    let cfg = config.clone();
                    let st = state_clone.clone();
                    let library = self.library.clone();
                    tokio::spawn(async move {
                        info!(%url, "starting background download");
                        match downloader::download_song(&url, &cfg).await {
                            Ok((path, meta)) => {
                                info!(%url, title = %meta.title, "download complete");
                                persist_to_library(&library, &meta, &url, &cfg, &st);
                                let mut s = st.lock().unwrap();
                                if let Some(song) =
                                    s.queue.iter_mut().find(|s| s.url == url)
                                {
                                    song.title = meta.title;
                                    song.artist = meta.artist;
                                    song.file_path = Some(path);
                                    song.duration =
                                        Some(Duration::from_secs_f64(meta.duration_secs));
                                    song.status = SongStatus::Ready;
                                }
                            }
                            Err(e) => {
                                error!(%url, ?e, "download failed");
                            }
                        }
                    });
                }
            }

            "replace_queue" => {
                let queries: Vec<String> = input["queries"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                info!(?queries, "replace_queue");

                {
                    let mut s = state.lock().unwrap();
                    s.queue.clear();
                    s.clamp_cursors();
                }

                for query in queries {
                    info!(%query, "searching for queue replacement");
                    let results = downloader::search_youtube(&query, 2).await?;
                    info!(count = results.len(), %query, "search results");

                    let config = self.config.clone();
                    let state_clone = state.clone();

                    for result in results {
                        // Check library for cached entry
                        let cached = {
                            let lib = self.library.lock().unwrap();
                            lib.find_by_url(&result.url).and_then(|entry| {
                                let cached_path = config.cache_dir.join(&entry.file_path);
                                if cached_path.exists() {
                                    Some((cached_path, entry.title.clone(), entry.artist.clone(), entry.duration_secs))
                                } else {
                                    None
                                }
                            })
                        };

                        if let Some((path, title, artist, duration_secs)) = cached {
                            info!(url = %result.url, %title, "using cached library entry");
                            let mut s = state_clone.lock().unwrap();
                            let mut song = Song::new_queued(&title, &artist, &result.url);
                            song.file_path = Some(path);
                            song.duration = Some(Duration::from_secs_f64(duration_secs));
                            song.status = SongStatus::Ready;
                            s.queue.push(song);
                            continue;
                        }

                        info!(title = %result.title, url = %result.url, "queueing song for download");
                        {
                            let mut s = state_clone.lock().unwrap();
                            let mut song = Song::new_queued(
                                &result.title,
                                "",
                                &result.url,
                            );
                            song.status = SongStatus::Downloading;
                            s.queue.push(song);
                        }

                        let url = result.url.clone();
                        let cfg = config.clone();
                        let st = state_clone.clone();
                        let library = self.library.clone();
                        tokio::spawn(async move {
                            info!(%url, "starting background download");
                            match downloader::download_song(&url, &cfg).await {
                                Ok((path, meta)) => {
                                    info!(%url, title = %meta.title, "download complete");
                                    persist_to_library(&library, &meta, &url, &cfg, &st);
                                    let mut s = st.lock().unwrap();
                                    if let Some(song) =
                                        s.queue.iter_mut().find(|s| s.url == url)
                                    {
                                        song.title = meta.title;
                                        song.artist = meta.artist;
                                        song.file_path = Some(path);
                                        song.duration = Some(Duration::from_secs_f64(
                                            meta.duration_secs,
                                        ));
                                        song.status = SongStatus::Ready;
                                    }
                                }
                                Err(e) => {
                                    error!(%url, ?e, "download failed");
                                }
                            }
                        });
                    }
                }
            }

            "skip" => {
                info!("tool: skip");
                state.lock().unwrap().pending_commands.push(PlayerCommand::Skip);
            }

            "pause" => {
                info!("tool: pause");
                state.lock().unwrap().pending_commands.push(PlayerCommand::Pause);
            }

            "resume" => {
                info!("tool: resume");
                state.lock().unwrap().pending_commands.push(PlayerCommand::Resume);
            }

            "set_volume" => {
                let level = input["level"].as_u64().unwrap_or(70) as u8;
                info!(level, "tool: set_volume");
                state.lock().unwrap().pending_commands.push(PlayerCommand::SetVolume(level));
            }

            other => {
                warn!(tool = %other, "unknown tool call received");
            }
        }

        Ok(())
    }
}

fn persist_to_library(
    library: &Arc<Mutex<Library>>,
    meta: &downloader::SongMeta,
    url: &str,
    config: &Config,
    state: &Arc<Mutex<AppState>>,
) {
    let entry = crate::library::LibraryEntry {
        video_id: meta.video_id.clone(),
        title: meta.title.clone(),
        artist: meta.artist.clone(),
        url: url.to_string(),
        duration_secs: meta.duration_secs,
        file_path: format!("{}.mp3", meta.video_id),
        downloaded_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = library.lock().unwrap().add(entry) {
        warn!(?e, "failed to persist library entry");
    }

    // Also add to the in-memory library panel (deduplicate by URL)
    let mut s = state.lock().unwrap();
    if !s.library.iter().any(|song| song.url == url) {
        let mut song = Song::new_queued(&meta.title, &meta.artist, url);
        song.file_path = Some(config.cache_dir.join(format!("{}.mp3", meta.video_id)));
        song.duration = Some(Duration::from_secs_f64(meta.duration_secs));
        song.status = SongStatus::Ready;
        s.library.push(song);
        info!(title = %meta.title, "added song to library panel");
    }
}

fn build_context(state: &AppState) -> String {
    let mut ctx = String::new();

    if let Some(ref np) = state.current {
        ctx.push_str(&format!(
            "Now playing: {} - {}\n",
            np.song.title, np.song.artist
        ));
    } else {
        ctx.push_str("Now playing: nothing\n");
    }

    if state.library.is_empty() {
        ctx.push_str("Library: empty\n");
    } else {
        ctx.push_str("Library:\n");
        for (i, song) in state.library.iter().enumerate() {
            ctx.push_str(&format!("  {}. {}\n", i + 1, song.title));
        }
    }

    if state.queue.is_empty() {
        ctx.push_str("Queue: empty\n");
    } else {
        ctx.push_str("Queue:\n");
        for (i, song) in state.queue.iter().enumerate() {
            ctx.push_str(&format!(
                "  {}. {} ({:?})\n",
                i + 1,
                song.title,
                song.status
            ));
        }
    }

    ctx.push_str(&format!("Volume: {}\n", state.volume));
    ctx.push_str(&format!(
        "Paused: {}\n",
        if state.paused { "yes" } else { "no" }
    ));

    ctx
}
