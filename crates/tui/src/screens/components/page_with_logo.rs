use ratatui::{Frame, layout::Rect};

use crate::{
    screens::components::{LogoType, render_logo},
    ui::full_rect,
};

pub(crate) fn page_with_logo(
    frame: &mut Frame,
    a: Rect,
    logo_type: LogoType,
    title: &str,
    help_text: &str,
) -> Rect {
    let (inner, area) = full_rect(a, title, help_text);
    frame.render_widget(inner, a);

    let split = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(0),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(area);

    render_logo(frame, split[0], logo_type);

    return split[1];
}
