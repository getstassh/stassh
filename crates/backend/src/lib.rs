mod config;
mod db;
mod loader;

pub use crate::config::Config;
pub use crate::db::{Database, DbEncryption};

#[derive(Debug, Clone)]
pub struct AppState {
    app_name: String,
    should_quit: bool,
    config: Config,
    db: Database,
}

impl AppState {
    pub fn new(config: Config, db: Database) -> Self {
        Self {
            app_name: "stassh".to_string(),
            should_quit: false,
            config,
            db,
        }
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

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::db::Database;

    use super::AppState;

    #[test]
    fn app_state_ticks_and_quits() {
        let mut app = AppState::new(Config::default(), Database::default());

        assert_eq!(app.app_name(), "stassh");
        assert!(!app.should_quit());

        app.request_quit();

        assert!(app.should_quit());
    }
}
