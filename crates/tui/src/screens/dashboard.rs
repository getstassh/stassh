use std::{
    net::{TcpStream, ToSocketAddrs},
    thread,
    time::{Duration, Instant},
};

use backend::{AppState, HostAuth, SshHost, TrustedHostKey};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostAuthMode, HostConnectionStatus, HostFormField,
        HostFormState, HostModalMode, HostModalState, HostProbeTask, Screen, SshSessionPhase,
        SshSessionState,
    },
    screens::{AppEffect, ScreenHandler},
    ssh_client::{
        SessionEvent, SessionInput, StartSessionResult, TrustChallenge, start_session_async,
    },
    ui::full_rect,
};

const HOME_GRID_COLUMNS: usize = 3;
const HOST_CARD_HEIGHT: u16 = 6;
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

    if state.active_page == DashboardPage::Ssh {
        return handle_ssh_key(key, state);
    }

    if state.active_page != DashboardPage::Home {
        if state.active_page == DashboardPage::Debug {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    state.debug_scroll = state.debug_scroll.saturating_sub(1);
                    return None;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.debug_scroll = state.debug_scroll.saturating_add(1);
                    return None;
                }
                KeyCode::PageUp => {
                    state.debug_scroll = state.debug_scroll.saturating_sub(8);
                    return None;
                }
                KeyCode::PageDown => {
                    state.debug_scroll = state.debug_scroll.saturating_add(8);
                    return None;
                }
                _ => {}
            }
        }
        return None;
    }

    match key.code {
        KeyCode::Char('a') => {
            state.host_modal = Some(HostModalState {
                mode: HostModalMode::Create,
                form: HostFormState::new(),
            });
        }
        KeyCode::Char('e') => {
            if let Some(host) = app.db.hosts.get(state.selected_host) {
                state.host_modal = Some(HostModalState {
                    mode: HostModalMode::Edit { host_id: host.id },
                    form: form_from_host(host),
                });
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            state.selected_host = move_left(state.selected_host, app.db.hosts.len());
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.selected_host = move_right(state.selected_host, app.db.hosts.len());
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected_host = move_up(state.selected_host, app.db.hosts.len());
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected_host = move_down(state.selected_host, app.db.hosts.len());
        }
        KeyCode::Enter => {
            if let Some(host) = app.db.hosts.get(state.selected_host) {
                let host_id = host.id;
                let title = format!("{}@{}:{}", host.user, host.host, host.port);
                let rows_cols = crossterm::terminal::size().unwrap_or((120, 40));
                let rows = rows_cols.1;
                let cols = rows_cols.0;
                state
                    .ssh_tabs
                    .push(SshSessionState::new_starting(title, rows, cols, host_id));
                let idx = state.ssh_tabs.len().saturating_sub(1);
                state.active_ssh_tab = Some(idx);
                state.active_page = DashboardPage::Ssh;
                state.sidebar_cursor = DashboardState::FIXED_SIDEBAR_ITEMS + idx;
            }
        }
        _ => {}
    }

    None
}

fn handle_paste(_app: &AppState, text: &str, state: &mut DashboardState) -> Option<AppEffect> {
    if let Some(modal) = &mut state.host_modal {
        insert_pasted_text(&mut modal.form, text);
        return None;
    }

    if state.active_page == DashboardPage::Ssh && state.active_ssh_tab.is_some() {
        if let Some(tab) = active_ssh_tab_mut(state) {
            if let SshSessionPhase::Running { live } = &tab.phase {
                live.send_input(SessionInput::Data(text.as_bytes().to_vec()));
            }
        }
    }

    None
}

