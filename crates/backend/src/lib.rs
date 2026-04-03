#[derive(Debug, Clone)]
pub struct AppState {
    app_name: String,
    tick_count: u64,
    should_quit: bool,
}

impl AppState {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            tick_count: 0,
            should_quit: false,
        }
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.saturating_add(1);
    }

    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;

    #[test]
    fn app_state_ticks_and_quits() {
        let mut app = AppState::new("stassh");

        assert_eq!(app.app_name(), "stassh");
        assert_eq!(app.tick_count(), 0);
        assert!(!app.should_quit());

        app.tick();
        app.tick();
        app.request_quit();

        assert_eq!(app.tick_count(), 2);
        assert!(app.should_quit());
    }
}
