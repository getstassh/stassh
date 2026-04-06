use std::{collections::HashMap, thread::JoinHandle, time::Instant};

use crate::ssh_client::{LiveSshSession, PendingSshStart, TrustChallenge};

pub(crate) enum Screen {
    StartupUpdateCheck { state: StartupUpdateState },
    StartupUpdatePrompt { state: StartupUpdateState },
    OnboardingWantsEncryption { state: YesNoState },
    OnboardingWantsPassphrase { state: StringState },
    OnboardingWantsTelemetry { state: YesNoState },
    AskingPassphrase { state: StringState },
    Dashboard { state: DashboardState },
}

pub(crate) struct SshSessionState {
    pub(crate) title: String,
    pub(crate) parser: vt100::Parser,
    pub(crate) phase: SshSessionPhase,
    pub(crate) last_good_rows: u16,
    pub(crate) last_good_cols: u16,
}

impl SshSessionState {
    pub(crate) fn new_starting(title: String, rows: u16, cols: u16, host_id: u32) -> Self {
        Self {
            title,
            parser: vt100::Parser::new(rows, cols, 10_000),
            phase: SshSessionPhase::starting(host_id),
            last_good_rows: rows.max(1),
            last_good_cols: cols.max(1),
        }
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        self.last_good_rows = rows.max(1);
        self.last_good_cols = cols.max(1);
        self.parser.screen_mut().set_size(rows, cols);
    }
}

pub(crate) enum SshSessionPhase {
    Starting {
        host_id: u32,
        pending: Option<PendingSshStart>,
        spinner_frame: usize,
        started_at: Instant,
    },
    TrustPrompt {
        host_id: u32,
        challenge: TrustChallenge,
    },
    Running {
        live: LiveSshSession,
    },
}

