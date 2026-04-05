use std::{
    net::{TcpStream, ToSocketAddrs},
    thread,
    time::{Duration, Instant},
};

use backend::{AppState, HostAuth, SshHost};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostAuthMode, HostConnectionStatus, HostFormField,
        HostFormState, HostModalMode, HostModalState, HostProbeTask, Screen,
    },
    screens::{AppEffect, ScreenHandler},
    ui::{
        accent_text, centered_rect_no_border, frame_block, full_rect, modal_block, muted_text, text,
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

    if key.code == KeyCode::Tab || key.code == KeyCode::Down {
        modal.form.focus = modal.form.focus.next();
        modal.form.error = None;
        return None;
    }

    if key.code == KeyCode::BackTab || key.code == KeyCode::Up {
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
    let width = (app_area.width.saturating_sub(4)).min(80);
    let height = 16;
    let popup_area = centered_rect_no_border(width, height, app_area);

    frame.render_widget(Clear, popup_area);
    let block = modal_block(
        match modal.mode {
            HostModalMode::Create => "Create Host",
            HostModalMode::Edit { .. } => "Edit Host",
        },
        "Ctrl+S save | Esc cancel | Tab/Shift+Tab next/prev",
    );

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

    let content = Paragraph::new(lines.join("\n"))
        .alignment(Alignment::Left)
        .style(text());
    frame.render_widget(content, inner);
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

fn keybind_hint(state: &DashboardState, app: &AppState, area: Rect) -> &'static str {
    if state.host_modal.is_some() {
        return "HOST form: Tab/Shift+Tab move field | Ctrl+S save | Esc cancel/exit";
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
