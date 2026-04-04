use backend::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::{
    screens::{AppEffect, ScreenHandler},
    ui::full_rect,
};

pub fn dashboard_handler() -> ScreenHandler<backend::Screen> {
    ScreenHandler {
        matches: backend::Screen::is_dashboard,
        get: |s| match s {
            backend::Screen::Dashboard => Some(s),
            _ => None,
        },
        get_mut: |s| match s {
            backend::Screen::Dashboard => Some(s),
            _ => None,
        },
        render: ui,
        handle_key: |app, key_code, _| None,
        handle_tick: handle_tick,
    }
}

fn handle_tick(app: &AppState, _state: &mut backend::Screen) -> Option<AppEffect> {
    return Some(Box::new(move |app| {
        app.state.db.index += 1;
        let _ = app.save_db();
    }));
}

fn ui(frame: &mut Frame, app: &AppState, _kind: &backend::Screen) {
    let a = frame.area();

    let (inner, area) = full_rect(a, "Stassh", "Use ←/→ or Tab to switch");

    frame.render_widget(inner, a);

    let welcome = Paragraph::new(format!("Welcome to stassh!",)).alignment(Alignment::Center);

    let config = Paragraph::new(format!("Config: {:?}", app.config)).alignment(Alignment::Left);
    let database = Paragraph::new(format!("Database: {:?}", app.db)).alignment(Alignment::Left);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(welcome, layout[0]);
    frame.render_widget(config, layout[1]);
    frame.render_widget(database, layout[2]);
}
