mod config;
mod db;
mod sql_migrations;
mod update;
mod version;

pub use crate::config::Config;
pub use crate::db::{
    Database, DbEncryption, DbOpenStatus, HostAuth, SshEndpoint, SshHost, TrustedHostKey,
};
pub use crate::update::{
    ReleaseAsset, UpdateCheckStatus, UpdateInstallStatus, check_for_updates as check_for_update,
    start_update_install,
};
pub use crate::version::{VersionCheckStatus, check_for_updates};

use crate::db::{load_state, save_config_only, save_state};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppState {
    pub app_name: String,

    pub config: Config,
    pub db: Database,

    pub version_status: VersionCheckStatus,

    pub password: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            app_name: "Stassh".to_string(),
            config: Config::default(),
            db: Database::default(),
            version_status: VersionCheckStatus::Idle,
            password: None,
        }
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn load_db(&mut self) -> Result<()> {
        let (db, config) = load_state(self.password.as_deref())?;
        self.db = db;
        self.config = config;
        Ok(())
    }

    pub fn save_db(&self) -> Result<()> {
        let encryption = self
            .config
            .db_encryption
            .clone()
            .unwrap_or(DbEncryption::None);
        save_state(&self.db, &self.config, encryption, self.password.as_deref())?;
        Ok(())
    }

    pub fn save_config(&self) -> Result<()> {
        let encryption = self
            .config
            .db_encryption
            .clone()
            .unwrap_or(DbEncryption::None);
        save_config_only(&self.config, encryption, self.password.as_deref())
    }

    pub fn is_correct_password(&self, passphrase: &str) -> bool {
        db::is_correct_password(passphrase).unwrap_or(false)
    }

    pub fn db_open_status(&self) -> DbOpenStatus {
        db::db_open_status().unwrap_or(DbOpenStatus::Missing)
    }

    pub fn delete_data(&mut self) -> Result<()> {
        config::delete_config()?;
        db::delete_db()?;
        Ok(())
    }
}
