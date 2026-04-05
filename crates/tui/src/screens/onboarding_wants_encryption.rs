use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::handle_yes_no_input,
    navigation::{Screen, StringState, YesNoState},
    screens::{
        AppEffect, ScreenHandler,
        components::{LogoType, render_logo},
    },
    ui::{accent_text, button, full_rect, muted_text, text},
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
        "←/→ or Tab switch | Enter confirm | Y/N quick answer",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Protect your host data with local passphrase encryption?")
        .alignment(Alignment::Center)
        .style(text());

    let buttons = Paragraph::new(format!(
        "{} {}",
        button("Yes", state.is_yes()),
        button("No", state.is_no()),
    ))
    .alignment(Alignment::Center)
    .style(accent_text());

    let note = Paragraph::new("Encryption can be enabled now and used every launch.")
        .alignment(Alignment::Center)
        .style(muted_text());

    let split = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(0),
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(area);
    render_logo(frame, split[0], LogoType::Simple);
    frame.render_widget(question, split[1]);
    frame.render_widget(note, split[2]);
    frame.render_widget(buttons, split[3]);
}
