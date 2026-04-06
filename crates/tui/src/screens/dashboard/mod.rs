use std::{
    net::{TcpStream, ToSocketAddrs},
    thread,
    time::{Duration, Instant},
};

use backend::{AppState, HostAuth, SshEndpoint, SshHost};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostAuthMode, HostConnectionStatus, HostFormField,
        HostFormState, HostKeyInputMode, HostKeyPickerState, HostModalMode, HostModalState,
        HostProbeTask, Screen,
    },
    screens::{AppEffect, ScreenHandler},
    ui::{
        accent_text, border, centered_rect_no_border, danger_text, frame_block, full_rect,
        modal_block, muted_text, selected_border, text,
    },
};

mod pages;

const HOST_PROBE_INTERVAL: Duration = Duration::from_secs(20);
const DEBUG_HOLD_DURATION: Duration = Duration::from_secs(10);
const DEBUG_HOLD_GAP_RESET: Duration = Duration::from_millis(450);

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
    if let Some(effect) = handle_debug_hold_toggle(key, state) {
        return Some(effect);
    }

    if !is_debug_hold_key(key) {
        state.debug_hold_started_at = None;
        state.debug_hold_last_seen_at = None;
    }

    if let Some(modal) = &mut state.host_modal {
        return handle_modal_key(app, key, state.selected_host, modal);
    }

    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.quick_switcher = Some(crate::navigation::QuickSwitcherState::new());
        return None;
    }

    if state.quick_switcher.is_some() {
        return handle_quick_switcher_key(key, state, app.config.show_debug_panel);
    }

    match state.active_page {
        DashboardPage::Home => pages::home::handle_key(app, key, state),
        DashboardPage::Debug => {
            if app.config.show_debug_panel {
                pages::debug::handle_key(key, state)
            } else {
                state.active_page = DashboardPage::Home;
                None
            }
        }
        DashboardPage::Ssh => pages::ssh::handle_key(key, state),
        DashboardPage::Settings => None,
    }
}

fn handle_debug_hold_toggle(key: KeyEvent, state: &mut DashboardState) -> Option<AppEffect> {
    if !is_debug_hold_key(key) {
        return None;
    }

    let now = Instant::now();
    if let Some(last_seen) = state.debug_hold_last_seen_at
        && now.duration_since(last_seen) > DEBUG_HOLD_GAP_RESET
    {
        state.debug_hold_started_at = Some(now);
    }

    if state.debug_hold_started_at.is_none() {
        state.debug_hold_started_at = Some(now);
    }
    state.debug_hold_last_seen_at = Some(now);

    if let Some(started_at) = state.debug_hold_started_at
        && now.duration_since(started_at) >= DEBUG_HOLD_DURATION
    {
        state.debug_hold_started_at = None;
        state.debug_hold_last_seen_at = None;

        return Some(Box::new(|app| app.toggle_debug_panel()));
    }

    None
}

fn is_debug_hold_key(key: KeyEvent) -> bool {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return false;
    }

    matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D'))
}

fn handle_paste(_app: &AppState, text: &str, state: &mut DashboardState) -> Option<AppEffect> {
    if let Some(modal) = &mut state.host_modal {
        insert_pasted_text(&mut modal.form, text);
        return None;
    }

    if let Some(switcher) = &mut state.quick_switcher {
        switcher.query.push_str(text);
        switcher.selected_idx = 0;
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
        let statuses = task.join.join().unwrap_or_default();
        state.host_statuses.insert(task.host_id, statuses);
    }
}

fn sync_host_status_maps(app: &AppState, state: &mut DashboardState) {
    let host_ids = app.db.hosts.iter().map(|h| h.id).collect::<Vec<_>>();
    state.host_statuses.retain(|id, _| host_ids.contains(id));

    for host in &app.db.hosts {
        let expected_len = host.endpoints.len();
        let entry = state
            .host_statuses
            .entry(host.id)
            .or_insert_with(|| vec![HostConnectionStatus::Unknown; expected_len]);
        if entry.len() != expected_len {
            *entry = vec![HostConnectionStatus::Unknown; expected_len];
        }
    }
}

