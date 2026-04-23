use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::handle_yes_no_input,
    navigation::{Screen, YesNoState},
    screens::{
        AppEffect, ScreenHandler,
        components::{LogoType, page_with_logo, paragraph_with_note},
    },
    ui::{accent_text, button},
};

pub(crate) static HANDLER: ScreenHandler<YesNoState> = ScreenHandler {
    matches: |s| matches!(s, Screen::OnboardingWantsTelemetry { .. }),
    get: |s| match s {
        Screen::OnboardingWantsTelemetry { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::OnboardingWantsTelemetry { state } => Some(state),
        _ => None,
    },
    render: ui,
    handle_key: handle_key,
    handle_mouse: |_, _, _| None,
    handle_paste: |_, _, _| None,
    handle_resize: |_, _, _, _| None,
    handle_tick: |_app, _| None,
};

fn handle_key(_: &AppState, key: KeyEvent, state: &mut YesNoState) -> Option<AppEffect> {
    let result = handle_yes_no_input(state, key.code);
    if let Some(result) = result {
        return Some(Box::new(move |app| {
            app.config.enable_telemetry = Some(result);
            let _ = app.save_config();
            app.go_to_dashboard();
        }));
    }

    None
}

fn ui(frame: &mut Frame, _app: &AppState, state: &YesNoState) {
    let area = page_with_logo(
        frame,
        frame.area(),
        LogoType::Simple,
        "Welcome to stassh!",
        "←/→ or Tab switch | Enter confirm | Y/N quick answer",
    );

    let split = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(2),
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(area);

    paragraph_with_note(
        frame,
        split[0],
        "Share anonymous hosts count for our website? Just an integer.",
        "No hostnames, usernames, or credentials are included.",
    );

    let buttons = Paragraph::new(format!(
        "{} {}",
        button("Yes", state.is_yes()),
        button("No", state.is_no()),
    ))
    .alignment(Alignment::Center)
    .style(accent_text());
    frame.render_widget(buttons, split[1]);
}
