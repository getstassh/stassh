use backend::AppState;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::navigation::DashboardState;
use crate::ui::{soft_accent_text, text};

pub(crate) fn handle_key(
    key: KeyEvent,
    state: &mut DashboardState,
) -> Option<crate::screens::AppEffect> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.debug_scroll = state.debug_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.debug_scroll = state.debug_scroll.saturating_add(1);
        }
        KeyCode::PageUp => {
            state.debug_scroll = state.debug_scroll.saturating_sub(8);
        }
        KeyCode::PageDown => {
            state.debug_scroll = state.debug_scroll.saturating_add(8);
        }
        _ => {}
    }

    None
}

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    let debug_text = format!(
        "DEBUG PANEL\n\nConfig object:\n{:#?}\n\nDB object:\n{:#?}",
        app.config, app.db,
    );

    let content = Paragraph::new(debug_text.clone())
        .alignment(Alignment::Left)
        .style(text())
        .scroll((state.debug_scroll, 0));

    let content_lines = debug_text.lines().count() as u16;
    let max_scroll = max_scroll_for_lines(content_lines, area);
    let has_scrollbar = max_scroll > 0;

    let content_viewport = area.height.saturating_sub(1);
    let content_max_scroll = content_lines.saturating_sub(content_viewport);
    let content_scroll = state.debug_scroll.min(content_max_scroll);

    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .thumb_style(soft_accent_text())
        .track_style(Style::default());
    let mut scrollbar_state =
        ScrollbarState::new(content_max_scroll as usize).position(content_scroll as usize);

    frame.render_widget(content.scroll((content_scroll, 0)), area);
    if has_scrollbar {
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

pub(crate) fn has_scrollbar(app: &AppState, area: Rect) -> bool {
    let debug_text = format!(
        "DEBUG PANEL\n\nConfig object:\n{:#?}\n\nDB object:\n{:#?}",
        app.config, app.db,
    );
    let content_lines = debug_text.lines().count() as u16;
    max_scroll_for_lines(content_lines, area) > 0
}

fn max_scroll_for_lines(content_lines: u16, area: Rect) -> u16 {
    let viewport = area.height.saturating_sub(1) as u16;
    content_lines.saturating_sub(viewport)
}

pub(crate) fn footer_hint(has_scrollbar: bool) -> &'static str {
    if has_scrollbar {
        "j/k or Up/Down to scroll | Ctrl+Q quick switch | Esc exit"
    } else {
        "Ctrl+Q quick switch | Esc exit"
    }
}
