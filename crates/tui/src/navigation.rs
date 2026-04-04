use std::time::Instant;

use crate::ssh_client::{LiveSshSession, PendingSshStart, TrustChallenge};

pub(crate) enum Screen {
    OnboardingWantsEncryption { state: YesNoState },
    OnboardingWantsPassphrase { state: StringState },
    OnboardingWantsTelemetry { state: YesNoState },
    AskingPassphrase { state: StringState },
    Dashboard { state: DashboardState },
    SshSession { state: SshSessionState },
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
    Error(String),
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
    Credits,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DashboardState {
    pub(crate) active_page: DashboardPage,
    pub(crate) selected_host: usize,
    pub(crate) host_modal: Option<HostModalState>,
    pub(crate) last_status: Option<String>,
}

impl DashboardState {
    pub(crate) fn new() -> Self {
        Self {
            active_page: DashboardPage::Home,
            selected_host: 0,
            host_modal: None,
            last_status: None,
        }
    }
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
    Host,
    User,
    Port,
    AuthMode,
    AuthValue,
}

impl HostFormField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Name => Self::Host,
            Self::Host => Self::User,
            Self::User => Self::Port,
            Self::Port => Self::AuthMode,
            Self::AuthMode => Self::AuthValue,
            Self::AuthValue => Self::Name,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Name => Self::AuthValue,
            Self::Host => Self::Name,
            Self::User => Self::Host,
            Self::Port => Self::User,
            Self::AuthMode => Self::Port,
            Self::AuthValue => Self::AuthMode,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HostFormState {
    pub(crate) focus: HostFormField,
    pub(crate) name: String,
    pub(crate) host: String,
    pub(crate) user: String,
    pub(crate) port: String,
    pub(crate) auth_mode: HostAuthMode,
    pub(crate) key_path: String,
    pub(crate) password: String,
    pub(crate) error: Option<String>,
}

impl HostFormState {
    pub(crate) fn new() -> Self {
        Self {
            focus: HostFormField::Name,
            name: String::new(),
            host: String::new(),
            user: String::new(),
            port: String::from("22"),
            auth_mode: HostAuthMode::Key,
            key_path: String::new(),
            password: String::new(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HostModalState {
    pub(crate) mode: HostModalMode,
    pub(crate) form: HostFormState,
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
