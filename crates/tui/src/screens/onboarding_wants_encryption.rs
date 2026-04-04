use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::handle_yes_no_input,
    navigation::{Screen, StringState, YesNoState},
    screens::{AppEffect, ScreenHandler},
    ui::{button, dual_vertical_rect, full_rect},
};

pub(crate) static HANDLER: ScreenHandler<YesNoState> = ScreenHandler {
    matches: |s| matches!(s, Screen::OnboardingWantsEncryption { .. }),
    get: |s| match s {
        Screen::OnboardingWantsEncryption { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::OnboardingWantsEncryption { state } => Some(state),
        _ => None,
    },
    render: ui,
    handle_key: handle_key,
    handle_paste: |_, _, _| None,
    handle_resize: |_, _, _, _| None,
    handle_tick: |_app, _| None,
};

fn handle_key(_: &AppState, key: KeyEvent, state: &mut YesNoState) -> Option<AppEffect> {
    let result = handle_yes_no_input(state, key.code);
    if let Some(result) = result {
        return Some(Box::new(move |app| {
            if result {
                app.screen = Screen::OnboardingWantsPassphrase {
                    state: StringState::invisible(),
                };
            } else {
                app.config.db_encryption = Some(backend::DbEncryption::None);
                let _ = app.save_config();
                let _ = app.load_db();
                app.go_to_dashboard();
            }
        }));
    }

    None
}

fn ui(frame: &mut Frame, _app: &AppState, state: &YesNoState) {
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
