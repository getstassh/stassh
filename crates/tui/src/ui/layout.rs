use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

pub(crate) fn centered_rect<'a>(width: u16, height: u16, area: Rect) -> (Block<'a>, Rect, Rect) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width),
            Constraint::Fill(1),
        ])
        .split(vertical[1]);
    let block = Block::default().borders(Borders::ALL);
    let outer = horizontal[1];
    let inner = block.inner(outer);
    (block, outer, inner)
}

pub(crate) fn full_rect<'a>(
    area: Rect,
    title_top: &'a str,
    title_bottom: &'a str,
) -> (Block<'a>, Rect) {
    let block = Block::default()
        .title(format!(" {} ", title_top.trim()))
        .title_bottom(format!(" {} ", title_bottom.trim()))
        .borders(Borders::ALL);

    let inner = block.inner(area);

    (block, inner)
}

pub(crate) fn dual_vertical_rect(area: Rect) -> (Rect, Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    (layout[0], layout[1])
}
