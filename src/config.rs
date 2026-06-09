// src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub source: PathBuf,
    pub destination: PathBuf,
    // Add more fields later for exclusion patterns, retention policy, etc.
}

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&config_str)?;
        Ok(config)
    }
}
