#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    OnboardingWantsEncryption { state: YesNoState },
    OnboardingWantsPassphrase { state: StringState },
    AskingPassphrase { state: StringState },
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
    pub caret_position: usize,
}

impl StringState {
    pub fn visible() -> Self {
        Self {
            is_visible: true,
            text: String::new(),
            caret_position: 0,
        }
    }
    pub fn invisible() -> Self {
        Self {
            is_visible: false,
            text: String::new(),
            caret_position: 0,
        }
    }
    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }
    pub fn toggle_visibility(&mut self) {
        self.is_visible = !self.is_visible;
    }
    pub fn visible_text(self) -> String {
        let text = if self.is_visible {
            self.text.clone()
        } else {
            "*".repeat(self.text.len())
        };

        format!("{} ", text)
    }
}
