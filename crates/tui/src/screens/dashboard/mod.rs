use std::{
    net::{TcpStream, ToSocketAddrs},
    thread,
    time::{Duration, Instant},
};

use backend::{AppState, HostAuth, SshHost};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostAuthMode, HostConnectionStatus, HostFormField,
        HostFormState, HostModalMode, HostModalState, HostProbeTask, Screen,
    },
    screens::{AppEffect, ScreenHandler},
    ui::full_rect,
};

mod pages;

const HOST_PROBE_INTERVAL: Duration = Duration::from_secs(20);

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
    handle_key,
    handle_paste,
    handle_resize,
    handle_tick,
};

fn handle_key(app: &AppState, key: KeyEvent, state: &mut DashboardState) -> Option<AppEffect> {
    if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let current = app.config.show_sidebar;
        return Some(Box::new(move |app| {
            app.config.show_sidebar = !current;
            let _ = app.save_config();
        }));
    }

    if let Some(modal) = &mut state.host_modal {
        return handle_modal_key(app, key, state.selected_host, modal);
    }

    match key.code {
        KeyCode::Char('1') => {
            state.active_page = DashboardPage::Home;
            state.sidebar_cursor = 0;
        }
        KeyCode::Char('2') => {
            state.active_page = DashboardPage::Settings;
            state.sidebar_cursor = 1;
        }
        KeyCode::Char('3') => {
            state.active_page = DashboardPage::Debug;
            state.sidebar_cursor = 2;
        }
        KeyCode::Char('4') => {
            state.active_page = DashboardPage::Credits;
            state.sidebar_cursor = 3;
        }
        _ => {}
    }

    if app.config.show_sidebar {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                state.sidebar_cursor = state.sidebar_cursor.saturating_sub(1);
                return None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = state.sidebar_items_count().saturating_sub(1);
                state.sidebar_cursor = (state.sidebar_cursor + 1).min(max);
                return None;
            }
            KeyCode::Enter => {
                activate_sidebar_selection(state);
                return None;
            }
            _ => {}
        }

        return None;
    }

    match state.active_page {
        DashboardPage::Home => pages::home::handle_key(app, key, state),
        DashboardPage::Debug => pages::debug::handle_key(key, state),
        DashboardPage::Ssh => pages::ssh::handle_key(key, state),
        DashboardPage::Settings | DashboardPage::Credits => None,
    }
}

fn handle_paste(_app: &AppState, text: &str, state: &mut DashboardState) -> Option<AppEffect> {
    if let Some(modal) = &mut state.host_modal {
        insert_pasted_text(&mut modal.form, text);
        return None;
    }

    if state.active_page == DashboardPage::Ssh {
        pages::ssh::handle_paste(text, state);
    }

    None
}

fn handle_tick(app: &AppState, state: &mut DashboardState) -> Option<AppEffect> {
    pages::ssh::tick_tabs(app, state);

    reap_probe_tasks(state);
    sync_host_status_maps(app, state);

    let should_probe =
        state.needs_initial_probe || state.last_probe_at.elapsed() >= HOST_PROBE_INTERVAL;
    if should_probe {
        start_probe_round(app, state);
        state.last_probe_at = Instant::now();
        state.needs_initial_probe = false;
    }

    None
}

fn handle_resize(
    _app: &AppState,
    cols: u16,
    rows: u16,
    state: &mut DashboardState,
) -> Option<AppEffect> {
    pages::ssh::handle_resize(cols, rows, state);
    None
}

fn reap_probe_tasks(state: &mut DashboardState) {
    let mut idx = 0;
    while idx < state.probe_tasks.len() {
        if !state.probe_tasks[idx].join.is_finished() {
            idx += 1;
            continue;
        }

        let task = state.probe_tasks.swap_remove(idx);
        let reachable = task.join.join().unwrap_or(false);
        let status = if reachable {
            HostConnectionStatus::Reachable
        } else {
            HostConnectionStatus::Unreachable
        };
        state.host_statuses.insert(task.host_id, status);
    }
}

fn sync_host_status_maps(app: &AppState, state: &mut DashboardState) {
    let host_ids = app.db.hosts.iter().map(|h| h.id).collect::<Vec<_>>();
    state.host_statuses.retain(|id, _| host_ids.contains(id));

    for host in &app.db.hosts {
        state
            .host_statuses
            .entry(host.id)
            .or_insert(HostConnectionStatus::Unknown);
    }
}

