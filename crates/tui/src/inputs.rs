use crossterm::event::KeyCode;

use crate::navigation::{StringState, YesNoState};

pub(crate) fn handle_yes_no_input(state: &mut YesNoState, key_code: KeyCode) -> Option<bool> {
    if key_code == KeyCode::Left || key_code == KeyCode::Right || key_code == KeyCode::Tab {
        state.toggle();
        None
    } else if key_code == KeyCode::Char('y') || (key_code == KeyCode::Enter && state.is_yes()) {
        Some(true)
    } else if key_code == KeyCode::Char('n') || (key_code == KeyCode::Enter && state.is_no()) {
        Some(false)
    } else {
        None
    }
}

pub(crate) fn handle_text_input(state: &mut StringState, key_code: KeyCode) -> Option<&str> {
    match key_code {
        KeyCode::Char(c) => {
            let mut text = state.text.clone();
            text.insert(state.caret_position, c);
            state.set_text(text);
            state.caret_position += 1;
            None
        }
        KeyCode::Backspace => {
            let mut text = state.text.clone();
            if state.caret_position > 0 {
                text.remove(state.caret_position - 1);
                state.set_text(text);
                state.caret_position -= 1;
            }
            None
        }
        KeyCode::Enter => Some(state.text.as_str()),
        KeyCode::Left => {
            if state.caret_position > 0 {
                state.caret_position -= 1;
            }
            None
        }
        KeyCode::Right => {
            if state.caret_position < state.text.len() {
                state.caret_position += 1;
            }
            None
        }
        _ => None,
    }
}