fn handle_tick(app: &AppState, state: &mut DashboardState) -> Option<AppEffect> {
    tick_ssh_tabs(app, state);

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
    if cols == 0 || rows == 0 {
        return None;
    }

    for tab in &mut state.ssh_tabs {
        tab.resize(rows, cols);
        if let SshSessionPhase::Running { live } = &tab.phase {
            live.send_input(SessionInput::Resize {
                cols: cols.max(1),
                rows: rows.max(1),
            });
        }
    }

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

fn handle_ssh_key(key: KeyEvent, state: &mut DashboardState) -> Option<AppEffect> {
    let Some(tab_idx) = state.active_ssh_tab else {
        state.active_page = DashboardPage::Home;
        state.sidebar_cursor = 0;
        return None;
    };

    let Some(tab) = state.ssh_tabs.get_mut(tab_idx) else {
        state.active_ssh_tab = None;
        state.active_page = DashboardPage::Home;
        state.sidebar_cursor = 0;
        return None;
    };

    let mut close_status: Option<String> = None;
    let mut trust_key: Option<(u32, TrustedHostKey)> = None;

    match &mut tab.phase {
        SshSessionPhase::Starting { pending, .. } => {
            if key.code == KeyCode::Esc {
                if let Some(pending) = pending {
                    pending.cancel();
                }
                close_status = Some("Connection canceled".to_string());
            }
        }
        SshSessionPhase::TrustPrompt { host_id, challenge } => {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) {
                trust_key = Some((*host_id, challenge.proposed_key.clone()));
            }

            if matches!(key.code, KeyCode::Char('n') | KeyCode::Esc) {
                close_status = Some("Connection canceled: host key not trusted".to_string());
            }
        }
        SshSessionPhase::Running { live } => {
            if key.code == KeyCode::Esc {
                live.send_input(SessionInput::Disconnect);
                return None;
            }

            if let Some(bytes) = key_to_bytes(key) {
                live.send_input(SessionInput::Data(bytes));
            }
        }
    }

    if let Some((host_id, key)) = trust_key {
        if let Some(tab) = state.ssh_tabs.get_mut(tab_idx) {
            tab.phase = SshSessionPhase::starting(host_id);
        }
        return Some(Box::new(move |app| {
            trust_host_key(app, key);
        }));
    }

    if let Some(status) = close_status {
        close_ssh_tab(state, tab_idx, status);
    }

    None
}

fn tick_ssh_tabs(app: &AppState, state: &mut DashboardState) {
    let mut idx = 0;
    while idx < state.ssh_tabs.len() {
        let mut close_status = None;
        let tab = &mut state.ssh_tabs[idx];

        let mut next_phase = None;
        match &mut tab.phase {
            SshSessionPhase::Starting {
                host_id,
                pending,
                spinner_frame,
                ..
            } => {
                *spinner_frame = spinner_frame.wrapping_add(1);

                if pending.is_none() {
                    if let Some(host) = app.db.hosts.iter().find(|h| h.id == *host_id).cloned() {
                        *pending = Some(start_session_async(
                            &host,
                            &app.db.trusted_host_keys,
                            tab.last_good_rows,
                            tab.last_good_cols,
                            app.config.ssh_idle_timeout_seconds,
                            app.config.ssh_connect_timeout_seconds,
                        ));
                    } else {
                        close_status = Some("Selected host no longer exists".to_string());
                    }
                }

                if close_status.is_none() {
                    if let Some(pending_start) = pending.as_mut() {
                        if let Some(result) = pending_start.try_recv() {
                            match result {
                                StartSessionResult::Started(live) => {
                                    next_phase = Some(SshSessionPhase::Running { live });
                                }
                                StartSessionResult::TrustRequired(challenge) => {
                                    next_phase = Some(SshSessionPhase::TrustPrompt {
                                        host_id: *host_id,
                                        challenge,
                                    });
                                }
                                StartSessionResult::Error(error) => {
                                    close_status = Some(error);
                                }
                            }
                        }
                    }
                }
            }
            SshSessionPhase::TrustPrompt { .. } => {}
            SshSessionPhase::Running { live } => {
                let parser = &mut tab.parser;
                while let Some(event) = live.try_recv() {
                    match event {
                        SessionEvent::OutputBytes(bytes) => parser.process(&bytes),
                        SessionEvent::Error(error) => {
                            if close_status.is_none() {
                                close_status = Some(error);
                            }
                        }
                        SessionEvent::Closed(status) => {
                            if close_status.is_none() {
                                close_status = Some(status);
                            }
                        }
                    }
                }
            }
        }

        if let Some(phase) = next_phase {
            tab.phase = phase;
        }

        if let Some(status) = close_status {
            if let Some(tab) = state.ssh_tabs.get_mut(idx) {
                if let SshSessionPhase::Running { live } = &mut tab.phase {
                    live.stop();
                }
            }
            close_ssh_tab(state, idx, status);
            continue;
        }

        idx += 1;
    }
}