fn start_probe_round(app: &AppState, state: &mut DashboardState) {
    let timeout = Duration::from_secs(app.config.ssh_connect_timeout_seconds.max(1));

    for host in &app.db.hosts {
        if state.probe_tasks.iter().any(|task| task.host_id == host.id) {
            continue;
        }

        state
            .host_statuses
            .insert(host.id, HostConnectionStatus::Checking);
        let host_id = host.id;
        let host_name = host.host.clone();
        let port = host.port;

        let join = thread::spawn(move || host_is_reachable(&host_name, port, timeout));
        state.probe_tasks.push(HostProbeTask { host_id, join });
    }
}

fn host_is_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };

    for addr in addrs {
        if TcpStream::connect_timeout(&addr, timeout).is_ok() {
            return true;
        }
    }

    false
}

fn handle_modal_key(
    app: &AppState,
    key: KeyEvent,
    selected_host: usize,
    modal: &mut HostModalState,
) -> Option<AppEffect> {
    if key.code == KeyCode::Esc {
        modal.form.error = None;
        return Some(Box::new(move |app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.host_modal = None;
            }
        }));
    }

    if key.code == KeyCode::Tab || key.code == KeyCode::Down || key.code == KeyCode::Char('j') {
        modal.form.focus = modal.form.focus.next();
        modal.form.error = None;
        return None;
    }

    if key.code == KeyCode::BackTab || key.code == KeyCode::Up || key.code == KeyCode::Char('k') {
        modal.form.focus = modal.form.focus.prev();
        modal.form.error = None;
        return None;
    }

    if modal.form.focus == HostFormField::AuthMode {
        if key.code == KeyCode::Left
            || key.code == KeyCode::Right
            || key.code == KeyCode::Char('h')
            || key.code == KeyCode::Char('l')
            || key.code == KeyCode::Enter
            || key.code == KeyCode::Char(' ')
        {
            modal.form.auth_mode = match modal.form.auth_mode {
                HostAuthMode::Key => HostAuthMode::Password,
                HostAuthMode::Password => HostAuthMode::Key,
            };
            modal.form.error = None;
            return None;
        }
    }

    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return save_modal(app, selected_host, modal);
    }

    if key.code == KeyCode::Enter && modal.form.focus == HostFormField::AuthValue {
        return save_modal(app, selected_host, modal);
    }

    edit_form_field(&mut modal.form, key);
    None
}

fn save_modal(
    app: &AppState,
    selected_host: usize,
    modal: &mut HostModalState,
) -> Option<AppEffect> {
    let form = modal.form.clone();
    let validation = validate_form(&form);
    let (name, host, user, port, auth) = match validation {
        Ok(v) => v,
        Err(err) => {
            modal.form.error = Some(err);
            return None;
        }
    };

    let mode = modal.mode;
    let create_selected_index = app.db.hosts.len();

    Some(Box::new(move |app| {
        match mode {
            HostModalMode::Create => {
                let id = app.db.next_host_id.max(1);
                app.db.next_host_id = id.saturating_add(1);
                app.db.hosts.push(SshHost {
                    id,
                    name,
                    host,
                    user,
                    port,
                    auth,
                });
                if let Screen::Dashboard { state } = &mut app.screen {
                    state.selected_host = create_selected_index;
                }
            }
            HostModalMode::Edit { host_id } => {
                if let Some(existing) = app.db.hosts.iter_mut().find(|h| h.id == host_id) {
                    existing.name = name;
                    existing.host = host;
                    existing.user = user;
                    existing.port = port;
                    existing.auth = auth;
                }
                let max_selected = app.db.hosts.len().saturating_sub(1);
                if let Screen::Dashboard { state } = &mut app.screen {
                    state.selected_host = selected_host.min(max_selected);
                }
            }
        }
        if let Screen::Dashboard { state } = &mut app.screen {
            state.host_modal = None;
        }
        let _ = app.save_db();
    }))
}

fn edit_form_field(form: &mut HostFormState, key: KeyEvent) {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return;
    }

    let target = match form.focus {
        HostFormField::Name => Some(&mut form.name),
        HostFormField::Host => Some(&mut form.host),
        HostFormField::User => Some(&mut form.user),
        HostFormField::Port => Some(&mut form.port),
        HostFormField::AuthValue => match form.auth_mode {
            HostAuthMode::Key => Some(&mut form.key_path),
            HostAuthMode::Password => Some(&mut form.password),
        },
        HostFormField::AuthMode => None,
    };

    let Some(field) = target else {
        return;
    };

    match key.code {
        KeyCode::Char(c) => field.push(c),
        KeyCode::Backspace => {
            field.pop();
        }
        _ => {}
    }
}

