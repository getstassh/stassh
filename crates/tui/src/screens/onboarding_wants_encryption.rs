use backend::AppState;
use crossterm::event::KeyCode;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::handle_yes_no_input,
    screens::{AppEffect, ScreenHandler},
    ui::{button, dual_vertical_rect, full_rect},
};

pub fn onboarding_wants_encryption_handler() -> ScreenHandler<backend::YesNoState> {
    ScreenHandler {
        matches: backend::Screen::is_onboarding_wants_encryption,
        get: |s| match s {
            backend::Screen::OnboardingWantsEncryption { state } => Some(state),
            _ => None,
        },
        get_mut: |s| match s {
            backend::Screen::OnboardingWantsEncryption { state } => Some(state),
            _ => None,
        },
        render: ui,
        handle_key: handle_key,
        handle_tick: |app, _| None,
    }
}

fn handle_key(
    _: &AppState,
    key_code: KeyCode,
    state: &mut backend::YesNoState,
) -> Option<AppEffect> {
    let result = handle_yes_no_input(state, key_code);
    if let Some(result) = result {
        return Some(Box::new(move |app| {
            if result {
                app.screen = backend::Screen::OnboardingWantsPassphrase {
                    state: backend::StringState::invisible(),
                };
            } else {
                app.state.config.db_encryption = Some(backend::DbEncryption::None);
                app.save_config();
                app.load_db();
                app.screen = backend::Screen::Dashboard;
            }
        }));
    }

    None
}

fn ui(frame: &mut Frame, _app: &AppState, state: &backend::YesNoState) {
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Welcome to stassh!",
        "Use ←/→ or Tab to switch, Enter to confirm, Y/N for quick answers",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Do you want to enable encryption?").alignment(Alignment::Center);

    let buttons = Paragraph::new(format!(
        "{} {}",
        button("Yes", state.is_yes()),
        button("No", state.is_no()),
    ))
    .alignment(Alignment::Center);

    let (top, bottom) = dual_vertical_rect(area);
    frame.render_widget(question, top);
    frame.render_widget(buttons, bottom);
}
