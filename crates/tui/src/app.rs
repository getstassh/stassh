use std::ops::{Deref, DerefMut};

use backend::DbEncryption;

use crate::navigation::{DashboardPage, DashboardState, Screen, StringState, YesNoState};

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
                Screen::Dashboard {
                    state: DashboardState::new(),
                }
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

    pub(crate) fn go_to_dashboard(&mut self) {
        if self.config.enable_telemetry.is_none() {
            self.screen = Screen::OnboardingWantsTelemetry {
                state: YesNoState::new(),
            };
            return;
        }
        self.screen = Screen::Dashboard {
            state: DashboardState::new(),
        };
    }

    pub(crate) fn is_ssh_screen(&self) -> bool {
        matches!(&self.screen, Screen::Dashboard { state } if state.active_page == DashboardPage::Ssh)
    }

    pub(crate) fn has_modal_open(&self) -> bool {
        matches!(&self.screen, Screen::Dashboard { state } if state.host_modal.is_some() || state.quick_switcher.is_some())
    }

    pub(crate) fn toggle_debug_panel(&mut self) {
        self.config.show_debug_panel = !self.config.show_debug_panel;
        let debug_enabled = self.config.show_debug_panel;

        if let Screen::Dashboard { state } = &mut self.screen {
            if !debug_enabled && state.active_page == DashboardPage::Debug {
                state.active_page = DashboardPage::Home;
            }

            state.last_status = Some(if debug_enabled {
                "Debug panel enabled".to_string()
            } else {
                "Debug panel disabled".to_string()
            });
        }

        let _ = self.save_config();
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
