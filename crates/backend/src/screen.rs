#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    LoadingLogo,
    OnboardingWantsEncryption,
    OnboardingWantsPassphrase,
    AskingPassphrase,
    Dashboard,
}
