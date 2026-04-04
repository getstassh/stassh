use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DbEncryption {
    None,
    Passphrase,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Database {
    pub version: u32,
    pub name: Option<String>,
}

impl Database {
    pub fn default() -> Self {
        Self {
            version: 0,
            name: None,
        }
    }
}
