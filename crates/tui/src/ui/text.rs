use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::navigation::StringState;

pub fn line_with_caret(state: &StringState) -> Line<'static> {
    let text = state.visible_text();
    let caret = state.caret_position;

    let before = text[..caret].to_string();
    let current = text[caret..].chars().next().unwrap_or(' ');
    let after = if caret < text.len() {
        text[caret + current.len_utf8()..].to_string()
    } else {
        String::new()
    };

    Line::from(vec![
        Span::raw(before),
        Span::styled(
            current.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ),
        Span::raw(after),
    ])
}
