// src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RetentionPolicy {
    #[serde(default = "default_keep_daily")]
    pub keep_daily: u32,
    #[serde(default = "default_keep_weekly")]
    pub keep_weekly: u32,
    #[serde(default = "default_keep_monthly")]
    pub keep_monthly: u32,
    #[serde(default = "default_keep_yearly")]
    pub keep_yearly: u32,
}

// Default values for RetentionPolicy
fn default_keep_daily() -> u32 { 7 }
fn default_keep_weekly() -> u32 { 4 }
fn default_keep_monthly() -> u32 { 12 }
fn default_keep_yearly() -> u32 { 5 }

impl Default for RetentionPolicy {
    fn default() -> Self {
        RetentionPolicy {
            keep_daily: default_keep_daily(),
            keep_weekly: default_keep_weekly(),
            keep_monthly: default_keep_monthly(),
            keep_yearly: default_keep_yearly(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BackupJob {
    pub name: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)] // Allow retention_policy to be optional and use Default impl if not specified
    pub retention_policy: RetentionPolicy,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub jobs: Vec<BackupJob>,
}

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&config_str)?;
        Ok(config)
    }
}