fn start_probe_round(app: &AppState, state: &mut DashboardState) {
    let timeout = Duration::from_secs(app.config.ssh_connect_timeout_seconds.max(1));

    for host in &app.db.hosts {
        if state.probe_tasks.iter().any(|task| task.host_id == host.id) {
            continue;
        }

        let host_id = host.id;
        let endpoints = host.endpoints.clone();

        let join = thread::spawn(move || {
            endpoints
                .iter()
                .map(|e| {
                    if host_is_reachable(&e.host, e.port, timeout) {
                        HostConnectionStatus::Reachable
                    } else {
                        HostConnectionStatus::Unreachable
                    }
                })
                .collect::<Vec<_>>()
        });
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
    if let Some(picker) = &mut modal.key_picker {
        match key.code {
            KeyCode::Esc => {
                modal.key_picker = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                picker.selected = picker.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = picker.options.len().saturating_sub(1);
                picker.selected = (picker.selected + 1).min(max);
            }
            KeyCode::Enter => {
                if let Some(path) = picker.options.get(picker.selected) {
                    modal.form.key_path = path.clone();
                    modal.form.caret = modal.form.key_path.len();
                }
                modal.key_picker = None;
            }
            _ => {}
        }
        return None;
    }

    if key.code == KeyCode::Esc {
        modal.form.error = None;
        return Some(Box::new(move |app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.host_modal = None;
            }
        }));
    }

    if key.code == KeyCode::Tab || key.code == KeyCode::Down {
        modal.form.focus = modal.form.focus.next();
        modal.form.caret = current_field_value(&modal.form).len();
        modal.form.error = None;
        return None;
    }

    if key.code == KeyCode::BackTab || key.code == KeyCode::Up {
        modal.form.focus = modal.form.focus.prev();
        modal.form.caret = current_field_value(&modal.form).len();
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
            if key.code == KeyCode::Char(' ') {
                modal.form.key_input_mode = match modal.form.key_input_mode {
                    HostKeyInputMode::Path => HostKeyInputMode::Inline,
                    HostKeyInputMode::Inline => HostKeyInputMode::Path,
                };
            } else {
                modal.form.auth_mode = match modal.form.auth_mode {
                    HostAuthMode::Key => HostAuthMode::Password,
                    HostAuthMode::Password => HostAuthMode::Key,
                };
            }
            modal.form.error = None;
            return None;
        }
    }

    if key.code == KeyCode::Char('f')
        && modal.form.focus == HostFormField::AuthValue
        && modal.form.auth_mode == HostAuthMode::Key
        && modal.form.key_input_mode == HostKeyInputMode::Path
    {
        modal.key_picker = Some(HostKeyPickerState {
            options: discover_key_files(),
            selected: 0,
        });
        return None;
    }

    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return save_modal(app, selected_host, modal);
    }

    if key.code == KeyCode::Enter {
        if modal.form.focus == HostFormField::AuthValue {
            return save_modal(app, selected_host, modal);
        }
        modal.form.focus = modal.form.focus.next();
        modal.form.caret = current_field_value(&modal.form).len();
        return None;
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
    let (name, user, endpoints, auth) = match validation {
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
                    user,
                    endpoints,
                    auth,
                });
                if let Screen::Dashboard { state } = &mut app.screen {
                    state.selected_host = create_selected_index;
                }
            }
            HostModalMode::Edit { host_id } => {
                if let Some(existing) = app.db.hosts.iter_mut().find(|h| h.id == host_id) {
                    existing.name = name;
                    existing.user = user;
                    existing.endpoints = endpoints;
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
        HostFormField::User => Some(&mut form.user),
        HostFormField::Endpoints => Some(&mut form.endpoints),
        HostFormField::AuthValue => match form.auth_mode {
            HostAuthMode::Key => match form.key_input_mode {
                HostKeyInputMode::Path => Some(&mut form.key_path),
                HostKeyInputMode::Inline => Some(&mut form.key_inline),
            },
            HostAuthMode::Password => Some(&mut form.password),
        },
        HostFormField::AuthMode => None,
    };

    let Some(field) = target else {
        return;
    };

    if form.caret > field.len() {
        form.caret = field.len();
    }

    match key.code {
        KeyCode::Char(c) => {
            field.insert(form.caret, c);
            form.caret += c.len_utf8();
        }
        KeyCode::Backspace => {
            if form.caret > 0 {
                let mut idx = form.caret - 1;
                while !field.is_char_boundary(idx) {
                    idx = idx.saturating_sub(1);
                }
                field.remove(idx);
                form.caret = idx;
            }
        }
        KeyCode::Delete => {
            if form.caret < field.len() {
                let idx = form.caret;
                field.remove(idx);
            }
        }
        KeyCode::Left => {
            if form.caret > 0 {
                let mut idx = form.caret - 1;
                while !field.is_char_boundary(idx) {
                    idx = idx.saturating_sub(1);
                }
                form.caret = idx;
            }
        }
        KeyCode::Right => {
            if form.caret < field.len() {
                let mut idx = form.caret + 1;
                while idx < field.len() && !field.is_char_boundary(idx) {
                    idx += 1;
                }
                form.caret = idx;
            }
        }
        KeyCode::Home => {
            form.caret = 0;
        }
        KeyCode::End => {
            form.caret = field.len();
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
        HostFormField::User => Some(&mut form.user),
        HostFormField::Endpoints => Some(&mut form.endpoints),
        HostFormField::AuthValue => match form.auth_mode {
            HostAuthMode::Key => match form.key_input_mode {
                HostKeyInputMode::Path => Some(&mut form.key_path),
                HostKeyInputMode::Inline => Some(&mut form.key_inline),
            },
            HostAuthMode::Password => Some(&mut form.password),
        },
        HostFormField::AuthMode => None,
    };

    if let Some(field) = target {
        if form.caret > field.len() {
            form.caret = field.len();
        }
        field.insert_str(form.caret, text);
        form.caret += text.len();
    }
}

fn validate_form(
    form: &HostFormState,
) -> Result<(String, String, Vec<SshEndpoint>, HostAuth), String> {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err("Name is required".to_string());
    }

    let endpoints = parse_endpoints(&form.endpoints)?;
    if endpoints.is_empty() {
        return Err("At least one endpoint is required".to_string());
    }

    let user = form.user.trim().to_string();
    if user.is_empty() {
        return Err("User is required".to_string());
    }

    let auth = match form.auth_mode {
        HostAuthMode::Key => match form.key_input_mode {
            HostKeyInputMode::Path => {
                let key_path = form.key_path.trim().to_string();
                if key_path.is_empty() {
                    return Err("Key path is required".to_string());
                }
                HostAuth::KeyPath { key_path }
            }
            HostKeyInputMode::Inline => {
                let private_key = form.key_inline.trim().to_string();
                if private_key.is_empty() {
                    return Err("Inline private key is required".to_string());
                }
                HostAuth::KeyInline { private_key }
            }
        },
        HostAuthMode::Password => {
            let password = form.password.trim().to_string();
            if password.is_empty() {
                return Err("Password is required".to_string());
            }
            HostAuth::Password { password }
        }
    };

    Ok((name, user, endpoints, auth))
}

