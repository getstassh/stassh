use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::db::Database;

pub const LATEST_DB_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct DatabaseV0 {
    pub name: Option<String>,
    pub index: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseV1 {
    pub index: u32,
}

pub fn migrate_db_value(value: Value) -> Result<(Database, bool)> {
    let mut changed = false;
    let mut current_value = value;
    let mut current_version = parse_version(&current_value).unwrap_or(0);

    loop {
        match current_version {
            0 => {
                let v0: DatabaseV0 = serde_json::from_value(current_value)
                    .context("failed to parse v0 database payload")?;
                let next = Database {
                    version: 1,
                    index: v0.index.unwrap_or(0),
                };
                current_value =
                    serde_json::to_value(&next).context("failed to serialize migrated v1 DB")?;
                current_version = 1;
                changed = true;
            }
            1 => {
                let v1: DatabaseV1 = serde_json::from_value(current_value)
                    .context("failed to parse v1 database payload")?;
                let next = Database {
                    version: 1,
                    index: v1.index,
                };
                return Ok((next, changed));
            }
            version => bail!("unsupported database version {version}"),
        }
    }
}

fn parse_version(value: &Value) -> Option<u32> {
    value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}
