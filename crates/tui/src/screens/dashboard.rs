use backend::AppState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    navigation::{DashboardFocus, DashboardPage, DashboardState, Screen},
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

fn handle_key(app: &AppState, key: KeyEvent, state: &mut DashboardState) -> Option<AppEffect> {
    if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let current = app.config.show_sidebar;
        state.focus = if current {
            DashboardFocus::Content
        } else {
            DashboardFocus::Nav
        };

        return Some(Box::new(move |app| {
            app.config.show_sidebar = !current;
            let _ = app.save_config();
        }));
    }

    let sidebar_visible = app.config.show_sidebar;

    if !sidebar_visible {
        state.focus = DashboardFocus::Content;
    }

    if key.code == KeyCode::Left && sidebar_visible {
        state.focus = DashboardFocus::Nav;
        return None;
    }

    if key.code == KeyCode::Right {
        state.focus = DashboardFocus::Content;
        return None;
    }

    if sidebar_visible && state.focus == DashboardFocus::Nav {
        if key.code == KeyCode::Up {
            state.active_page = prev_page(state.active_page);
            return None;
        }

        if key.code == KeyCode::Down {
            state.active_page = next_page(state.active_page);
            return None;
        }
    }

    match key.code {
        KeyCode::Char('1') if sidebar_visible && state.focus == DashboardFocus::Nav => {
            state.active_page = DashboardPage::Home
        }
        KeyCode::Char('2') if sidebar_visible && state.focus == DashboardFocus::Nav => {
            state.active_page = DashboardPage::Settings
        }
        KeyCode::Char('3') if sidebar_visible && state.focus == DashboardFocus::Nav => {
            state.active_page = DashboardPage::Debug
        }
        KeyCode::Char('4') if sidebar_visible && state.focus == DashboardFocus::Nav => {
            state.active_page = DashboardPage::Credits
        }
        KeyCode::Char('t')
            if state.focus == DashboardFocus::Content
                && state.active_page == DashboardPage::Settings =>
        {
            return Some(Box::new(move |app| {
                app.config.enable_telemetry = Some(!app.config.enable_telemetry.unwrap_or(false));
                let _ = app.save_config();
            }));
        }
        KeyCode::Char('r')
            if state.focus == DashboardFocus::Content
                && state.active_page == DashboardPage::Debug =>
        {
            return Some(Box::new(move |app| {
                let _ = app.load_db();
            }));
        }
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
    let sidebar_visible = app.config.show_sidebar;

    let footer = keybind_hint(state, sidebar_visible);
    let (inner, area) = full_rect(a, "Stassh Dashboard", footer);

    frame.render_widget(inner, a);

    let layout = if sidebar_visible {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(area)
    };

    let mut content_host = layout[1];

    if sidebar_visible {
        let nav_border = if state.focus == DashboardFocus::Nav {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        };

        let sidebar_block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(nav_border);
        let sidebar_area = sidebar_block.inner(layout[0]);
        frame.render_widget(sidebar_block, layout[0]);

        let sidebar = Paragraph::new(format!(
            "{}\n{}\n{}\n{}",
            nav_line(state, DashboardPage::Home, 1, "Home"),
            nav_line(state, DashboardPage::Settings, 2, "Settings"),
            nav_line(state, DashboardPage::Debug, 3, "Debug"),
            nav_line(state, DashboardPage::Credits, 4, "Credits"),
        ))
        .alignment(Alignment::Left);
        frame.render_widget(sidebar, sidebar_area);
    } else {
        content_host = layout[0];
    }

    let content_block = Block::default()
        .title("Content")
        .borders(Borders::LEFT)
        .border_style(if state.focus == DashboardFocus::Content {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let content_area = content_block.inner(content_host);
    frame.render_widget(content_block, content_host);

    let content = match state.active_page {
        DashboardPage::Home => render_home(app),
        DashboardPage::Settings => render_settings(app),
        DashboardPage::Debug => render_debug(app),
        DashboardPage::Credits => render_credits(),
    };
    frame.render_widget(content, content_area);
}

fn nav_line(state: &DashboardState, page: DashboardPage, index: u8, title: &str) -> String {
    let label = if state.focus == DashboardFocus::Nav {
        format!("{} {}", index, title)
    } else {
        title.to_string()
    };

    if state.active_page == page {
        format!("> {}", label)
    } else {
        format!("  {}", label)
    }
}

fn next_page(page: DashboardPage) -> DashboardPage {
    match page {
        DashboardPage::Home => DashboardPage::Settings,
        DashboardPage::Settings => DashboardPage::Debug,
        DashboardPage::Debug => DashboardPage::Credits,
        DashboardPage::Credits => DashboardPage::Home,
    }
}

fn prev_page(page: DashboardPage) -> DashboardPage {
    match page {
        DashboardPage::Home => DashboardPage::Credits,
        DashboardPage::Settings => DashboardPage::Home,
        DashboardPage::Debug => DashboardPage::Settings,
        DashboardPage::Credits => DashboardPage::Debug,
    }
}

fn keybind_hint(state: &DashboardState, sidebar_visible: bool) -> &'static str {
    if !sidebar_visible {
        return match state.active_page {
            DashboardPage::Home => "SIDEBAR hidden (Ctrl+B): CONTENT Home selected",
            DashboardPage::Settings => {
                "SIDEBAR hidden (Ctrl+B): CONTENT Settings selected, T toggle telemetry"
            }
            DashboardPage::Debug => "SIDEBAR hidden (Ctrl+B): CONTENT Debug selected, R reload db",
            DashboardPage::Credits => "SIDEBAR hidden (Ctrl+B): CONTENT Credits selected",
        };
    }

    match (state.focus, state.active_page) {
        (DashboardFocus::Nav, _) => {
            "NAV selected: ←/→ switch panel, ↑/↓ cycle pages, 1-4 jump pages, Ctrl+B hide"
        }
        (DashboardFocus::Content, DashboardPage::Home) => {
            "CONTENT Home selected: ←/→ switch panel, Ctrl+B hide"
        }
        (DashboardFocus::Content, DashboardPage::Settings) => {
            "CONTENT Settings selected: T toggle telemetry, ←/→ switch panel, Ctrl+B hide"
        }
        (DashboardFocus::Content, DashboardPage::Debug) => {
            "CONTENT Debug selected: R reload db, ←/→ switch panel, Ctrl+B hide"
        }
        (DashboardFocus::Content, DashboardPage::Credits) => {
            "CONTENT Credits selected: ←/→ switch panel, Ctrl+B hide"
        }
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
        "Settings\n\n- Telemetry enabled: {:?}\n- Database encryption: {:?}\n- Sidebar visible: {:?}\n\n(T to toggle telemetry, Ctrl+B to toggle sidebar)",
        app.config.enable_telemetry, app.config.db_encryption, app.config.show_sidebar,
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