fn close_ssh_tab(state: &mut DashboardState, idx: usize, status: String) {
    if idx >= state.ssh_tabs.len() {
        return;
    }

    state.ssh_tabs.remove(idx);
    state.last_status = Some(status);

    if state.ssh_tabs.is_empty() {
        state.active_ssh_tab = None;
        state.active_page = DashboardPage::Home;
        state.sidebar_cursor = 0;
        return;
    }

    let next_idx = idx.min(state.ssh_tabs.len().saturating_sub(1));
    state.active_ssh_tab = Some(next_idx);
    if state.active_page == DashboardPage::Ssh {
        state.sidebar_cursor = DashboardState::FIXED_SIDEBAR_ITEMS + next_idx;
    }
}

fn active_ssh_tab_mut(state: &mut DashboardState) -> Option<&mut SshSessionState> {
    let idx = state.active_ssh_tab?;
    state.ssh_tabs.get_mut(idx)
}

fn trust_host_key(app: &mut crate::app::App, key: TrustedHostKey) {
    app.db
        .trusted_host_keys
        .retain(|k| !(k.host == key.host && k.port == key.port));
    app.db.trusted_host_keys.push(key);
    let _ = app.save_db();
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

fn form_from_host(host: &SshHost) -> HostFormState {
    let mut form = HostFormState::new();
    form.name = host.name.clone();
    form.host = host.host.clone();
    form.user = host.user.clone();
    form.port = host.port.to_string();
    match &host.auth {
        HostAuth::Key { key_path } => {
            form.auth_mode = HostAuthMode::Key;
            form.key_path = key_path.clone();
        }
        HostAuth::Password { password } => {
            form.auth_mode = HostAuthMode::Password;
            form.password = password.clone();
        }
    }
    form
}

fn move_left(selected: usize, hosts_len: usize) -> usize {
    if hosts_len == 0 {
        return 0;
    }
    if selected % HOME_GRID_COLUMNS == 0 {
        selected
    } else {
        selected.saturating_sub(1)
    }
}

fn move_right(selected: usize, hosts_len: usize) -> usize {
    if hosts_len == 0 {
        return 0;
    }
    if (selected % HOME_GRID_COLUMNS) == HOME_GRID_COLUMNS - 1 || selected + 1 >= hosts_len {
        selected
    } else {
        selected + 1
    }
}

fn move_up(selected: usize, hosts_len: usize) -> usize {
    if hosts_len == 0 {
        return 0;
    }
    if selected >= HOME_GRID_COLUMNS {
        selected - HOME_GRID_COLUMNS
    } else {
        selected
    }
}

fn move_down(selected: usize, hosts_len: usize) -> usize {
    if hosts_len == 0 {
        return 0;
    }

    let next_row_same_col = selected + HOME_GRID_COLUMNS;
    if next_row_same_col < hosts_len {
        return next_row_same_col;
    }

    let next_row_start = ((selected / HOME_GRID_COLUMNS) + 1) * HOME_GRID_COLUMNS;
    if next_row_start < hosts_len {
        return next_row_start;
    }

    selected
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
        DashboardPage::Home => render_home(frame, content_area, app, state),
        DashboardPage::Settings => frame.render_widget(render_settings(app), content_area),
        DashboardPage::Debug => render_debug(frame, content_area, app, state),
        DashboardPage::Credits => frame.render_widget(render_credits(), content_area),
        DashboardPage::Ssh => render_ssh_workspace(frame, a, content_area, state),
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

fn render_home(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    if app.db.hosts.is_empty() {
        let message = if let Some(status) = &state.last_status {
            format!(
                "No hosts yet.\n\nPress A to create your first SSH host.\n\nLast status: {status}"
            )
        } else {
            "No hosts yet.\n\nPress A to create your first SSH host.".to_string()
        };
        let empty = Paragraph::new(message).alignment(Alignment::Left);
        frame.render_widget(empty, area);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let header = if let Some(status) = &state.last_status {
        format!("Last status: {status}")
    } else {
        "Ready to connect.".to_string()
    };
    frame.render_widget(Paragraph::new(header), layout[0]);

    let grid_area = layout[1];

    let columns = HOME_GRID_COLUMNS.min(app.db.hosts.len().max(1));
    let rows = app.db.hosts.len().div_ceil(columns);

    let mut row_constraints = vec![Constraint::Length(HOST_CARD_HEIGHT); rows];
    row_constraints.push(Constraint::Fill(1));
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(grid_area);

    for (row_idx, row_area) in row_areas.iter().take(rows).enumerate() {
        let column_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Ratio(1, columns as u32); columns])
            .split(*row_area);

        for (col_idx, col_area) in column_areas.iter().enumerate() {
            let index = row_idx * columns + col_idx;
            if let Some(host) = app.db.hosts.get(index) {
                let selected = index == state.selected_host;
                let status = state
                    .host_statuses
                    .get(&host.id)
                    .copied()
                    .unwrap_or(HostConnectionStatus::Unknown);
                render_host_card(frame, *col_area, host, selected, status);
            }
        }
    }
}

fn render_host_card(
    frame: &mut Frame,
    area: Rect,
    host: &SshHost,
    selected: bool,
    status: HostConnectionStatus,
) {
    let border = if selected {
        Style::default().fg(Color::Yellow)
    } else if status == HostConnectionStatus::Unreachable {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(format!(" {} ", host.name))
        .borders(Borders::ALL)
        .border_style(border)
        .style(Style::default());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let auth_label = match &host.auth {
        HostAuth::Key { .. } => "key",
        HostAuth::Password { .. } => "password",
    };

    let content = Paragraph::new(format!(
        "{}@{}:{}\nauth: {}\nstatus: {}",
        host.user,
        host.host,
        host.port,
        auth_label,
        host_status_label(status),
    ));
    frame.render_widget(content, inner);
}

fn host_status_label(status: HostConnectionStatus) -> &'static str {
    match status {
        HostConnectionStatus::Unknown => "unknown",
        HostConnectionStatus::Checking => "checking",
        HostConnectionStatus::Reachable => "reachable",
        HostConnectionStatus::Unreachable => "unreachable",
    }
}

fn render_ssh_workspace(frame: &mut Frame, app_area: Rect, area: Rect, state: &DashboardState) {
    let Some(tab_idx) = state.active_ssh_tab else {
        frame.render_widget(
            Paragraph::new("No active SSH session. Open one from Home.").alignment(Alignment::Left),
            area,
        );
        return;
    };

    let Some(tab) = state.ssh_tabs.get(tab_idx) else {
        frame.render_widget(
            Paragraph::new("No active SSH session. Open one from Home.").alignment(Alignment::Left),
            area,
        );
        return;
    };

    match &tab.phase {
        SshSessionPhase::Starting {
            spinner_frame,
            started_at,
            ..
        } => {
            const FRAMES: [&str; 8] = ["-", "\\", "|", "/", "-", "\\", "|", "/"];
            let spinner = FRAMES[*spinner_frame % FRAMES.len()];
            let elapsed = started_at.elapsed().as_secs_f32();
            frame.render_widget(
                Paragraph::new(format!(
                    "{spinner} Connecting to {}\n\nPlease wait... ({elapsed:.1}s)",
                    tab.title
                ))
                .alignment(Alignment::Center),
                area,
            );
        }
        SshSessionPhase::TrustPrompt { challenge, .. } => {
            frame.render_widget(
                Paragraph::new(render_vt100_text(&tab.parser)).alignment(Alignment::Left),
                area,
            );
            render_trust_modal(frame, app_area, challenge);
        }
        SshSessionPhase::Running { .. } => {
            frame.render_widget(
                Paragraph::new(render_vt100_text(&tab.parser)).alignment(Alignment::Left),
                area,
            );
        }
    }
}

fn render_trust_modal(frame: &mut Frame, app_area: Rect, challenge: &TrustChallenge) {
    let width = (app_area.width.saturating_sub(4)).min(90);
    let height = 12;
    let popup_area = centered_rect_no_border(width, height, app_area);

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" Host Key Verification ")
        .title_bottom(" Y/Enter trust and connect | N/Esc cancel ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let body = if let Some(previous) = &challenge.previous_fingerprint {
        format!(
            "WARNING: host key changed for {}:{}\nOld fingerprint: {}\nNew fingerprint: {}\nAlgorithm: {}",
            challenge.proposed_key.host,
            challenge.proposed_key.port,
            previous,
            challenge.proposed_key.fingerprint_sha256,
            challenge.proposed_key.algorithm,
        )
    } else {
        format!(
            "First connection to {}:{}\nFingerprint: {}\nAlgorithm: {}",
            challenge.proposed_key.host,
            challenge.proposed_key.port,
            challenge.proposed_key.fingerprint_sha256,
            challenge.proposed_key.algorithm,
        )
    };

    frame.render_widget(Paragraph::new(body).alignment(Alignment::Left), inner);
}

