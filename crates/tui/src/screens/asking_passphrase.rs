use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::{handle_pasted_text, handle_text_input},
    navigation::{Screen, StringState},
    screens::{
        AppEffect, ScreenHandler,
        components::{LogoType, render_logo},
    },
    ui::{accent_text, centered_rect, danger_text, full_rect, line_with_caret, text},
};

pub(crate) static HANDLER: ScreenHandler<StringState> = ScreenHandler {
    matches: |s| matches!(s, Screen::AskingPassphrase { .. }),
    get: |s| match s {
        Screen::AskingPassphrase { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::AskingPassphrase { state } => Some(state),
        _ => None,
    },
    render: ui,
    handle_key: handle_key,
    handle_paste: handle_paste,
    handle_resize: |_, _, _, _| None,
    handle_tick: |_app, _| None,
};

fn handle_key(_: &AppState, key: KeyEvent, state: &mut StringState) -> Option<AppEffect> {
    let text = handle_text_input(state, key);
    if let Some(text) = text {
        let text = text.to_string();
        return Some(Box::new(move |app| {
            if !app.is_correct_password(&text) {
                app.screen = Screen::AskingPassphrase {
                    state: StringState::invisible_with_error("Incorrect passphrase".to_string()),
                };
                return;
            }
            app.password = Some(text);
            let result = app.load_db();
            if let Err(e) = result {
                app.screen = Screen::OnboardingWantsPassphrase {
                    state: StringState::invisible_with_error(format!(
                        "Failed to load database with provided passphrase: {e}"
                    )),
                };
                return;
            }

            app.go_to_dashboard();
        }));
    }
    None
}

fn handle_paste(_: &AppState, text: &str, state: &mut StringState) -> Option<AppEffect> {
    handle_pasted_text(state, text);
    None
}

fn ui(frame: &mut Frame, _app: &AppState, state: &StringState) {
    let a = frame.area();

    let (inner, area) = full_rect(a, "Stassh", "Type passphrase | Enter confirm");

    frame.render_widget(inner, a);

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
    render_logo(frame, split[0], LogoType::WithCredits);

    if let Some(error) = &state.error {
        let error = Paragraph::new(error.clone())
            .alignment(Alignment::Center)
            .style(danger_text());
        frame.render_widget(error, split[1]);
    }

    let question = Paragraph::new("Enter your passphrase:")
        .alignment(Alignment::Center)
        .style(text());
    frame.render_widget(question, split[2]);
    let (text_box, text_box_area, text_area) = centered_rect(56, 3, split[3]);
    frame.render_widget(text_box, text_box_area);
    let passphrase = Paragraph::new(line_with_caret(state))
        .alignment(Alignment::Left)
        .style(accent_text());
    frame.render_widget(passphrase, text_area);
}
