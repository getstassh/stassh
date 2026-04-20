use std::{collections::HashMap, thread::JoinHandle, time::Instant};

use crate::ssh_client::{LiveSshSession, PendingSshStart, TrustChallenge};

pub(crate) enum Screen {
    StartupUpdatePrompt { state: StartupUpdateState },
    OnboardingWantsEncryption { state: YesNoState },
    OnboardingWantsPassphrase { state: OnboardingPassphraseState },
    OnboardingWantsTelemetry { state: YesNoState },
    AskingPassphrase { state: StringState },
    Dashboard { state: DashboardState },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnboardingPassphraseField {
    Passphrase,
    Confirm,
}

impl OnboardingPassphraseField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Passphrase => Self::Confirm,
            Self::Confirm => Self::Passphrase,
        }
    }

    pub(crate) fn prev(self) -> Self {
        self.next()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OnboardingPassphraseState {
    pub(crate) focus: OnboardingPassphraseField,
    pub(crate) passphrase: StringState,
    pub(crate) confirm_passphrase: StringState,
    pub(crate) error: Option<String>,
}

impl OnboardingPassphraseState {
    pub(crate) fn new() -> Self {
        Self {
            focus: OnboardingPassphraseField::Passphrase,
            passphrase: StringState::invisible(),
            confirm_passphrase: StringState::invisible(),
            error: None,
        }
    }
}

pub(crate) struct SshSessionState {
    pub(crate) title: String,
    pub(crate) parser: vt100::Parser,
    pub(crate) phase: SshSessionPhase,
    pub(crate) last_good_rows: u16,
    pub(crate) last_good_cols: u16,
}

impl SshSessionState {
    pub(crate) fn new_starting(
        title: String,
        rows: u16,
        cols: u16,
        host_id: u32,
        selected_endpoint_index: Option<usize>,
    ) -> Self {
        Self {
            title,
            parser: vt100::Parser::new(rows, cols, 10_000),
            phase: SshSessionPhase::starting(host_id, selected_endpoint_index),
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
        selected_endpoint_index: Option<usize>,
        pending: Option<PendingSshStart>,
        spinner_frame: usize,
        started_at: Instant,
    },
    TrustPrompt {
        host_id: u32,
        selected_endpoint_index: Option<usize>,
        challenge: TrustChallenge,
        choice: YesNoState,
    },
    Running {
        live: LiveSshSession,
    },
}

impl SshSessionPhase {
    pub(crate) fn starting(host_id: u32, selected_endpoint_index: Option<usize>) -> Self {
        Self::Starting {
            host_id,
            selected_endpoint_index,
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
    Ssh,
}

pub(crate) struct DashboardState {
    pub(crate) active_page: DashboardPage,
    pub(crate) selected_host: usize,
    pub(crate) host_modal: Option<HostModalState>,
    pub(crate) endpoint_picker: Option<EndpointPickerState>,
    pub(crate) quick_switcher: Option<QuickSwitcherState>,
    pub(crate) last_status: Option<String>,
    pub(crate) host_statuses: HashMap<u32, Vec<HostConnectionStatus>>,
    pub(crate) probe_tasks: Vec<HostProbeTask>,
    pub(crate) last_probe_at: Instant,
    pub(crate) needs_initial_probe: bool,
    pub(crate) ssh_tabs: Vec<SshSessionState>,
    pub(crate) active_ssh_tab: Option<usize>,
    pub(crate) settings_selected_row: usize,
    pub(crate) settings_modal: Option<SettingsSecurityModalState>,
    pub(crate) settings_backup_modal: Option<SettingsBackupModalState>,
    pub(crate) update_prompt: Option<DashboardUpdatePromptState>,
}

#[derive(Debug, Clone)]
pub(crate) struct DashboardUpdatePromptState {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) release_url: String,
    pub(crate) asset: backend::ReleaseAsset,
    pub(crate) checksum_asset: Option<backend::ReleaseAsset>,
    pub(crate) choice: YesNoState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSecurityAction {
    EnableEncryption,
    ChangePassphrase,
    RemovePassphrase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSecurityField {
    Current,
    New,
    Confirm,
    DangerConfirm,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsSecurityModalState {
    pub(crate) action: SettingsSecurityAction,
    pub(crate) focus: SettingsSecurityField,
    pub(crate) current_passphrase: StringState,
    pub(crate) new_passphrase: StringState,
    pub(crate) confirm_passphrase: StringState,
    pub(crate) danger_confirm: YesNoState,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsBackupAction {
    CopyDbBlob,
    RestoreDbBlob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsBackupField {
    Blob,
    Passphrase,
    DangerConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsBackupRestoreStage {
    Blob,
    Passphrase,
    Confirm,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsBackupModalState {
    pub(crate) action: SettingsBackupAction,
    pub(crate) focus: SettingsBackupField,
    pub(crate) restore_stage: SettingsBackupRestoreStage,
    pub(crate) blob: StringState,
    pub(crate) passphrase: StringState,
    pub(crate) danger_confirm: YesNoState,
    pub(crate) requires_passphrase: bool,
    pub(crate) copy_feedback: Option<String>,
    pub(crate) error: Option<String>,
}

impl SettingsBackupModalState {
    pub(crate) fn for_action(action: SettingsBackupAction) -> Self {
        let mut blob = StringState::invisible();
        blob.is_visible = true;

        Self {
            action,
            focus: SettingsBackupField::Blob,
            restore_stage: SettingsBackupRestoreStage::Blob,
            blob,
            passphrase: StringState::invisible(),
            danger_confirm: YesNoState { selected: false },
            requires_passphrase: false,
            copy_feedback: None,
            error: None,
        }
    }
}

impl SettingsSecurityModalState {
    pub(crate) fn for_action(action: SettingsSecurityAction) -> Self {
        let focus = match action {
            SettingsSecurityAction::EnableEncryption => SettingsSecurityField::New,
            SettingsSecurityAction::ChangePassphrase => SettingsSecurityField::Current,
            SettingsSecurityAction::RemovePassphrase => SettingsSecurityField::Current,
        };

        Self {
            action,
            focus,
            current_passphrase: StringState::invisible(),
            new_passphrase: StringState::invisible(),
            confirm_passphrase: StringState::invisible(),
            danger_confirm: YesNoState { selected: false },
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StartupUpdatePhase {
    Downloading,
    Verifying,
    Installing,
    Done,
    Failed,
}

#[derive(Debug)]
pub(crate) struct StartupUpdateState {
    pub(crate) phase: StartupUpdatePhase,
    pub(crate) message: Option<String>,
    pub(crate) spinner_frame: usize,
    pub(crate) downloaded: u64,
    pub(crate) total: Option<u64>,
    pub(crate) install_receiver: Option<std::sync::mpsc::Receiver<backend::UpdateInstallStatus>>,
}

impl DashboardState {
    pub(crate) fn new() -> Self {
        Self {
            active_page: DashboardPage::Home,
            selected_host: 0,
            host_modal: None,
            endpoint_picker: None,
            quick_switcher: None,
            last_status: None,
            host_statuses: HashMap::new(),
            probe_tasks: Vec::new(),
            last_probe_at: Instant::now(),
            needs_initial_probe: true,
            ssh_tabs: Vec::new(),
            active_ssh_tab: None,
            settings_selected_row: 0,
            settings_modal: None,
            settings_backup_modal: None,
            update_prompt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointPickerState {
    pub(crate) host_id: u32,
    pub(crate) host_name: String,
    pub(crate) host_user: String,
    pub(crate) endpoints: Vec<backend::SshEndpoint>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QuickSwitcherState {
    pub(crate) query: String,
    pub(crate) selected_idx: usize,
    pub(crate) ctrl_cycle_on_release: bool,
}

impl QuickSwitcherState {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            selected_idx: 0,
            ctrl_cycle_on_release: false,
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
            auth_mode: HostAuthMode::Password,
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
pub(crate) struct HostKeyPickerEntry {
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HostKeyPickerState {
    pub(crate) target_mode: HostKeyInputMode,
    pub(crate) current_dir: String,
    pub(crate) entries: Vec<HostKeyPickerEntry>,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) command_input: String,
    pub(crate) completion_prefix: String,
    pub(crate) completion_matches: Vec<String>,
    pub(crate) completion_index: usize,
    pub(crate) command_history: Vec<String>,
    pub(crate) history_index: Option<usize>,
    pub(crate) status: Option<String>,
    pub(crate) error: Option<String>,
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