fn render_vt100_text(parser: &vt100::Parser) -> Text<'static> {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 {
        return Text::from(Vec::<Line<'static>>::new());
    }

    let (raw_cursor_row, raw_cursor_col) = screen.cursor_position();
    let cursor_visible = !screen.hide_cursor();
    let cursor_row = raw_cursor_row.min(rows.saturating_sub(1));
    let cursor_col = raw_cursor_col.min(cols.saturating_sub(1));
    let mut lines = Vec::with_capacity(rows as usize);

    for r in 0..rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut run = String::new();
        let mut run_style: Option<Style> = None;

        for c in 0..cols {
            let Some(cell) = screen.cell(r, c) else {
                continue;
            };

            if cell.is_wide_continuation() {
                continue;
            }

            let mut style = style_from_cell(cell);
            let is_cursor = cursor_visible && r == cursor_row && c == cursor_col;
            if is_cursor {
                style = Style::default()
                    .fg(Color::Gray)
                    .bg(Color::White)
                    .add_modifier(Modifier::DIM);
            }

            let text = if is_cursor {
                if cell.has_contents() {
                    cell.contents().to_string()
                } else {
                    " ".to_string()
                }
            } else if cell.has_contents() {
                cell.contents().to_string()
            } else {
                " ".to_string()
            };

            if run_style == Some(style) {
                run.push_str(&text);
            } else {
                if let Some(prev_style) = run_style {
                    spans.push(Span::styled(std::mem::take(&mut run), prev_style));
                }
                run_style = Some(style);
                run.push_str(&text);
            }
        }

        if let Some(prev_style) = run_style {
            spans.push(Span::styled(run, prev_style));
        }
        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

