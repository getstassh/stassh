use backend::AppState;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::{
    inputs::{handle_pasted_text, handle_text_input},
    navigation::{OnboardingPassphraseField, OnboardingPassphraseState, Screen, StringState},
    screens::{
        AppEffect, ScreenHandler,
        components::{LogoType, page_with_logo, paragraph_with_note},
    },
    ui::{accent_text, centered_rect, danger_text, line_with_caret, muted_text, text},
};

pub(crate) static HANDLER: ScreenHandler<OnboardingPassphraseState> = ScreenHandler {
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

fn active_input_mut(state: &mut OnboardingPassphraseState) -> &mut StringState {
    match state.focus {
        OnboardingPassphraseField::Passphrase => &mut state.passphrase,
        OnboardingPassphraseField::Confirm => &mut state.confirm_passphrase,
    }
}

fn submit(state: &OnboardingPassphraseState) -> Option<AppEffect> {
    let passphrase = state.passphrase.text.clone();
    let confirm = state.confirm_passphrase.text.clone();

    Some(Box::new(move |app| {
        if passphrase.trim().is_empty() {
            let mut next = OnboardingPassphraseState::new();
            next.passphrase = StringState::invisible();
            next.confirm_passphrase = StringState::invisible();
            next.error = Some("Passphrase cannot be empty".to_string());
            app.screen = Screen::OnboardingWantsPassphrase { state: next };
            return;
        }

        if confirm.is_empty() {
            let mut next = OnboardingPassphraseState::new();
            next.passphrase.text = passphrase.clone();
            next.passphrase.caret_position = next.passphrase.text.len();
            next.focus = OnboardingPassphraseField::Confirm;
            next.error = Some("Please confirm your passphrase".to_string());
            app.screen = Screen::OnboardingWantsPassphrase { state: next };
            return;
        }

        if passphrase != confirm {
            let mut next = OnboardingPassphraseState::new();
            next.passphrase.text = passphrase.clone();
            next.passphrase.caret_position = next.passphrase.text.len();
            next.confirm_passphrase.text = confirm.clone();
            next.confirm_passphrase.caret_position = next.confirm_passphrase.text.len();
            next.focus = OnboardingPassphraseField::Confirm;
            next.error = Some("Passphrases do not match".to_string());
            app.screen = Screen::OnboardingWantsPassphrase { state: next };
            return;
        }

        app.password = Some(passphrase);
        let result = app.load_db();
        if let Err(e) = result {
            app.password = None;
            let mut next = OnboardingPassphraseState::new();
            next.error = Some(format!("Failed to initialize encrypted database: {e}"));
            app.screen = Screen::OnboardingWantsPassphrase { state: next };
            return;
        }

        app.config.db_encryption = Some(backend::DbEncryption::Passphrase);
        let _ = app.save_config();

        app.go_to_dashboard();
    }))
}

fn handle_key(
    _: &AppState,
    key: KeyEvent,
    state: &mut OnboardingPassphraseState,
) -> Option<AppEffect> {
    match key.code {
        KeyCode::Tab | KeyCode::Down => {
            state.focus = state.focus.next();
            state.error = None;
            return None;
        }
        KeyCode::BackTab | KeyCode::Up => {
            state.focus = state.focus.prev();
            state.error = None;
            return None;
        }
        _ => {}
    }

    let entered = {
        let input = active_input_mut(state);
        handle_text_input(input, key)
            .map(ToString::to_string)
            .is_some()
    };

    if entered {
        state.error = None;
        if state.focus == OnboardingPassphraseField::Passphrase {
            state.focus = OnboardingPassphraseField::Confirm;
            return None;
        }
        return submit(state);
    }

    if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) {
        state.error = None;
    }

    None
}

fn handle_paste(
    _: &AppState,
    text: &str,
    state: &mut OnboardingPassphraseState,
) -> Option<AppEffect> {
    handle_pasted_text(active_input_mut(state), text);
    state.error = None;
    None
}

fn ui(frame: &mut Frame, _app: &AppState, state: &OnboardingPassphraseState) {
    let area = page_with_logo(
        frame,
        frame.area(),
        LogoType::Simple,
        "Welcome to stassh!",
        "Tab/Up/Down switch | Enter next/confirm | Type passphrase",
    );

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    if let Some(error) = &state.error {
        frame.render_widget(
            Paragraph::new(error.clone())
                .alignment(Alignment::Center)
                .style(danger_text()),
            split[0],
        );
    }

    paragraph_with_note(
        frame,
        split[1],
        "Use a strong phrase you can remember.",
        "You will need it on every launch.",
    );

    render_passphrase_input(
        frame,
        split[2],
        "Passphrase",
        &state.passphrase,
        state.focus == OnboardingPassphraseField::Passphrase,
    );
    render_passphrase_input(
        frame,
        split[3],
        "Confirm passphrase",
        &state.confirm_passphrase,
        state.focus == OnboardingPassphraseField::Confirm,
    );
}

fn render_passphrase_input(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    label: &str,
    value: &StringState,
    selected: bool,
) {
    let (text_box, text_box_area, text_area) = centered_rect(56, 3, area);
    let title = if selected {
        format!(" {label} ")
    } else {
        format!(" {label} ")
    };
    let text_box = text_box
        .title(if selected {
            ratatui::text::Span::styled(title, accent_text())
        } else {
            ratatui::text::Span::styled(title, muted_text())
        })
        .border_style(if selected {
            ratatui::style::Style::default().fg(ratatui::style::Color::Cyan)
        } else {
            ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)
        });

    frame.render_widget(text_box, text_box_area);

    let line = if selected {
        line_with_caret(value)
    } else {
        ratatui::text::Line::from(value.visible_text())
    };
    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Left)
            .style(if selected { accent_text() } else { text() }),
        text_area,
    );
}
