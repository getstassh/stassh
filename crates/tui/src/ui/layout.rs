use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
};

use crate::ui::{
    accent_text, border, muted_text, panel_alt_background, selected_border, soft_accent_text,
    theme::{TEXT, app_background, panel_background},
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
    let block = input_block(false);
    let outer = horizontal[1];
    let inner = block.inner(outer);
    (block, outer, inner)
}

pub(crate) fn centered_rect_no_border(width: u16, height: u16, area: Rect) -> Rect {
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

    horizontal[1]
}

pub(crate) fn full_rect<'a>(
    area: Rect,
    title_top: &'a str,
    title_bottom: &'a str,
) -> (Block<'a>, Rect) {
    let block = shell_block(title_top, title_bottom);

    let inner = block.inner(area);

    (block, inner)
}

pub(crate) fn shell_block<'a>(title_top: &'a str, title_bottom: &'a str) -> Block<'a> {
    Block::default()
        .title(Line::from(vec![
            Span::styled(" ", muted_text()),
            Span::styled(title_top.trim(), accent_text()),
            Span::styled(" ", muted_text()),
        ]))
        .title_bottom(Line::from(vec![
            Span::styled(" ", muted_text()),
            Span::styled(title_bottom.trim(), soft_accent_text()),
            Span::styled(" ", muted_text()),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(border())
        .style(app_background().fg(TEXT))
}

pub(crate) fn frame_block<'a>() -> Block<'a> {
    Block::default().style(panel_background())
}

pub(crate) fn input_block<'a>(selected: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if selected {
            selected_border()
        } else {
            border()
        })
        .style(panel_alt_background())
}

pub(crate) fn modal_block<'a>(title: &'a str, footer: &'a str) -> Block<'a> {
    Block::default()
        .title(Line::from(Span::styled(
            format!(" {title} "),
            accent_text(),
        )))
        .title_bottom(Line::from(Span::styled(
            format!(" {footer} "),
            muted_text(),
        )))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(selected_border())
        .style(panel_background().fg(TEXT))
}
