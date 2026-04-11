use std::{
    ops::{Deref, DerefMut},
    sync::mpsc::{self, TryRecvError},
    time::{SystemTime, UNIX_EPOCH},
};

use backend::{
    DbOpenStatus, UpdateCheckStatus, VersionCheckStatus, check_for_update, start_update_install,
};
use uuid::Uuid;

use crate::navigation::{
    DashboardPage, DashboardState, DashboardUpdatePromptState, Screen, StartupUpdateState,
    StringState, YesNoState,
};
use crate::telemetry;

const TELEMETRY_REPORT_INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;

pub(crate) struct App {
    pub(crate) screen: Screen,
    backend: backend::AppState,
    update_receiver: Option<mpsc::Receiver<UpdateCheckStatus>>,
    pending_update_prompt: Option<DashboardUpdatePromptState>,
    boot_completed: bool,
    exit_requested: bool,
    restart_requested: bool,
}

impl App {
    pub(crate) fn new() -> Self {
        let backend = backend::AppState::new();

        let mut app = Self {
            screen: Screen::OnboardingWantsEncryption {
                state: YesNoState::new(),
            },
            backend,
            update_receiver: None,
            pending_update_prompt: None,
            boot_completed: false,
            exit_requested: false,
            restart_requested: false,
        };

        app.boot_completed = true;
        app.screen = app.normal_start_screen();

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
            let status = match check_for_update(&version) {
                Ok(status) => status,
                Err(err) => UpdateCheckStatus::Error(err.to_string()),
            };
            let _ = tx.send(status);
        });

        self.update_receiver = Some(rx);
    }

    pub(crate) fn start_update_install_from_dashboard_prompt(&mut self) {
        if let Screen::Dashboard { state } = &mut self.screen
            && let Some(prompt) = state.update_prompt.take()
        {
            let install_receiver = Some(start_update_install(
                prompt.asset.clone(),
                prompt.checksum_asset.clone(),
            ));
            self.screen = Screen::StartupUpdatePrompt {
                state: StartupUpdateState {
                    phase: crate::navigation::StartupUpdatePhase::Downloading,
                    message: None,
                    spinner_frame: 0,
                    downloaded: 0,
                    total: None,
                    install_receiver,
                },
            };
        }
    }

    pub(crate) fn skip_update_gate(&mut self) {
        self.boot_completed = true;
        self.advance_boot_flow();
    }

    pub(crate) fn request_restart_and_exit(&mut self) {
        self.restart_requested = true;
        self.exit_requested = true;
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub(crate) fn restart_requested(&self) -> bool {
        self.restart_requested
    }

    fn advance_boot_flow(&mut self) {
        if !self.boot_completed {
            return;
        }

        if let Screen::StartupUpdatePrompt { .. } = self.screen {
            self.screen = self.normal_start_screen();
        }
    }

    fn normal_start_screen(&mut self) -> Screen {
        match self.backend.db_open_status() {
            DbOpenStatus::Plain => {
                let _ = self.backend.load_db();
                let mut state = DashboardState::new();
                state.update_prompt = self.pending_update_prompt.take();
                Screen::Dashboard { state }
            }
            DbOpenStatus::PassphraseRequired => Screen::AskingPassphrase {
                state: StringState::invisible(),
            },
            DbOpenStatus::Missing => Screen::OnboardingWantsEncryption {
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
        let mut state = DashboardState::new();
        state.update_prompt = self.pending_update_prompt.take();
        self.screen = Screen::Dashboard { state };
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
                    self.version_status = match &status {
                        UpdateCheckStatus::NoUpdate { current } => VersionCheckStatus::UpToDate {
                            current: current.clone(),
                        },
                        UpdateCheckStatus::UpdateAvailable {
                            current,
                            latest,
                            release_url,
                            ..
                        } => VersionCheckStatus::UpdateAvailable {
                            current: current.clone(),
                            latest: latest.clone(),
                            url: release_url.clone(),
                        },
                        UpdateCheckStatus::Error(err) => VersionCheckStatus::Error(err.clone()),
                    };
                    self.update_receiver = None;
                    if let UpdateCheckStatus::UpdateAvailable {
                        current,
                        latest,
                        release_url,
                        asset,
                        checksum_asset,
                    } = status
                    {
                        let prompt = DashboardUpdatePromptState {
                            current_version: current.to_string(),
                            latest_version: latest.to_string(),
                            release_url,
                            asset,
                            checksum_asset,
                            choice: YesNoState::new(),
                        };

                        if let Screen::Dashboard { state } = &mut self.screen {
                            state.update_prompt = Some(prompt);
                        } else {
                            self.pending_update_prompt = Some(prompt);
                        }
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
        matches!(&self.screen, Screen::Dashboard { state } if state.host_modal.is_some() || state.endpoint_picker.is_some() || state.quick_switcher.is_some() || state.settings_modal.is_some() || state.update_prompt.is_some())
    }

    pub(crate) fn is_quick_switcher_open(&self) -> bool {
        matches!(&self.screen, Screen::Dashboard { state } if state.quick_switcher.is_some())
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
