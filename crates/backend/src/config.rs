use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{
    db::DbEncryption,
    migrations::{LATEST_CONFIG_VERSION, migrate_config_value},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub version: &'static str,
    pub enable_telemetry: Option<bool>,
    pub db_encryption: Option<DbEncryption>,
    pub show_debug_panel: bool,
    pub ssh_idle_timeout_seconds: u64,
    pub ssh_connect_timeout_seconds: u64,
}

impl Config {
    pub(crate) fn default() -> Self {
        Self {
            version: LATEST_CONFIG_VERSION,
            enable_telemetry: None,
            db_encryption: None,
            show_debug_panel: false,
            ssh_idle_timeout_seconds: 600,
            ssh_connect_timeout_seconds: 5,
        }
    }

    pub(crate) fn load_config() -> Self {
        load_config().unwrap_or_else(|_| Self::default())
    }

    pub(crate) fn save_config(&self) -> Result<()> {
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

pub(crate) fn delete_config() -> Result<()> {
    let path = config_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn load_config() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        let config = Config::default();
        config.save_config()?;
        return Ok(config);
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;

    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse config JSON from {}", path.display()))?;

    let (config, changed) = migrate_config_value(value)?;

    if changed {
        config.save_config()?;
    }

    Ok(config)
}
