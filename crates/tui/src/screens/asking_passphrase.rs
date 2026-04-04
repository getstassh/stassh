use backend::AppState;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::{
    inputs::handle_text_input,
    navigation::{Screen, StringState},
    screens::{AppEffect, ScreenHandler, components::render_logo_with_credits},
    ui::{centered_rect, full_rect, line_with_caret},
};

pub(crate) fn asking_passphrase_handler() -> ScreenHandler<StringState> {
    ScreenHandler {
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
        handle_tick: |_app, _| None,
    }
}

fn handle_key(_: &AppState, key_code: KeyCode, state: &mut StringState) -> Option<AppEffect> {
    let text = handle_text_input(state, key_code);
    if let Some(text) = text {
        let text = text.to_string();
        return Some(Box::new(move |app| {
            app.password = Some(text);
            let result = app.load_db();
            if let Err(e) = result {
                panic!("Failed to load database with provided passphrase: {e}");
            }

            app.screen = Screen::Dashboard;
        }));
    }
    None
}

fn ui(frame: &mut Frame, _app: &AppState, state: &StringState) {
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Enter Passphrase",
        "Type your passphrase and press Enter",
    );

    frame.render_widget(inner, a);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    render_logo_with_credits(frame, layout[0]);

    let question = Paragraph::new("Enter your passphrase:").alignment(Alignment::Center);
    frame.render_widget(question, layout[2]);
    let (text_box, text_box_area, text_area) = centered_rect(50, 3, layout[3]);
    frame.render_widget(text_box, text_box_area);
    let passphrase = Paragraph::new(line_with_caret(state)).alignment(Alignment::Left);
    frame.render_widget(passphrase, text_area);
}
