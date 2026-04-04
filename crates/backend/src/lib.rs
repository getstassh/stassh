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
pub struct App {
    pub screen: Screen,
    pub state: AppState,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub app_name: String,

    pub config: Config,
    pub db: Database,

    pub password: Option<String>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let mut db = Database::default();
        let screen = match config.db_encryption {
            Some(DbEncryption::None) => {
                db = load_db(DbEncryption::None, None).unwrap_or_else(|_| Database::default());
                Screen::Dashboard
            }
            Some(DbEncryption::Passphrase) => Screen::AskingPassphrase {
                state: StringState::invisible(),
            },
            None => Screen::OnboardingWantsEncryption {
                state: YesNoState::new(),
            },
        };

        Self {
            screen,
            state: AppState {
                app_name: "Stassh".to_string(),
                config,
                db,
                password: None,
            },
        }
    }

    pub fn set_screen(&mut self, screen: Screen) {
        self.screen = screen;
    }

    pub fn app_name(&self) -> &str {
        &self.state.app_name
    }

    pub fn load_db(&mut self) -> Result<()> {
        let encryption = self
            .state
            .config
            .db_encryption
            .clone()
            .unwrap_or(DbEncryption::None);
        self.state.db = load_db(encryption, self.state.password.as_deref())?;
        Ok(())
    }

    pub fn save_db(&self) -> Result<()> {
        let encryption = self
            .state
            .config
            .db_encryption
            .clone()
            .unwrap_or(DbEncryption::None);
        save_db(&self.state.db, encryption, self.state.password.as_deref())?;
        Ok(())
    }

    pub fn save_config(&self) -> Result<()> {
        self.state.config.save_config()
    }

    pub fn delete_data(&mut self) -> Result<()> {
        config::delete_config()?;
        db::delete_db()?;
        Ok(())
    }
}
