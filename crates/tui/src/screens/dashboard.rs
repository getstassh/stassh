use backend::AppState;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    navigation::{DashboardPage, DashboardState, Screen},
    screens::{AppEffect, ScreenHandler},
    ui::full_rect,
};

pub(crate) static HANDLER: ScreenHandler<DashboardState> = ScreenHandler {
    matches: |s| matches!(s, Screen::Dashboard { .. }),
    get: |s| match s {
        Screen::Dashboard { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::Dashboard { state } => Some(state),
        _ => None,
    },
    render: ui,
    handle_key: handle_key,
    handle_tick: handle_tick,
};

fn handle_key(_app: &AppState, key_code: KeyCode, state: &mut DashboardState) -> Option<AppEffect> {
    match key_code {
        KeyCode::Char('1') => state.active_page = DashboardPage::Home,
        KeyCode::Char('2') => state.active_page = DashboardPage::Settings,
        KeyCode::Char('3') => state.active_page = DashboardPage::Debug,
        KeyCode::Char('4') => state.active_page = DashboardPage::Credits,
        _ => {}
    }

    None
}

fn handle_tick(_app: &AppState, _state: &mut DashboardState) -> Option<AppEffect> {
    Some(Box::new(move |app| {
        app.db.index += 1;
        let _ = app.save_db();
    }))
}

fn ui(frame: &mut Frame, app: &AppState, state: &DashboardState) {
    let a = frame.area();

    let (inner, area) = full_rect(a, "Stassh Dashboard", "1-4 Nav");

    frame.render_widget(inner, a);

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(area);

    let sidebar_block = Block::default().borders(Borders::RIGHT);
    let sidebar_area = sidebar_block.inner(layout[0]);
    frame.render_widget(sidebar_block, layout[0]);

    let sidebar = Paragraph::new(format!(
        "{}\n{}\n{}\n{}",
        nav_line(state, DashboardPage::Home, "1 Home"),
        nav_line(state, DashboardPage::Settings, "2 Settings"),
        nav_line(state, DashboardPage::Debug, "3 Debug"),
        nav_line(state, DashboardPage::Credits, "4 Credits"),
    ))
    .alignment(Alignment::Left);
    frame.render_widget(sidebar, sidebar_area);

    let content = match state.active_page {
        DashboardPage::Home => render_home(app),
        DashboardPage::Settings => render_settings(app),
        DashboardPage::Debug => render_debug(app),
        DashboardPage::Credits => render_credits(),
    };
    frame.render_widget(content, layout[1]);
}

fn nav_line(state: &DashboardState, page: DashboardPage, title: &str) -> String {
    if state.active_page == page {
        format!("> {}", title)
    } else {
        format!("  {}", title)
    }
}

fn render_home(app: &AppState) -> Paragraph<'static> {
    Paragraph::new(format!(
        "Welcome to stassh!\n\nApp: {}\nEncryption: {:?}\nTelemetry: {:?}",
        app.app_name(),
        app.config.db_encryption,
        app.config.enable_telemetry,
    ))
    .alignment(Alignment::Left)
}

fn render_settings(app: &AppState) -> Paragraph<'static> {
    Paragraph::new(format!(
        "Settings\n\n- Telemetry enabled: {:?}\n- Database encryption: {:?}",
        app.config.enable_telemetry, app.config.db_encryption,
    ))
    .alignment(Alignment::Left)
}

fn render_debug(app: &AppState) -> Paragraph<'static> {
    Paragraph::new(format!(
        "Debug\n\nConfig object:\n{:#?}\n\nDB object:\n{:#?}",
        app.config, app.db,
    ))
    .alignment(Alignment::Left)
}

fn render_credits() -> Paragraph<'static> {
    Paragraph::new(
        "Credits\n\nBuilt by Lazar\nTerminal UI: ratatui + crossterm\nThanks for using stassh.",
    )
    .alignment(Alignment::Left)
}
