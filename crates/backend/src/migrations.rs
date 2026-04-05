use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    DbEncryption,
    config::Config,
    db::{Database, SshHost, TrustedHostKey},
};

pub(crate) const LATEST_DB_VERSION: &str = "3";

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "version")]
enum DatabaseAny {
    #[serde(rename = "0")]
    V0 {
        name: Option<String>,
        index: Option<u32>,
    },

    #[serde(rename = "1")]
    V1 { index: u32 },

    #[serde(rename = "2")]
    V2 {
        hosts: Vec<SshHost>,
        next_host_id: u32,
    },

    #[serde(rename = "3")]
    V3 {
        hosts: Vec<SshHost>,
        next_host_id: u32,
        trusted_host_keys: Vec<TrustedHostKey>,
    },
}

impl DatabaseAny {
    fn upgrade_one(&self) -> Option<Self> {
        match self {
            Self::V0 { index, .. } => Some(Self::V1 {
                index: (*index).unwrap_or(0),
            }),

            Self::V1 { index } => Some(Self::V2 {
                hosts: Vec::new(),
                next_host_id: (*index).max(1),
            }),

            Self::V2 {
                hosts,
                next_host_id,
            } => Some(Self::V3 {
                hosts: hosts.clone(),
                next_host_id: (*next_host_id).max(1),
                trusted_host_keys: Vec::new(),
            }),

            Self::V3 { .. } => None,
        }
    }

    fn into_latest(self) -> Database {
        match self {
            Self::V3 {
                hosts,
                next_host_id,
                trusted_host_keys,
            } => Database {
                version: "3",
                hosts,
                next_host_id: next_host_id.max(1),
                trusted_host_keys,
            },

            _ => unreachable!("database was not fully migrated"),
        }
    }
}

pub(crate) fn migrate_db_value(value: Value) -> Result<(Database, bool)> {
    let mut db: DatabaseAny = serde_json::from_value(value).context("failed to parse database")?;

    let mut changed = false;

    while let Some(next) = db.upgrade_one() {
        db = next;
        changed = true;
    }

    Ok((db.into_latest(), changed))
}

pub(crate) const LATEST_CONFIG_VERSION: &str = "6";
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "version")]
enum ConfigAny {
    #[serde(rename = "0")]
    V0 {
        enable_telemetry: Option<bool>,
        db_encryption: Option<DbEncryption>,
        show_sidebar: Option<bool>,
    },

    #[serde(rename = "1")]
    V1 {
        enable_telemetry: Option<bool>,
        db_encryption: Option<DbEncryption>,
        show_sidebar: bool,
        ssh_idle_timeout_seconds: u64,
    },

    #[serde(rename = "2")]
    V2 {
        enable_telemetry: Option<bool>,
        db_encryption: Option<DbEncryption>,
        show_sidebar: bool,
        ssh_idle_timeout_seconds: u64,
        ssh_connect_timeout_seconds: u64,
    },

    #[serde(rename = "3")]
    V3 {
        enable_telemetry: Option<bool>,
        db_encryption: Option<DbEncryption>,
        ssh_idle_timeout_seconds: u64,
        ssh_connect_timeout_seconds: u64,
    },

    #[serde(rename = "4")]
    V4 {
        enable_telemetry: Option<bool>,
        db_encryption: Option<DbEncryption>,
        show_debug_panel: bool,
        ssh_idle_timeout_seconds: u64,
        ssh_connect_timeout_seconds: u64,
    },

    #[serde(rename = "5")]
    V5 {
        enable_telemetry: Option<bool>,
        telemetry_uuid: Option<String>,
        db_encryption: Option<DbEncryption>,
        show_debug_panel: bool,
        ssh_idle_timeout_seconds: u64,
        ssh_connect_timeout_seconds: u64,
    },

    #[serde(rename = "6")]
    V6 {
        enable_telemetry: Option<bool>,
        telemetry_uuid: Option<String>,
        last_telemetry_report_at_unix_ms: Option<u64>,
        db_encryption: Option<DbEncryption>,
        show_debug_panel: bool,
        ssh_idle_timeout_seconds: u64,
        ssh_connect_timeout_seconds: u64,
    },
}

impl ConfigAny {
    fn upgrade_one(&self) -> Option<Self> {
        match self {
            Self::V0 {
                enable_telemetry,
                db_encryption,
                show_sidebar,
            } => Some(Self::V1 {
                enable_telemetry: *enable_telemetry,
                db_encryption: db_encryption.clone(),
                show_sidebar: show_sidebar.unwrap_or(true),
                ssh_idle_timeout_seconds: 600,
            }),
            Self::V1 {
                enable_telemetry,
                db_encryption,
                show_sidebar,
                ssh_idle_timeout_seconds,
            } => Some(Self::V2 {
                enable_telemetry: *enable_telemetry,
                db_encryption: db_encryption.clone(),
                show_sidebar: *show_sidebar,
                ssh_idle_timeout_seconds: *ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds: 5,
            }),
            Self::V2 {
                enable_telemetry,
                db_encryption,
                ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds,
                ..
            } => Some(Self::V3 {
                enable_telemetry: *enable_telemetry,
                db_encryption: db_encryption.clone(),
                ssh_idle_timeout_seconds: *ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds: *ssh_connect_timeout_seconds,
            }),
            Self::V3 {
                enable_telemetry,
                db_encryption,
                ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds,
            } => Some(Self::V4 {
                enable_telemetry: *enable_telemetry,
                db_encryption: db_encryption.clone(),
                show_debug_panel: false,
                ssh_idle_timeout_seconds: *ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds: *ssh_connect_timeout_seconds,
            }),
            Self::V4 {
                enable_telemetry,
                db_encryption,
                show_debug_panel,
                ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds,
            } => Some(Self::V5 {
                enable_telemetry: *enable_telemetry,
                telemetry_uuid: None,
                db_encryption: db_encryption.clone(),
                show_debug_panel: *show_debug_panel,
                ssh_idle_timeout_seconds: *ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds: *ssh_connect_timeout_seconds,
            }),
            Self::V5 {
                enable_telemetry,
                telemetry_uuid,
                db_encryption,
                show_debug_panel,
                ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds,
            } => Some(Self::V6 {
                enable_telemetry: *enable_telemetry,
                telemetry_uuid: telemetry_uuid.clone(),
                last_telemetry_report_at_unix_ms: None,
                db_encryption: db_encryption.clone(),
                show_debug_panel: *show_debug_panel,
                ssh_idle_timeout_seconds: *ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds: *ssh_connect_timeout_seconds,
            }),
            Self::V6 { .. } => None,
        }
    }

    fn into_latest(self) -> Config {
        match self {
            Self::V6 {
                enable_telemetry,
                telemetry_uuid,
                last_telemetry_report_at_unix_ms,
                db_encryption,
                show_debug_panel,
                ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds,
            } => Config {
                version: LATEST_CONFIG_VERSION,
                enable_telemetry,
                telemetry_uuid,
                last_telemetry_report_at_unix_ms,
                db_encryption,
                show_debug_panel,
                ssh_idle_timeout_seconds: ssh_idle_timeout_seconds.max(1),
                ssh_connect_timeout_seconds: ssh_connect_timeout_seconds.max(1),
            },
            _ => unreachable!("config was not fully migrated"),
        }
    }
}

pub(crate) fn migrate_config_value(value: Value) -> Result<(Config, bool)> {
    let mut config: ConfigAny = serde_json::from_value(value).context("failed to parse config")?;

    let mut changed = false;

    while let Some(next) = config.upgrade_one() {
        config = next;
        changed = true;
    }

    Ok((config.into_latest(), changed))
}