fn parse_endpoints(value: &str) -> Result<Vec<SshEndpoint>, String> {
    let mut endpoints = Vec::new();
    for raw in value.lines().flat_map(|line| line.split(',')) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((host, port)) = trimmed.rsplit_once(':') else {
            return Err(format!("Endpoint '{trimmed}' must be host:port"));
        };

        let host = host.trim();
        if host.is_empty() {
            return Err(format!("Endpoint '{trimmed}' has an empty host"));
        }

        let port = port
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("Endpoint '{trimmed}' has invalid port"))?;
        if port == 0 {
            return Err(format!("Endpoint '{trimmed}' has invalid port"));
        }

        endpoints.push(SshEndpoint {
            host: host.to_string(),
            port,
        });
    }
    Ok(endpoints)
}

fn current_field_value(form: &HostFormState) -> String {
    match form.focus {
        HostFormField::Name => form.name.clone(),
        HostFormField::User => form.user.clone(),
        HostFormField::Endpoints => form.endpoints.clone(),
        HostFormField::AuthMode => String::new(),
        HostFormField::AuthValue => match form.auth_mode {
            HostAuthMode::Key => match form.key_input_mode {
                HostKeyInputMode::Path => form.key_path.clone(),
                HostKeyInputMode::Inline => form.key_inline.clone(),
            },
            HostAuthMode::Password => form.password.clone(),
        },
    }
}

