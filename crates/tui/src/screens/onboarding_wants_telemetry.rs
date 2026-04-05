use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::handle_yes_no_input,
    navigation::{Screen, YesNoState},
    screens::{
        AppEffect, ScreenHandler,
        components::{LogoType, render_logo},
    },
    ui::{accent_text, button, full_rect, muted_text, text},
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
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Welcome to stassh!",
        "←/→ or Tab switch | Enter confirm | Y/N quick answer",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Share anonymous hosts count for our website? Just an integer.")
        .alignment(Alignment::Center)
        .style(text());

    let buttons = Paragraph::new(format!(
        "{} {}",
        button("Yes", state.is_yes()),
        button("No", state.is_no()),
    ))
    .alignment(Alignment::Center)
    .style(accent_text());

    let note = Paragraph::new("No hostnames, usernames, or credentials are included.")
        .alignment(Alignment::Center)
        .style(muted_text());

    let split = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(0),
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(area);

    render_logo(frame, split[0], LogoType::Simple);

    frame.render_widget(question, split[1]);
    frame.render_widget(note, split[2]);
    frame.render_widget(buttons, split[3]);
}
