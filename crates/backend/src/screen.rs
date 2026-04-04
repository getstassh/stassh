#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    LoadingLogo,
    OnboardingWantsEncryption { state: YesNoState },
    OnboardingWantsPassphrase { passphrase: StringState },
    AskingPassphrase { passphrase: StringState },
    Dashboard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct YesNoState {
    pub selected: bool,
}

impl YesNoState {
    pub fn new() -> Self {
        Self { selected: true }
    }
    pub fn toggle(&mut self) {
        self.selected = !self.selected;
    }
    pub fn set(&mut self, value: bool) {
        self.selected = value;
    }
    pub fn is_yes(&self) -> bool {
        self.selected
    }
    pub fn is_no(&self) -> bool {
        !self.selected
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringState {
    pub is_visible: bool,
    pub text: String,
}

impl StringState {
    pub fn new() -> Self {
        Self {
            is_visible: true,
            text: String::new(),
        }
    }
    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }
    pub fn toggle_visibility(&mut self) {
        self.is_visible = !self.is_visible;
    }
    pub fn visible_text(&self) -> String {
        if self.is_visible {
            self.text.clone()
        } else {
            "*".repeat(self.text.len())
        }
    }
}
