#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Screen {
    OnboardingWantsEncryption { state: YesNoState },
    OnboardingWantsPassphrase { state: StringState },
    OnboardingWantsTelemetry { state: YesNoState },
    AskingPassphrase { state: StringState },
    Dashboard { state: () },
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