fn insert_pasted_text(form: &mut HostFormState, text: &str) {
    if text.is_empty() {
        return;
    }

    let target = match form.focus {
        HostFormField::Name => Some(&mut form.name),
        HostFormField::Host => Some(&mut form.host),
        HostFormField::User => Some(&mut form.user),
        HostFormField::Port => Some(&mut form.port),
        HostFormField::AuthValue => match form.auth_mode {
            HostAuthMode::Key => Some(&mut form.key_path),
            HostAuthMode::Password => Some(&mut form.password),
        },
        HostFormField::AuthMode => None,
    };

    if let Some(field) = target {
        field.push_str(text);
    }
}

fn validate_form(form: &HostFormState) -> Result<(String, String, String, u16, HostAuth), String> {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err("Name is required".to_string());
    }

    let host = form.host.trim().to_string();
    if host.is_empty() {
        return Err("Host is required".to_string());
    }

    let user = form.user.trim().to_string();
    if user.is_empty() {
        return Err("User is required".to_string());
    }

    let port = form
        .port
        .trim()
        .parse::<u16>()
        .map_err(|_| "Port must be a valid number (1-65535)".to_string())?;
    if port == 0 {
        return Err("Port must be a valid number (1-65535)".to_string());
    }

    let auth = match form.auth_mode {
        HostAuthMode::Key => {
            let key_path = form.key_path.trim().to_string();
            if key_path.is_empty() {
                return Err("Key path is required".to_string());
            }
            HostAuth::Key { key_path }
        }
        HostAuthMode::Password => {
            let password = form.password.trim().to_string();
            if password.is_empty() {
                return Err("Password is required".to_string());
            }
            HostAuth::Password { password }
        }
    };

    Ok((name, host, user, port, auth))
}

#[derive(Clone, Copy)]
enum SidebarTarget {
    Page(DashboardPage),
    Session(usize),
}

fn sidebar_target_for_cursor(state: &DashboardState) -> SidebarTarget {
    let idx = state
        .sidebar_cursor
        .min(state.sidebar_items_count().saturating_sub(1));

    match idx {
        0 => SidebarTarget::Page(DashboardPage::Home),
        1 => SidebarTarget::Page(DashboardPage::Settings),
        2 => SidebarTarget::Page(DashboardPage::Debug),
        3 => SidebarTarget::Page(DashboardPage::Credits),
        n => SidebarTarget::Session(n - DashboardState::FIXED_SIDEBAR_ITEMS),
    }
}

