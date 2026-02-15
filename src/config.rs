use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Config {
    pub api_key: String,
    pub model: String,
    pub cache_dir: PathBuf,
    pub library_path: PathBuf,
    pub default_volume: u8,
}

impl Config {
    pub fn load() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY environment variable not set")?;

        let cache_dir = dirs::home_dir()
            .context("Could not find home directory")?
            .join(".vibeplayer")
            .join("cache");

        std::fs::create_dir_all(&cache_dir)
            .context("Failed to create cache directory")?;

        let library_path = cache_dir.parent()
            .unwrap_or(&cache_dir)
            .join("library.json");

        Ok(Self {
            api_key,
            model: "claude-sonnet-4-5-20250929".to_string(),
            cache_dir,
            library_path,
            default_volume: 70,
        })
    }
}