impl SshSessionPhase {
    pub(crate) fn starting(host_id: u32) -> Self {
        Self::Starting {
            host_id,
            pending: None,
            spinner_frame: 0,
            started_at: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DashboardPage {
    Home,
    Settings,
    Debug,
    Ssh,
}

pub(crate) struct DashboardState {
    pub(crate) active_page: DashboardPage,
    pub(crate) selected_host: usize,
    pub(crate) host_modal: Option<HostModalState>,
    pub(crate) quick_switcher: Option<QuickSwitcherState>,
    pub(crate) last_status: Option<String>,
    pub(crate) debug_scroll: u16,
    pub(crate) host_statuses: HashMap<u32, Vec<HostConnectionStatus>>,
    pub(crate) probe_tasks: Vec<HostProbeTask>,
    pub(crate) last_probe_at: Instant,
    pub(crate) needs_initial_probe: bool,
    pub(crate) ssh_tabs: Vec<SshSessionState>,
    pub(crate) active_ssh_tab: Option<usize>,
    pub(crate) debug_hold_started_at: Option<Instant>,
    pub(crate) debug_hold_last_seen_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StartupUpdatePhase {
    Checking,
    Prompt,
    Downloading,
    Verifying,
    Installing,
    Done,
    Failed,
}

#[derive(Debug)]
pub(crate) struct StartupUpdateState {
    pub(crate) phase: StartupUpdatePhase,
    pub(crate) current_version: String,
    pub(crate) latest_version: Option<String>,
    pub(crate) release_url: Option<String>,
    pub(crate) asset: Option<backend::ReleaseAsset>,
    pub(crate) checksum_asset: Option<backend::ReleaseAsset>,
    pub(crate) message: Option<String>,
    pub(crate) spinner_frame: usize,
    pub(crate) downloaded: u64,
    pub(crate) total: Option<u64>,
    pub(crate) install_receiver: Option<std::sync::mpsc::Receiver<backend::UpdateInstallStatus>>,
}

impl StartupUpdateState {
    pub(crate) fn new(current_version: String) -> Self {
        Self {
            phase: StartupUpdatePhase::Checking,
            current_version,
            latest_version: None,
            release_url: None,
            asset: None,
            checksum_asset: None,
            message: None,
            spinner_frame: 0,
            downloaded: 0,
            total: None,
            install_receiver: None,
        }
    }
}

impl DashboardState {
    pub(crate) fn new() -> Self {
        Self {
            active_page: DashboardPage::Home,
            selected_host: 0,
            host_modal: None,
            quick_switcher: None,
            last_status: None,
            debug_scroll: 0,
            host_statuses: HashMap::new(),
            probe_tasks: Vec::new(),
            last_probe_at: Instant::now(),
            needs_initial_probe: true,
            ssh_tabs: Vec::new(),
            active_ssh_tab: None,
            debug_hold_started_at: None,
            debug_hold_last_seen_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QuickSwitcherState {
    pub(crate) query: String,
    pub(crate) selected_idx: usize,
}

impl QuickSwitcherState {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            selected_idx: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostConnectionStatus {
    Unknown,
    Reachable,
    Unreachable,
}

pub(crate) struct HostProbeTask {
    pub(crate) host_id: u32,
    pub(crate) join: JoinHandle<Vec<HostConnectionStatus>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HostModalMode {
    Create,
    Edit { host_id: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HostAuthMode {
    Key,
    Password,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HostFormField {
    Name,
    User,
    Endpoints,
    AuthMode,
    AuthValue,
}

impl HostFormField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Name => Self::User,
            Self::User => Self::Endpoints,
            Self::Endpoints => Self::AuthMode,
            Self::AuthMode => Self::AuthValue,
            Self::AuthValue => Self::Name,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Name => Self::AuthValue,
            Self::User => Self::Name,
            Self::Endpoints => Self::User,
            Self::AuthMode => Self::Endpoints,
            Self::AuthValue => Self::AuthMode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HostKeyInputMode {
    Path,
    Inline,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HostFormState {
    pub(crate) focus: HostFormField,
    pub(crate) name: String,
    pub(crate) user: String,
    pub(crate) endpoints: String,
    pub(crate) auth_mode: HostAuthMode,
    pub(crate) key_input_mode: HostKeyInputMode,
    pub(crate) key_path: String,
    pub(crate) key_inline: String,
    pub(crate) password: String,
    pub(crate) caret: usize,
    pub(crate) error: Option<String>,
}

impl HostFormState {
    pub(crate) fn new() -> Self {
        Self {
            focus: HostFormField::Name,
            name: String::new(),
            user: String::new(),
            endpoints: String::new(),
            auth_mode: HostAuthMode::Key,
            key_input_mode: HostKeyInputMode::Path,
            key_path: String::new(),
            key_inline: String::new(),
            password: String::new(),
            caret: 0,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HostModalState {
    pub(crate) mode: HostModalMode,
    pub(crate) form: HostFormState,
    pub(crate) key_picker: Option<HostKeyPickerState>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HostKeyPickerState {
    pub(crate) options: Vec<String>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct YesNoState {
    pub(crate) selected: bool,
}

impl YesNoState {
    pub(crate) fn new() -> Self {
        Self { selected: true }
    }

    pub(crate) fn toggle(&mut self) {
        self.selected = !self.selected;
    }

    pub(crate) fn is_yes(&self) -> bool {
        self.selected
    }

    pub(crate) fn is_no(&self) -> bool {
        !self.selected
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StringState {
    pub(crate) is_visible: bool,
    pub(crate) text: String,
    pub(crate) caret_position: usize,
    pub(crate) error: Option<String>,
}

impl StringState {
    pub(crate) fn invisible() -> Self {
        Self {
            is_visible: false,
            text: String::new(),
            caret_position: 0,
            error: None,
        }
    }

    pub(crate) fn invisible_with_error(error: String) -> Self {
        Self {
            is_visible: false,
            text: String::new(),
            caret_position: 0,
            error: Some(error),
        }
    }

    pub(crate) fn set_text(&mut self, text: String) {
        self.text = text;
    }

    pub(crate) fn visible_text(&self) -> String {
        let text = if self.is_visible {
            self.text.clone()
        } else {
            "*".repeat(self.text.len())
        };

        format!("{} ", text)
    }
}