fn activate_sidebar_selection(state: &mut DashboardState) {
    match sidebar_target_for_cursor(state) {
        SidebarTarget::Page(page) => {
            state.active_page = page;
            state.sidebar_cursor = match page {
                DashboardPage::Home => 0,
                DashboardPage::Settings => 1,
                DashboardPage::Debug => 2,
                DashboardPage::Credits => 3,
                DashboardPage::Ssh => state.sidebar_cursor,
            };
        }
        SidebarTarget::Session(tab_idx) => {
            if tab_idx < state.ssh_tabs.len() {
                state.active_page = DashboardPage::Ssh;
                state.active_ssh_tab = Some(tab_idx);
                state.sidebar_cursor = DashboardState::FIXED_SIDEBAR_ITEMS + tab_idx;
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &AppState, state: &DashboardState) {
    let a = frame.area();
    let footer = keybind_hint(state, app.config.show_sidebar);
    let (inner, area) = full_rect(a, "Stassh", footer);
    frame.render_widget(inner, a);

    let layout = if app.config.show_sidebar {
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
    if app.config.show_sidebar {
        render_sidebar(frame, layout[0], state);
    } else {
        content_host = layout[0];
    }

    let content_block = Block::default();
    let content_area = content_block.inner(content_host);
    frame.render_widget(content_block, content_host);

    match state.active_page {
        DashboardPage::Home => pages::home::render(frame, content_area, app, state),
        DashboardPage::Settings => pages::settings::render(frame, content_area, app),
        DashboardPage::Debug => pages::debug::render(frame, content_area, app, state),
        DashboardPage::Credits => pages::credits::render(frame, content_area),
        DashboardPage::Ssh => pages::ssh::render(frame, a, content_area, state),
    }

    if let Some(modal) = &state.host_modal {
        render_host_modal(frame, a, modal);
    }
}

fn render_sidebar(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let sidebar_block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Black));
    let sidebar_area = sidebar_block.inner(area);
    frame.render_widget(sidebar_block, area);

    let mut constraints = vec![Constraint::Length(3); state.sidebar_items_count()];
    constraints.push(Constraint::Fill(1));
    let nav_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(sidebar_area);

    render_sidebar_item(
        frame,
        nav_areas[0],
        state,
        0,
        "Home",
        SidebarTarget::Page(DashboardPage::Home),
    );
    render_sidebar_item(
        frame,
        nav_areas[1],
        state,
        1,
        "Settings",
        SidebarTarget::Page(DashboardPage::Settings),
    );
    render_sidebar_item(
        frame,
        nav_areas[2],
        state,
        2,
        "Debug",
        SidebarTarget::Page(DashboardPage::Debug),
    );
    render_sidebar_item(
        frame,
        nav_areas[3],
        state,
        3,
        "Credits",
        SidebarTarget::Page(DashboardPage::Credits),
    );

    for (session_idx, tab) in state.ssh_tabs.iter().enumerate() {
        let item_idx = DashboardState::FIXED_SIDEBAR_ITEMS + session_idx;
        render_sidebar_item(
            frame,
            nav_areas[item_idx],
            state,
            item_idx,
            &tab.title,
            SidebarTarget::Session(session_idx),
        );
    }
}

fn render_sidebar_item(
    frame: &mut Frame,
    area: Rect,
    state: &DashboardState,
    index: usize,
    title: &str,
    target: SidebarTarget,
) {
    let cursor_selected = state.sidebar_cursor == index;
    let active = match target {
        SidebarTarget::Page(page) => state.active_page == page,
        SidebarTarget::Session(tab_idx) => {
            state.active_page == DashboardPage::Ssh && state.active_ssh_tab == Some(tab_idx)
        }
    };

    let border = if cursor_selected {
        Style::default().fg(Color::Yellow)
    } else if active {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default().borders(Borders::ALL).border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let label = if cursor_selected {
        format!("> {} {}", index + 1, title)
    } else {
        format!("  {} {}", index + 1, title)
    };
    let text = Paragraph::new(label).alignment(Alignment::Left);
    frame.render_widget(text, inner);
}

fn render_host_modal(frame: &mut Frame, app_area: Rect, modal: &HostModalState) {
    let width = (app_area.width.saturating_sub(4)).min(80);
    let height = 16;
    let popup_area = centered_rect_no_border(width, height, app_area);

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(match modal.mode {
            HostModalMode::Create => " Create Host ",
            HostModalMode::Edit { .. } => " Edit Host ",
        })
        .title_bottom(" Ctrl+S save | Esc cancel | Tab/Shift+Tab next/prev ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let auth_mode = match modal.form.auth_mode {
        HostAuthMode::Key => "key",
        HostAuthMode::Password => "password",
    };

    let auth_value_label = match modal.form.auth_mode {
        HostAuthMode::Key => "key path",
        HostAuthMode::Password => "password",
    };

    let auth_value = match modal.form.auth_mode {
        HostAuthMode::Key => modal.form.key_path.clone(),
        HostAuthMode::Password => mask(&modal.form.password),
    };

    let mut lines = vec![
        modal_line(
            "name",
            &modal.form.name,
            modal.form.focus == HostFormField::Name,
        ),
        modal_line(
            "host",
            &modal.form.host,
            modal.form.focus == HostFormField::Host,
        ),
        modal_line(
            "user",
            &modal.form.user,
            modal.form.focus == HostFormField::User,
        ),
        modal_line(
            "port",
            &modal.form.port,
            modal.form.focus == HostFormField::Port,
        ),
        modal_line(
            "auth",
            auth_mode,
            modal.form.focus == HostFormField::AuthMode,
        ),
        modal_line(
            auth_value_label,
            &auth_value,
            modal.form.focus == HostFormField::AuthValue,
        ),
    ];

    if let Some(error) = &modal.form.error {
        lines.push(String::new());
        lines.push(format!("Error: {error}"));
    }

    let content = Paragraph::new(lines.join("\n")).alignment(Alignment::Left);
    frame.render_widget(content, inner);
}

fn centered_rect_no_border(width: u16, height: u16, area: Rect) -> Rect {
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

fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "*".repeat(value.len())
    }
}

fn modal_line(label: &str, value: &str, selected: bool) -> String {
    let prefix = if selected { ">" } else { " " };
    format!("{prefix} {label:10} {value}")
}

fn keybind_hint(state: &DashboardState, sidebar_visible: bool) -> &'static str {
    if state.host_modal.is_some() {
        return "HOST form: Tab/Shift+Tab move field | Ctrl+S save | Esc cancel/exit";
    }

    if sidebar_visible {
        return "Use Up/Down (j/k) to choose | Enter open page/session | Ctrl+B hide sidebar | Esc exit";
    }

    match state.active_page {
        DashboardPage::Home => pages::home::footer_hint(),
        DashboardPage::Settings => "Ctrl+B toggle sidebar | Esc exit",
        DashboardPage::Debug => pages::debug::footer_hint(),
        DashboardPage::Credits => "Ctrl+B toggle sidebar | Esc exit",
        DashboardPage::Ssh => pages::ssh::footer_hint(),
    }
}
