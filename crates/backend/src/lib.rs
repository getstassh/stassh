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

use crate::db::{
    automatic_backup_retention_count, backup_count, load_state, maybe_create_automatic_backup,
    rekey_database, save_config_only, save_state,
};

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
        let _ = maybe_create_automatic_backup(self.password.as_deref());
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

    pub fn backup_count(&self) -> Option<usize> {
        backup_count().ok()
    }

    pub fn automatic_backup_retention_count(&self) -> usize {
        automatic_backup_retention_count()
    }

    pub fn enable_encryption_with_passphrase(&mut self, new_passphrase: &str) -> Result<()> {
        rekey_database(None, Some(new_passphrase))?;
        self.password = Some(new_passphrase.to_string());
        self.config.db_encryption = Some(DbEncryption::Passphrase);
        self.save_config()
    }

    pub fn change_db_passphrase(
        &mut self,
        current_passphrase: &str,
        new_passphrase: &str,
    ) -> Result<()> {
        rekey_database(Some(current_passphrase), Some(new_passphrase))?;
        self.password = Some(new_passphrase.to_string());
        self.config.db_encryption = Some(DbEncryption::Passphrase);
        self.save_config()
    }

    pub fn remove_db_passphrase(&mut self, current_passphrase: &str) -> Result<()> {
        rekey_database(Some(current_passphrase), None)?;
        self.password = None;
        self.config.db_encryption = Some(DbEncryption::None);
        self.save_config()
    }

    pub fn delete_data(&mut self) -> Result<()> {
        config::delete_config()?;
        db::delete_db()?;
        Ok(())
    }
}
