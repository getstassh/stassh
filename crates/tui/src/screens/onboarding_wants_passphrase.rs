use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::{handle_pasted_text, handle_text_input},
    navigation::{Screen, StringState},
    screens::{AppEffect, ScreenHandler},
    ui::{centered_rect, dual_vertical_rect, full_rect, line_with_caret},
};

pub(crate) static HANDLER: ScreenHandler<StringState> = ScreenHandler {
    matches: |s| matches!(s, Screen::OnboardingWantsPassphrase { .. }),
    get: |s| match s {
        Screen::OnboardingWantsPassphrase { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::OnboardingWantsPassphrase { state } => Some(state),
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
            app.config.db_encryption = Some(backend::DbEncryption::Passphrase);
            let _ = app.save_config();
            app.password = Some(text);
            let result = app.load_db();
            if let Err(e) = result {
                panic!("Failed to load database with provided passphrase: {e}");
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

    let (inner, area) = full_rect(
        a,
        "Welcome to stassh!",
        "←/→ or Tab switch | Enter confirm | Type passphrase",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Enter your passphrase:").alignment(Alignment::Center);
    let (top, bottom) = dual_vertical_rect(area);
    frame.render_widget(question, top);
    let (text_box, text_box_area, text_area) = centered_rect(50, 3, bottom);
    frame.render_widget(text_box, text_box_area);
    let passphrase = Paragraph::new(line_with_caret(state)).alignment(Alignment::Left);
    frame.render_widget(passphrase, text_area);
}
