use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub duration_secs: Option<f64>,
}

/// Quick title fetch — faster than full metadata since we only need one field.
pub async fn get_title(url: &str) -> Result<String> {
    info!(%url, "fetching title via yt-dlp");
    let output = Command::new("yt-dlp")
        .args(["--print", "%(title)s", "--no-download", "--no-playlist", url])
        .output()
        .await
        .context("Failed to run yt-dlp")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(%url, %stderr, "yt-dlp get_title failed");
        anyhow::bail!("yt-dlp failed: {}", stderr);
    }

    let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
    info!(%url, %title, "title fetched");
    Ok(title)
}

pub async fn download_song(url: &str, config: &Config) -> Result<(PathBuf, SongMeta)> {
    info!(%url, "starting song download");
    let output_template = config
        .cache_dir
        .join("%(id)s.%(ext)s")
        .to_string_lossy()
        .to_string();

    // First get metadata
    info!(%url, "fetching metadata");
    let meta_output = Command::new("yt-dlp")
        .args([
            "--print", "%(title)s\n%(uploader)s\n%(duration)s\n%(id)s",
            "--no-download",
            url,
        ])
        .output()
        .await
        .context("Failed to run yt-dlp (is it installed?)")?;

    if !meta_output.status.success() {
        let stderr = String::from_utf8_lossy(&meta_output.stderr);
        error!(%url, %stderr, "yt-dlp metadata fetch failed");
        anyhow::bail!("yt-dlp metadata failed: {}", stderr);
    }

    let meta_str = String::from_utf8_lossy(&meta_output.stdout);
    let meta_lines: Vec<&str> = meta_str.trim().lines().collect();
    debug!(%url, ?meta_lines, "raw metadata lines");

    let title = meta_lines.first().unwrap_or(&"Unknown").to_string();
    let artist = meta_lines.get(1).unwrap_or(&"Unknown").to_string();
    let duration_secs: f64 = meta_lines
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let video_id = meta_lines.get(3).unwrap_or(&"unknown").to_string();

    info!(%title, %artist, %video_id, duration_secs, "metadata parsed");

    let file_path = config.cache_dir.join(format!("{}.mp3", video_id));

    // Skip download if already cached
    if file_path.exists() {
        info!(path = %file_path.display(), "using cached file");
    } else {
        info!(%url, path = %file_path.display(), "downloading audio");
        let dl_output = Command::new("yt-dlp")
            .args([
                "-x",
                "--audio-format",
                "mp3",
                "--audio-quality",
                "5",
                "-o",
                &output_template,
                "--no-playlist",
                url,
            ])
            .output()
            .await
            .context("yt-dlp download failed")?;

        if !dl_output.status.success() {
            let stderr = String::from_utf8_lossy(&dl_output.stderr);
            error!(%url, %stderr, "yt-dlp download failed");
            anyhow::bail!("yt-dlp failed: {}", stderr);
        }
        info!(path = %file_path.display(), "download complete");
    }

    Ok((
        file_path,
        SongMeta {
            title,
            artist,
            duration_secs,
            video_id,
        },
    ))
}

pub async fn search_youtube(query: &str, count: u32) -> Result<Vec<SearchResult>> {
    let search_query = format!("ytsearch{}:{}", count, query);
    info!(%search_query, "searching YouTube");

    let output = Command::new("yt-dlp")
        .args([
            "--print",
            "%(title)s\t%(webpage_url)s\t%(duration)s",
            "--no-download",
            "--flat-playlist",
            &search_query,
        ])
        .output()
        .await
        .context("yt-dlp search failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(%search_query, %stderr, "yt-dlp search failed");
        anyhow::bail!("yt-dlp search failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    debug!(%search_query, raw_output = %stdout, "search raw output");

    let results: Vec<SearchResult> = stdout
        .trim()
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() >= 2 {
                Some(SearchResult {
                    title: parts[0].to_string(),
                    url: parts[1].to_string(),
                    duration_secs: parts.get(2).and_then(|s| s.parse().ok()),
                })
            } else {
                warn!(%line, "unparseable search result line");
                None
            }
        })
        .collect();

    info!(%search_query, result_count = results.len(), "search complete");
    for (i, r) in results.iter().enumerate() {
        debug!(index = i, title = %r.title, url = %r.url, "search result");
    }

    Ok(results)
}

#[derive(Debug, Clone)]
pub struct SongMeta {
    pub title: String,
    pub artist: String,
    pub duration_secs: f64,
    pub video_id: String,
}
