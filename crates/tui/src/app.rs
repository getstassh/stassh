use std::{
    ops::{Deref, DerefMut},
    sync::mpsc::{self, TryRecvError},
    time::{SystemTime, UNIX_EPOCH},
};

use backend::{
    DbEncryption, ReleaseAsset, VersionCheckStatus, check_for_updates, start_update_install,
};
use uuid::Uuid;

use crate::navigation::{
    DashboardPage, DashboardState, Screen, StartupUpdateState, StringState, YesNoState,
};
use crate::telemetry;

const TELEMETRY_REPORT_INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;

pub(crate) struct App {
    pub(crate) screen: Screen,
    backend: backend::AppState,
    update_receiver: Option<mpsc::Receiver<VersionCheckStatus>>,
    boot_completed: bool,
}

impl App {
    pub(crate) fn new() -> Self {
        let mut backend = backend::AppState::new();

        let _screen = match backend.config.db_encryption.clone() {
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

        let mut app = Self {
            screen: Screen::StartupUpdateCheck {
                state: StartupUpdateState::new(env!("CARGO_PKG_VERSION").to_string()),
            },
            backend,
            update_receiver: None,
            boot_completed: false,
        };

        app.start_version_check();

        if matches!(app.screen, Screen::Dashboard { .. }) {
            app.maybe_report_telemetry();
        }

        app
    }

    fn start_version_check(&mut self) {
        self.version_status = VersionCheckStatus::Checking;
        let version = env!("CARGO_PKG_VERSION").to_string();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let status = match check_for_updates(&version) {
                Ok(status) => status,
                Err(err) => VersionCheckStatus::Error(err.to_string()),
            };
            let _ = tx.send(status);
        });

        self.update_receiver = Some(rx);
    }

    pub(crate) fn start_update_install(&mut self) {
        if let Screen::StartupUpdatePrompt { state } = &mut self.screen {
            if let (Some(url), Some(latest)) =
                (state.release_url.clone(), state.latest_version.clone())
            {
                state.phase = crate::navigation::StartupUpdatePhase::Downloading;
                state.install_receiver = Some(start_update_install(ReleaseAsset {
                    name: format!(
                        "stassh-v{}-{}.tar.gz",
                        latest,
                        option_env!("STASSH_BUILD_TARGET").unwrap_or("unknown")
                    ),
                    browser_download_url: url,
                }));
            }
        }
    }

    pub(crate) fn skip_update_gate(&mut self) {
        self.boot_completed = true;
        self.advance_boot_flow();
    }

    fn advance_boot_flow(&mut self) {
        if !self.boot_completed {
            return;
        }

        if let Screen::StartupUpdateCheck { .. } | Screen::StartupUpdatePrompt { .. } = self.screen
        {
            self.screen = self.normal_start_screen();
        }
    }

    fn normal_start_screen(&mut self) -> Screen {
        match self.backend.config.db_encryption.clone() {
            Some(DbEncryption::None) => {
                let _ = self.backend.load_db();
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
        }
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
        self.maybe_report_telemetry();
    }

    pub(crate) fn maybe_report_telemetry(&mut self) {
        if self.config.enable_telemetry != Some(true) {
            return;
        }

        let now = now_unix_ms();
        if self
            .config
            .last_telemetry_report_at_unix_ms
            .is_some_and(|last| now.saturating_sub(last) < TELEMETRY_REPORT_INTERVAL_MS)
        {
            return;
        }

        if self.config.telemetry_uuid.is_none() {
            self.config.telemetry_uuid = Some(Uuid::new_v4().to_string());
        }

        if let Some(uuid) = self.config.telemetry_uuid.clone() {
            telemetry::report_host_count_async(uuid, self.db.hosts.len());
            self.config.last_telemetry_report_at_unix_ms = Some(now);
            let _ = self.save_config();
        }
    }

    pub(crate) fn poll_version_check(&mut self) {
        if let Some(rx) = &self.update_receiver {
            match rx.try_recv() {
                Ok(status) => {
                    self.version_status = status;
                    self.update_receiver = None;
                    if matches!(self.screen, Screen::StartupUpdateCheck { .. }) {
                        self.screen = match &self.version_status {
                            VersionCheckStatus::UpdateAvailable {
                                current,
                                latest,
                                url,
                            } => Screen::StartupUpdatePrompt {
                                state: StartupUpdateState {
                                    phase: crate::navigation::StartupUpdatePhase::Prompt,
                                    current_version: current.to_string(),
                                    latest_version: Some(latest.to_string()),
                                    release_url: Some(url.clone()),
                                    message: None,
                                    spinner_frame: 0,
                                    downloaded: 0,
                                    total: None,
                                    install_receiver: None,
                                    install_started: false,
                                    skip_for_launch: false,
                                },
                            },
                            _ => {
                                self.boot_completed = true;
                                self.normal_start_screen()
                            }
                        };
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(_) => {
                    self.update_receiver = None;
                }
            }
        }
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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
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
