use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub url: String,
    pub duration_secs: f64,
    pub file_path: String,
    pub downloaded_at: String,
}

#[derive(Debug)]
pub struct Library {
    entries: Vec<LibraryEntry>,
    path: PathBuf,
}

impl Library {
    pub fn load(path: PathBuf) -> Result<Self> {
        let entries = if path.exists() {
            let data = std::fs::read_to_string(&path)
                .context("Failed to read library file")?;
            let entries: Vec<LibraryEntry> = serde_json::from_str(&data)
                .context("Failed to parse library JSON")?;
            info!(count = entries.len(), "library loaded from disk");
            entries
        } else {
            debug!(path = %path.display(), "library file not found, starting empty");
            Vec::new()
        };

        Ok(Self { entries, path })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create library directory")?;
        }
        let data = serde_json::to_string_pretty(&self.entries)
            .context("Failed to serialize library")?;
        std::fs::write(&self.path, data)
            .context("Failed to write library file")?;
        debug!(path = %self.path.display(), count = self.entries.len(), "library saved");
        Ok(())
    }

    pub fn add(&mut self, entry: LibraryEntry) -> Result<()> {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.video_id == entry.video_id) {
            info!(video_id = %entry.video_id, "updating existing library entry");
            *existing = entry;
        } else {
            info!(video_id = %entry.video_id, title = %entry.title, "adding new library entry");
            self.entries.push(entry);
        }
        self.save()
    }

    pub fn find_by_url(&self, url: &str) -> Option<&LibraryEntry> {
        self.entries.iter().find(|e| e.url == url)
    }

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }
}
