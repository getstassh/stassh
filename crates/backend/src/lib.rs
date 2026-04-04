mod config;
mod db;
mod screen;

pub use crate::config::Config;
pub use crate::db::{Database, DbEncryption};
pub use crate::screen::{Screen, StringState, YesNoState};

#[derive(Debug, Clone)]
pub struct AppState {
    pub app_name: String,
    pub started_timestamp: std::time::SystemTime,
    pub target_screen: Screen,
    pub should_quit: bool,

    pub screen: Screen,
    pub config: Config,
    pub db: Database,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let mut default_screen = Screen::LoadingLogo;
        match config.db_encryption {
            None => {
                default_screen = Screen::OnboardingWantsEncryption {
                    state: YesNoState::new(),
                };
                // ask user if they want to encrypt the db, if yes, ask for passphrase and create new db with encryption
                // if no, create new db without encryption
            }
            Some(DbEncryption::None) => {
                default_screen = Screen::Dashboard;
                // load db without encryption
            }
            Some(DbEncryption::Passphrase) => {
                default_screen = Screen::AskingPassphrase {
                    passphrase: StringState::new(),
                };
                // ask for passphrase, load db with encryption
            }
        }
        Self {
            app_name: "stassh".to_string(),
            started_timestamp: Self::get_timestamp(),
            target_screen: default_screen,
            should_quit: false,
            screen: Screen::LoadingLogo,
            config,
            db: Database::default(),
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
}
