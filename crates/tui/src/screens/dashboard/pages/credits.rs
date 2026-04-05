use ratatui::{
    layout::{Alignment, Rect},
    widgets::Paragraph,
    Frame,
};

pub(crate) fn render(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(
            "Credits\n\nBuilt by Lazar\nTerminal UI: ratatui + crossterm\nThanks for using stassh.",
        )
        .alignment(Alignment::Left),
        area,
    );
}
