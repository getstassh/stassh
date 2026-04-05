use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::Paragraph,
};

use crate::ui::text;

pub(crate) fn render(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(
            "CREDITS\n\nBuilt by Lazar\nTerminal UI: ratatui + crossterm\nSSH: russh + vt100\n\nThanks for using stassh.",
        )
        .alignment(Alignment::Left)
        .style(text()),
        area,
    );
}
