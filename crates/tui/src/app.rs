use std::ops::{Deref, DerefMut};

use backend::DbEncryption;

use crate::navigation::{Screen, StringState, YesNoState};

pub(crate) struct App {
    pub(crate) screen: Screen,
    backend: backend::AppState,
}

impl App {
    pub(crate) fn new() -> Self {
        let mut backend = backend::AppState::new();

        let screen = match backend.config.db_encryption.clone() {
            Some(DbEncryption::None) => {
                let _ = backend.load_db();
                Screen::Dashboard
            }
            Some(DbEncryption::Passphrase) => Screen::AskingPassphrase {
                state: StringState::invisible(),
            },
            None => Screen::OnboardingWantsEncryption {
                state: YesNoState::new(),
            },
        };

        Self { screen, backend }
    }

    pub(crate) fn state(&self) -> &backend::AppState {
        &self.backend
    }

    pub(crate) fn state_and_screen_mut(&mut self) -> (&backend::AppState, &mut Screen) {
        (&self.backend, &mut self.screen)
    }
}

impl Deref for App {
    type Target = backend::AppState;

    fn deref(&self) -> &Self::Target {
        &self.backend
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.backend
    }
}
