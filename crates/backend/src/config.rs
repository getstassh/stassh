use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::DbEncryption;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub enable_telemetry: Option<bool>,
    pub telemetry_uuid: Option<String>,
    pub last_telemetry_report_at_unix_ms: Option<u64>,
    pub db_encryption: Option<DbEncryption>,
    pub show_debug_panel: bool,
    pub ssh_idle_timeout_seconds: u64,
    pub ssh_connect_timeout_seconds: u64,
}

impl Config {
    pub(crate) fn default() -> Self {
        Self {
            enable_telemetry: None,
            telemetry_uuid: None,
            last_telemetry_report_at_unix_ms: None,
            db_encryption: None,
            show_debug_panel: false,
            ssh_idle_timeout_seconds: 600,
            ssh_connect_timeout_seconds: 5,
        }
    }
}

pub(crate) fn delete_config() -> Result<()> {
    Ok(())
}