fn style_from_cell(cell: &vt100::Cell) -> Style {
    let mut style = Style::default();

    style = style.fg(map_color(cell.fgcolor()));
    style = style.bg(map_color(cell.bgcolor()));

    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.dim() {
        style = style.add_modifier(Modifier::DIM);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }

    style
}

fn map_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn key_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = key.code {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                let v = (lower as u8) - b'a' + 1;
                return Some(vec![v]);
            }
        }
    }

    match key.code {
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::CONTROL) => Some(vec![0x17]),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Char(c) => Some(c.to_string().into_bytes()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        _ => None,
    }
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
        DashboardPage::Home => {
            "HOME: arrows or hjkl move | A add | E edit | Enter connect | R refresh | Ctrl+B toggle sidebar | Esc exit"
        }
        DashboardPage::Settings => "Ctrl+B toggle sidebar | Esc exit",
        DashboardPage::Debug => {
            "j/k or Up/Down scroll | PageUp/PageDown jump | Ctrl+B toggle sidebar | Esc exit"
        }
        DashboardPage::Credits => "Ctrl+B toggle sidebar | Esc exit",
        DashboardPage::Ssh => {
            "SSH: type to send input | Esc disconnect active | Ctrl+B toggle sidebar"
        }
    }
}

fn render_settings(app: &AppState) -> Paragraph<'static> {
    Paragraph::new(format!(
        "Settings\n\n- Telemetry enabled: {:?}\n- Database encryption: {:?}\n- Sidebar visible: {:?}\n- SSH idle timeout (seconds): {}",
        app.config.enable_telemetry, app.config.db_encryption, app.config.show_sidebar, app.config.ssh_idle_timeout_seconds,
    ))
    .alignment(Alignment::Left)
}

fn render_debug(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
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

fn render_credits() -> Paragraph<'static> {
    Paragraph::new(
        "Credits\n\nBuilt by Lazar\nTerminal UI: ratatui + crossterm\nThanks for using stassh.",
    )
    .alignment(Alignment::Left)
}
