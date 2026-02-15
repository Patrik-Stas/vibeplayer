---
id: 1
title: vibeplayer Design Document
created: 2026-02-14
updated: 2026-02-14
revision: 1
tags: [design, architecture, mvp]
---

# vibeplayer - Design Document

> A TUI-based YouTube vibe player powered by LLM intelligence.
> Paste a link, describe a mood, or name an artist — the player handles the rest.

---

## TUI Layout

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  > play something like mac miller but more upbeat_                              │
├────────────────────────────────────────────────────┬─────────────────────────────┤
│                                                    │  UP NEXT                    │
│                                                    │                             │
│                                                    │  1. Dang! - Mac Miller      │
│            ░▒▓█▓▒░    ░▒▓██▓▒░   ░▒▓█▓▒░          │     downloading...          │
│          ░▒▓████▓▒░░▒▓████████▓▒▓████▓▒░          │                             │
│        ░▒▓██████████████████████████████▓▒░        │  2. Best Day Ever           │
│      ░▒▓████████████████████████████████████▓░     │     - Mac Miller            │
│    ▁▂▃▅▆▇████▇▆▅▃▂▁▁▂▃▅▆▇████▇▆▅▃▂▁▂▃▅▆████▇▅▃  │     ready                   │
│    ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔  │                             │
│                                                    │  3. Circles - Post Malone   │
│                                                    │     queued                  │
│                                                    │                             │
│                                                    │  4. Sunflower - Rex O.C.    │
│                                                    │     queued                  │
│                                                    │                             │
│  Ladders - Mac Miller                              │                             │
│  The Swimming Album                                │                             │
│                                                    │                             │
│  [>>]  ━━━━━━━━━━━━━━●━━━━━━━━━━━━  2:34 / 4:12   │                             │
│                                                    │                             │
├────────────────────────────────────────────────────┴─────────────────────────────┤
│  [p] pause  [n] next  [s] skip  [q] quit           vol ████░░ 70%  [?] help     │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Layout Zones

| Zone             | Position    | Purpose                                      |
|------------------|-------------|----------------------------------------------|
| **Input Bar**    | Top row     | Natural language commands, URLs, search terms |
| **Visualizer**   | Center-left | ASCII audio visualization (spectrum bars)     |
| **Now Playing**  | Below viz   | Song title, artist, album, progress bar       |
| **Up Next**      | Right panel | Upcoming queue with download status           |
| **Status Bar**   | Bottom row  | Keyboard shortcuts, volume                    |

### Visualizer

The visualizer fills the "video player" area with a cheap real-time ASCII spectrum.
Approach: sample the audio amplitude at regular intervals and render as block characters
(`░▒▓█▁▂▃▄▅▆▇`). No FFT needed for MVP — simple amplitude-based bars are sufficient and
look good.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        TUI (ratatui)                        │
│                    renders from AppState                     │
│                    sends user input events                   │
└────────────────────────────┬────────────────────────────────┘
                             │
                       ┌─────▼─────┐
                       │ AppState  │
                       │ Arc<Mutex>│
                       └─────┬─────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
        ┌─────▼─────┐ ┌─────▼─────┐ ┌─────▼─────┐
        │ Downloader │ │  Player   │ │ LLM Agent │
        │  (yt-dlp)  │ │  (rodio)  │ │ (Claude)  │
        └───────────┘ └───────────┘ └───────────┘
```

### Core Components

**1. AppState** — Single shared state, protected by `Arc<Mutex<...>>`

```rust
struct AppState {
    queue: Vec<Song>,
    current: Option<NowPlaying>,
    input: InputState,
    agent_status: AgentStatus,  // idle | thinking | acting
    volume: u8,
    visualizer_data: Vec<f32>,
}

struct Song {
    title: String,
    artist: String,
    url: String,               // youtube URL
    file_path: Option<PathBuf>, // local mp3 path once downloaded
    status: SongStatus,         // queued | downloading | ready | playing | played
}
```

**2. Downloader** — Wraps yt-dlp subprocess

```rust
// Spawns yt-dlp as async subprocess
// Downloads to ~/.vibeplayer/cache/<hash>.mp3
// Updates Song.status and Song.file_path in AppState
async fn download_song(url: &str, state: Arc<Mutex<AppState>>) -> Result<PathBuf>
```

**3. Player** — Wraps rodio for audio playback

```rust
// Plays mp3 from local file
// Reports position/duration back to AppState
// Triggers next song download when approaching end of current track
```

**4. LLM Agent** — Claude API with tool calling

The brain of the application. Every input bar submission goes through the agent.

---

## LLM Agent Design

### Input Classification

The agent receives every input bar submission and decides what to do.
Some inputs can be short-circuited locally (pure URL, "pause", "skip")
but for MVP simplicity, all inputs go to the LLM.

### Tool Definitions

The LLM gets these tools to manipulate app state:

```
play_url(url: string)
    Download and play a YouTube URL immediately.

search_and_queue(query: string, count: int)
    Search YouTube for `query`, queue top `count` results.

queue_next(query: string)
    Search and insert a single song at the top of the queue.

skip()
    Skip current song.

pause() / resume()

set_volume(level: 0-100)

clear_queue()

replace_queue(queries: string[])
    Clear queue and populate with new searches.
    Used for "change the vibe" type commands.
```

### Example Interaction Flow

```
User types: "play something like mac miller but more upbeat"

  1. Input sent to Claude API with tool definitions + current queue context
  2. Claude responds with tool calls:
     - search_and_queue("mac miller upbeat songs", 3)
     - search_and_queue("anderson paak upbeat", 2)
  3. App executes: yt-dlp ytsearch for each query
  4. Songs appear in queue, first one starts downloading
  5. Playback begins when first download completes
