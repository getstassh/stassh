use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::Paragraph,
};

use crate::ui::{muted_text, text};

pub(crate) fn paragraph_with_note(frame: &mut Frame, area: Rect, title: &str, note: &str) {
    let question = Paragraph::new(title)
        .alignment(Alignment::Center)
        .style(text());

    let note = Paragraph::new(note)
        .alignment(Alignment::Center)
        .style(muted_text());

    let split = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Length(1),
        ])
        .split(area);
    frame.render_widget(question, split[0]);
    frame.render_widget(note, split[1]);
}
