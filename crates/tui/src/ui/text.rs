use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub fn line_with_caret(text: &str, caret: usize, caret_on: bool) -> Line<'static> {
    let before = text[..caret].to_string();
    let current = text[caret..].chars().next().unwrap_or(' ');
    let after = if caret < text.len() {
        text[caret + current.len_utf8()..].to_string()
    } else {
        String::new()
    };
    if !caret_on {
        return Line::from(text.to_string());
    }
    Line::from(vec![
        Span::raw(before),
        Span::styled(
            current.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ),
        Span::raw(after),
    ])
}
