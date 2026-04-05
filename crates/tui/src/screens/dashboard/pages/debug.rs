use backend::AppState;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Rect},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::navigation::DashboardState;

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
        "Debug\n\nConfig object:\n{:#?}\n\nDB object:\n{:#?}",
        app.config, app.db,
    );
    let text = Paragraph::new(debug_text.clone())
        .alignment(Alignment::Left)
        .scroll((state.debug_scroll, 0));

    let viewport = area.height.saturating_sub(1) as u16;
    let content_lines = debug_text.lines().count() as u16;
    let max_scroll = content_lines.saturating_sub(viewport);
    let scroll = state.debug_scroll.min(max_scroll);
    let scrollbar = Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight);
    let mut scrollbar_state = ScrollbarState::new(max_scroll as usize).position(scroll as usize);

    frame.render_widget(text.scroll((scroll, 0)), area);
    frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

pub(crate) fn footer_hint() -> &'static str {
    "j/k or Up/Down scroll | PageUp/PageDown jump | Ctrl+Q quick switch | Esc exit"
}
