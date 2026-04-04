mod config;
mod db;
mod db_crypto;
mod migrations;
mod screen;

pub use crate::config::Config;
pub use crate::db::{Database, DbEncryption, load_db, save_db};
pub use crate::screen::{Screen, StringState, YesNoState};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppState {
    pub app_name: String,
    pub started_timestamp: std::time::SystemTime,
    pub should_quit: bool,

    pub screen: Screen,
    pub config: Config,
    pub db: Database,

    pub password: Option<String>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            app_name: "stassh".to_string(),
            started_timestamp: Self::get_timestamp(),
            should_quit: false,
            screen: Screen::LoadingLogo,
            config,
            db: Database::default(),
            password: None,
        }
    }

    pub fn get_timestamp() -> std::time::SystemTime {
        std::time::SystemTime::now()
    }

    pub fn time_since_start(&self) -> std::time::Duration {
        self.started_timestamp.elapsed().unwrap_or_default()
    }

    pub fn set_screen(&mut self, screen: Screen) {
        self.screen = screen;
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    pub fn delete_data(&mut self) -> Result<()> {
        config::delete_config()?;
        db::delete_db()?;
        Ok(())
    }
}