fn discover_key_files() -> Vec<String> {
    let mut options = Vec::new();
    let home = std::env::var("HOME").ok();
    let Some(home) = home else {
        return options;
    };

    let ssh_dir = std::path::Path::new(&home).join(".ssh");
    let Ok(entries) = std::fs::read_dir(ssh_dir) else {
        return options;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (name.ends_with(".pub") || name == "known_hosts" || name == "config")
        {
            continue;
        }
        if let Some(text) = path.to_str() {
            options.push(text.to_string());
        }
    }

    options.sort();
    options
}

#[derive(Clone, Copy)]
enum QuickSwitchTarget {
    Page(DashboardPage),
    Session(usize),
}

struct QuickSwitchItem {
    number: usize,
    label: String,
    target: QuickSwitchTarget,
}

fn build_quick_switch_items(
    state: &DashboardState,
    show_debug_panel: bool,
) -> Vec<QuickSwitchItem> {
    let mut items = Vec::new();
    let mut number = 1;

    items.push(QuickSwitchItem {
        number,
        label: "Home".to_string(),
        target: QuickSwitchTarget::Page(DashboardPage::Home),
    });
    number += 1;

    for (idx, tab) in state.ssh_tabs.iter().enumerate() {
        items.push(QuickSwitchItem {
            number,
            label: tab.title.clone(),
            target: QuickSwitchTarget::Session(idx),
        });
        number += 1;
    }

    items.push(QuickSwitchItem {
        number,
        label: "Settings".to_string(),
        target: QuickSwitchTarget::Page(DashboardPage::Settings),
    });
    number += 1;
    if show_debug_panel {
        items.push(QuickSwitchItem {
            number,
            label: "Debug".to_string(),
            target: QuickSwitchTarget::Page(DashboardPage::Debug),
        });
    }

    items
}

fn filtered_quick_switch_indices(state: &DashboardState, items: &[QuickSwitchItem]) -> Vec<usize> {
    let query = state
        .quick_switcher
        .as_ref()
        .map(|s| s.query.trim().to_ascii_lowercase())
        .unwrap_or_default();

    if query.is_empty() {
        return (0..items.len()).collect();
    }

    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let label_match = item.label.to_ascii_lowercase().contains(&query);
            let number_match = item.number.to_string().contains(&query);
            label_match || number_match
        })
        .map(|(idx, _)| idx)
        .collect()
}

fn activate_quick_switch_target(state: &mut DashboardState, target: QuickSwitchTarget) {
    match target {
        QuickSwitchTarget::Page(page) => {
            state.active_page = page;
        }
        QuickSwitchTarget::Session(idx) => {
            if idx < state.ssh_tabs.len() {
                state.active_ssh_tab = Some(idx);
                state.active_page = DashboardPage::Ssh;
            }
        }
    }

    state.quick_switcher = None;
}

