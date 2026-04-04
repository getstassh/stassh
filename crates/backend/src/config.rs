use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::db::DbEncryption;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub enable_telemetry: Option<bool>,
    pub db_encryption: Option<DbEncryption>,
}

impl Config {
    pub fn default() -> Self {
        Self {
            enable_telemetry: None,
            db_encryption: None,
        }
    }

    pub fn load_config() -> Self {
        load_config().unwrap_or_else(|_| Self::default())
    }

    pub fn save_config(&self) -> Result<()> {
        let path = config_path()?;
        let text = serde_json::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("com", "bylazar", "stassh")
        .ok_or_else(|| anyhow::anyhow!("could not determine app dirs"))
}

fn config_path() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    let dir = dirs.config_dir();
    fs::create_dir_all(dir)?;
    Ok(dir.join("config.json"))
}

pub fn delete_config() -> Result<()> {
    let path = config_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        return Ok(Config::default());
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;

    let config = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse config JSON from {}", path.display()))?;

    Ok(config)
}
