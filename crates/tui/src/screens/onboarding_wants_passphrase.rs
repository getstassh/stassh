use backend::AppState;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    inputs::{handle_pasted_text, handle_text_input},
    navigation::{Screen, StringState},
    screens::{
        AppEffect, ScreenHandler,
        components::{LogoType, page_with_logo, paragraph_with_note},
    },
    ui::{accent_text, centered_rect, line_with_caret},
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
            if text.trim().is_empty() {
                app.screen = Screen::OnboardingWantsPassphrase {
                    state: StringState::invisible_with_error(
                        "Passphrase cannot be empty".to_string(),
                    ),
                };
                return;
            }

            app.password = Some(text);
            let result = app.load_db();
            if let Err(e) = result {
                app.password = None;
                app.screen = Screen::OnboardingWantsPassphrase {
                    state: StringState::invisible_with_error(format!(
                        "Failed to initialize encrypted database: {e}"
                    )),
                };
                return;
            }

            app.config.db_encryption = Some(backend::DbEncryption::Passphrase);
            let _ = app.save_config();

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
    let area = page_with_logo(
        frame,
        frame.area(),
        LogoType::Simple,
        "Welcome to stassh!",
        "←/→ or Tab switch | Enter confirm | Type passphrase",
    );

    let split = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(2),
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(area);

    paragraph_with_note(
        frame,
        split[0],
        "Use a strong phrase you can remember.",
        "Make sure to remember it!",
    );

    let (text_box, text_box_area, text_area) = centered_rect(56, 3, split[1]);
    frame.render_widget(text_box, text_box_area);
    let passphrase = Paragraph::new(line_with_caret(state))
        .alignment(Alignment::Left)
        .style(accent_text());
    frame.render_widget(passphrase, text_area);
}
