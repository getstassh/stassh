use backend::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::{
    navigation::Screen,
    screens::{AppEffect, ScreenHandler},
    ui::full_rect,
};

pub fn dashboard_handler() -> ScreenHandler<Screen> {
    ScreenHandler {
        matches: |s| matches!(s, Screen::Dashboard),
        get: |s| match s {
            Screen::Dashboard => Some(s),
            _ => None,
        },
        get_mut: |s| match s {
            Screen::Dashboard => Some(s),
            _ => None,
        },
        render: ui,
        handle_key: |_app, _key_code, _| None,
        handle_tick: handle_tick,
    }
}

fn handle_tick(_app: &AppState, _state: &mut Screen) -> Option<AppEffect> {
    return Some(Box::new(move |app| {
        app.db.index += 1;
        let _ = app.save_db();
    }));
}

fn ui(frame: &mut Frame, app: &AppState, _kind: &Screen) {
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