```

### Context Sent to LLM

Each request includes:
- The user's input
- Currently playing song (title, artist)
- Current queue (titles only, to keep tokens low)
- Available tools

This lets the LLM make contextual decisions like "more of this" or "change the mood".

---

## Data Flow

### Startup
```
1. App launches -> render empty TUI
2. Input bar focused, cursor blinking
3. User enters URL or command
```

### Play a URL
```
1. User pastes YouTube URL
2. -> Agent recognizes URL -> calls play_url(url)
3. -> Downloader spawns yt-dlp, status = "downloading"
4. -> TUI shows "downloading..." in queue
5. -> Download completes, status = "ready"
6. -> Player starts playback, visualizer begins
7. -> Agent auto-generates similar song queries
8. -> Background downloads begin for upcoming songs
```

### Natural Language Command
```
1. User types "change upcoming songs to be more cheerful"
2. -> Agent receives input + current queue context
3. -> Agent calls replace_queue(["happy upbeat songs", "feel good hits", ...])
4. -> Queue clears, new songs populate
5. -> Downloads begin for new queue
```

---

## Project Structure

```
vibeplayer/
├── Cargo.toml
├── src/
│   ├── main.rs              # entry point, tokio runtime, event loop
│   ├── app.rs               # AppState definition and mutations
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── layout.rs        # ratatui layout (zones, panels)
│   │   ├── input_bar.rs     # input bar widget
│   │   ├── visualizer.rs    # ASCII spectrum visualization
│   │   ├── queue.rs         # "up next" panel
│   │   └── now_playing.rs   # song info + progress bar
│   ├── player.rs            # rodio playback wrapper
│   ├── downloader.rs        # yt-dlp subprocess wrapper
│   ├── agent.rs             # Claude API integration + tool dispatch
│   └── config.rs            # API keys, cache dir, settings
├── DESIGN.md
└── README.md
```

---

## Dependencies

| Crate        | Purpose                     |
|--------------|-----------------------------|
| `ratatui`    | TUI framework               |
| `crossterm`  | Terminal backend for ratatui |
| `tokio`      | Async runtime               |
| `rodio`      | Audio playback              |
| `reqwest`    | HTTP client (Claude API)    |
| `serde`      | JSON serialization          |
| `serde_json` | JSON parsing                |
| `dirs`       | XDG/home directory paths    |
| `clap`       | CLI argument parsing        |

### External Dependencies (must be installed by user)

| Tool     | Purpose                        |
|----------|--------------------------------|
| `yt-dlp` | YouTube audio downloading      |
| `ffmpeg` | Audio format conversion to mp3 |

---

## MVP Scope

### In Scope (v0.1)

- Input bar accepting URLs and natural language
- yt-dlp download to local cache
- rodio mp3 playback with play/pause/skip
- Simple amplitude-based ASCII visualizer
- "Up Next" queue panel
- Claude API integration with tool calling
- LLM-powered song discovery (via ytsearch)
- LLM-powered queue manipulation
- Volume control
- Keyboard shortcuts for basic controls
- Progress bar with elapsed/total time

### Out of Scope (Future)

| Feature                    | Notes                                                            |
|----------------------------|------------------------------------------------------------------|
| Audio effects              | Reverb, slowed, sped-up. Requires DSP pipeline (cpal) or FFmpeg pre-processing. Not trivial with rodio alone. |
| Playlist URL import        | Parse `&list=` param and import full YouTube playlists.          |
| Live effect toggling       | Real-time audio manipulation during playback.                    |
| Local LLM support          | Ollama/llama.cpp as alternative to Claude API. Needs tool calling support. |
| Configurable LLM provider  | OpenAI, Gemini, local — abstracted behind a trait.               |
| Last.fm integration        | Better song recommendations via similar artist/track API.        |
| Spotify API integration    | Recommendation engine for vibe matching.                         |
| Song metadata display      | Album art (ASCII), lyrics, genre tags.                           |
| Persistent history         | Remember past sessions, liked songs, preferences.                |
| Offline mode               | Play from cache without internet.                                |
| FFT-based visualizer       | Proper frequency spectrum instead of amplitude bars.             |
| Mouse support              | Click on queue items, progress bar seeking.                      |
| Multiple audio backends    | PulseAudio, ALSA, CoreAudio selection.                           |
| Song variance control      | User-adjustable "how different should suggestions be" knob.      |

---

## Configuration

```toml
# ~/.vibeplayer/config.toml

[api]
claude_api_key = "sk-ant-..."
model = "claude-sonnet-4-5-20250929"    # cost-effective for tool calling

[cache]
directory = "~/.vibeplayer/cache"
max_size_mb = 2000

[player]
default_volume = 70
```

API key can also be set via `ANTHROPIC_API_KEY` environment variable.

---

## Key Design Decisions

1. **yt-dlp as subprocess, not native Rust** — YouTube changes its API constantly. Only yt-dlp's community keeps pace. Every pure-Rust/Go/Node implementation breaks regularly. The subprocess boundary is clean and the dependency is acceptable.

2. **All input goes through LLM** — Rather than building a complex parser for commands vs URLs vs search terms, the LLM handles classification. This is simpler to build and more flexible. Trade-off: ~1s latency per command, API cost.

3. **Sonnet over Opus for agent** — Tool calling for this use case doesn't need Opus-level reasoning. Sonnet is faster and cheaper while being excellent at structured tool use.

4. **Single shared AppState behind Mutex** — Simple concurrency model. The TUI reads state on every tick, background tasks (downloader, player, agent) mutate through the mutex. No complex message passing needed at MVP scale.

5. **Pre-download next songs** — When a song starts playing, trigger downloads for the next 1-2 songs in queue. Eliminates gaps between tracks.
