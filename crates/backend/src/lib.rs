mod config;
mod db;
mod db_crypto;
mod migrations;

pub use crate::config::Config;
pub use crate::db::{Database, DbEncryption};

use crate::db::{load_db, save_db};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppState {
    pub app_name: String,

    pub config: Config,
    pub db: Database,

    pub password: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        let config = Config::load_config();
        Self {
            app_name: "Stassh".to_string(),
            config,
            db: Database::default(),
            password: None,
        }
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn load_db(&mut self) -> Result<()> {
        let encryption = self
            .config
            .db_encryption
            .clone()
            .unwrap_or(DbEncryption::None);
        self.db = load_db(encryption, self.password.as_deref())?;
        Ok(())
    }

    pub fn save_db(&self) -> Result<()> {
        let encryption = self
            .config
            .db_encryption
            .clone()
            .unwrap_or(DbEncryption::None);
        save_db(&self.db, encryption, self.password.as_deref())?;
        Ok(())
    }

    pub fn save_config(&self) -> Result<()> {
        self.config.save_config()
    }

    pub fn delete_data(&mut self) -> Result<()> {
        config::delete_config()?;
        db::delete_db()?;
        Ok(())
    }
}