fn handle_quick_switcher_key(
    key: KeyEvent,
    state: &mut DashboardState,
    show_debug_panel: bool,
) -> Option<AppEffect> {
    let items = build_quick_switch_items(state, show_debug_panel);

    let filtered_indices = filtered_quick_switch_indices(state, &items);
    let selected_idx = state
        .quick_switcher
        .as_ref()
        .map(|s| s.selected_idx)
        .unwrap_or(0)
        .min(filtered_indices.len().saturating_sub(1));

    match key.code {
        KeyCode::Esc => {
            state.quick_switcher = None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(switcher) = &mut state.quick_switcher {
                switcher.selected_idx = switcher.selected_idx.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(switcher) = &mut state.quick_switcher {
                let max = filtered_indices.len().saturating_sub(1);
                switcher.selected_idx = (switcher.selected_idx + 1).min(max);
            }
        }
        KeyCode::Backspace => {
            if let Some(switcher) = &mut state.quick_switcher {
                switcher.query.pop();
                switcher.selected_idx = 0;
            }
        }
        KeyCode::Enter => {
            if let Some(item_idx) = filtered_indices.get(selected_idx)
                && let Some(item) = items.get(*item_idx)
            {
                activate_quick_switch_target(state, item.target);
            }
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            if let Some(switcher) = &mut state.quick_switcher {
                switcher.query.push(c);
                switcher.selected_idx = 0;
            }
        }
        _ => {}
    }

    None
}

fn ui(frame: &mut Frame, app: &AppState, state: &DashboardState) {
    let a = frame.area();
    let footer = keybind_hint(state, app, a);
    let (inner, area) = full_rect(a, "Stassh", footer);
    frame.render_widget(inner, a);
    let content_block = frame_block();
    let content_area = content_block.inner(area);
    frame.render_widget(content_block, area);

    match state.active_page {
        DashboardPage::Home => pages::home::render(frame, content_area, app, state),
        DashboardPage::Settings => pages::settings::render(frame, content_area, app),
        DashboardPage::Debug => {
            if app.config.show_debug_panel {
                pages::debug::render(frame, content_area, app, state)
            } else {
                pages::home::render(frame, content_area, app, state)
            }
        }
        DashboardPage::Ssh => pages::ssh::render(frame, a, content_area, state),
    }

    if let Some(modal) = &state.host_modal {
        render_host_modal(frame, a, modal);
    }

    if state.quick_switcher.is_some() {
        render_quick_switcher_modal(frame, a, state, app.config.show_debug_panel);
    }
}

fn render_quick_switcher_modal(
    frame: &mut Frame,
    app_area: Rect,
    state: &DashboardState,
    show_debug_panel: bool,
) {
    let width = (app_area.width.saturating_sub(8)).min(90);
    let height = 18;
    let popup_area = centered_rect_no_border(width, height, app_area);

    let items = build_quick_switch_items(state, show_debug_panel);
    let filtered_indices = filtered_quick_switch_indices(state, &items);
    let selected = state
        .quick_switcher
        .as_ref()
        .map(|s| s.selected_idx)
        .unwrap_or(0)
        .min(filtered_indices.len().saturating_sub(1));
    let query = state
        .quick_switcher
        .as_ref()
        .map(|s| s.query.as_str())
        .unwrap_or("");

    frame.render_widget(Clear, popup_area);
    let block = modal_block(
        "Quick Switcher",
        "Type to search | Up/Down select | Enter open | Esc close",
    );
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("search: ", muted_text()),
            Span::styled(query, accent_text()),
        ]))
        .style(text()),
        sections[0],
    );

    let visible_count = sections[1].height.saturating_sub(1) as usize;
    let start = selected.saturating_sub(visible_count.saturating_sub(1));
    let mut lines = Vec::new();

    if filtered_indices.is_empty() {
        lines.push("  no matches".to_string());
    } else {
        for (display_idx, item_idx) in filtered_indices
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_count)
        {
            if let Some(item) = items.get(*item_idx) {
                let prefix = if display_idx == selected { ">" } else { " " };
                lines.push(format!("{prefix} {:>2}. {}", item.number, item.label));
            }
        }
    }

    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .alignment(Alignment::Left)
            .style(text()),
        sections[1],
    );
}

fn render_host_modal(frame: &mut Frame, app_area: Rect, modal: &HostModalState) {
    let width = (app_area.width.saturating_sub(4)).min(100);
    let height = 24;
    let popup_area = centered_rect_no_border(width, height, app_area);

    frame.render_widget(Clear, popup_area);
    let block = modal_block(
        match modal.mode {
            HostModalMode::Create => "Create Host",
            HostModalMode::Edit { .. } => "Edit Host",
        },
        "Tab move | Enter next/save | Ctrl+S save | F file picker | Esc cancel",
    );

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(1),
        ])
        .split(inner);

    render_input_field(
        frame,
        chunks[0],
        "Name",
        &modal.form.name,
        modal.form.focus == HostFormField::Name,
        modal.form.caret,
        false,
    );
    render_input_field(
        frame,
        chunks[1],
        "User",
        &modal.form.user,
        modal.form.focus == HostFormField::User,
        modal.form.caret,
        false,
    );
    render_input_field(
        frame,
        chunks[2],
        "Endpoints (host:port, one per line)",
        &modal.form.endpoints,
        modal.form.focus == HostFormField::Endpoints,
        modal.form.caret,
        false,
    );

    let auth_text = if modal.form.auth_mode == HostAuthMode::Key {
        format!(
            "key ({})",
            if modal.form.key_input_mode == HostKeyInputMode::Path {
                "path"
            } else {
                "inline"
            }
        )
    } else {
        "password".to_string()
    };
    render_input_field(
        frame,
        chunks[3],
        "Auth mode (Left/Right toggle, Space switches key source)",
        &auth_text,
        modal.form.focus == HostFormField::AuthMode,
        modal.form.caret,
        false,
    );

    let auth_value_label = if modal.form.auth_mode == HostAuthMode::Key {
        if modal.form.key_input_mode == HostKeyInputMode::Path {
            "Key path (press F to browse ~/.ssh)"
        } else {
            "Private key content (paste supported)"
        }
    } else {
        "Password"
    };
    let auth_value = if modal.form.auth_mode == HostAuthMode::Key {
        if modal.form.key_input_mode == HostKeyInputMode::Path {
            modal.form.key_path.clone()
        } else {
            modal.form.key_inline.clone()
        }
    } else {
        mask(&modal.form.password)
    };

    render_input_field(
        frame,
        chunks[4],
        auth_value_label,
        &auth_value,
        modal.form.focus == HostFormField::AuthValue,
        modal.form.caret,
        modal.form.auth_mode == HostAuthMode::Password,
    );

    if let Some(error_text) = &modal.form.error {
        frame.render_widget(
            Paragraph::new(error_text.as_str()).style(danger_text()),
            chunks[5],
        );
    }

    if let Some(picker) = &modal.key_picker {
        render_key_picker(frame, popup_area, picker);
    }
}

fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "*".repeat(value.len())
    }
}

fn render_input_field(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    selected: bool,
    caret: usize,
    secret: bool,
) {
    let block = Block::default()
        .title(if selected {
            Span::styled(format!(" {label} "), accent_text())
        } else {
            Span::styled(format!(" {label} "), muted_text())
        })
        .borders(Borders::ALL)
        .border_style(if selected {
            selected_border()
        } else {
            border()
        });

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text_value = if secret {
        mask(value)
    } else {
        value.to_string()
    };

    let line = if selected {
        line_with_caret_value(&text_value, caret)
    } else {
        Line::from(text_value)
    };
    frame.render_widget(Paragraph::new(line).style(text()), inner);
}

fn line_with_caret_value(text_value: &str, caret: usize) -> Line<'static> {
    let safe_caret = caret.min(text_value.len());
    let before = text_value[..safe_caret].to_string();
    let current = text_value[safe_caret..].chars().next().unwrap_or(' ');
    let after = if safe_caret < text_value.len() {
        text_value[safe_caret + current.len_utf8()..].to_string()
    } else {
        String::new()
    };
    Line::from(vec![
        Span::raw(before),
        Span::styled(
            current.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ),
        Span::raw(after),
    ])
}

fn render_key_picker(frame: &mut Frame, host_popup: Rect, picker: &HostKeyPickerState) {
    let width = (host_popup.width.saturating_sub(8)).min(90);
    let height = 12;
    let area = centered_rect_no_border(width, height, host_popup);
    frame.render_widget(Clear, area);
    let block = modal_block("Select key file", "Up/Down move | Enter choose | Esc close");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if picker.options.is_empty() {
        lines.push("  no files found in ~/.ssh".to_string());
    } else {
        for (idx, path) in picker
            .options
            .iter()
            .enumerate()
            .take(inner.height as usize)
        {
            let marker = if idx == picker.selected { ">" } else { " " };
            lines.push(format!("{marker} {path}"));
        }
    }

    frame.render_widget(Paragraph::new(lines.join("\n")).style(text()), inner);
}

fn keybind_hint(state: &DashboardState, app: &AppState, area: Rect) -> &'static str {
    if state.host_modal.is_some() {
        return "HOST form: Tab move | Enter next/save | Ctrl+S save | paste/drag text | Esc cancel";
    }

    if state.quick_switcher.is_some() {
        return "SWITCHER: type to filter | Up/Down select | Enter open | Esc close";
    }

    match state.active_page {
        DashboardPage::Home => pages::home::footer_hint(),
        DashboardPage::Settings => "Ctrl+Q quick switch | Esc exit",
        DashboardPage::Debug => pages::debug::footer_hint(pages::debug::has_scrollbar(app, area)),
        DashboardPage::Ssh => pages::ssh::footer_hint(),
    }
}
