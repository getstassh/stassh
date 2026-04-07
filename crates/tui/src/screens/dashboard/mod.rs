use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
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
        HostFormState, HostKeyInputMode, HostKeyPickerEntry, HostKeyPickerState, HostModalMode,
        HostModalState, HostProbeTask, Screen,
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
        DashboardPage::Settings => pages::settings::handle_key(app, key, state),
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
        if let Some(picker) = &mut modal.key_picker {
            picker.command_input.push_str(text);
            picker.history_index = None;
            reset_picker_completion(picker);
            picker.error = None;
            picker.status = None;
            return None;
        }
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
    } else if state.active_page == DashboardPage::Settings {
        pages::settings::handle_paste(text, state);
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
        let form = &mut modal.form;
        match key.code {
            KeyCode::Esc => {
                modal.key_picker = None;
            }
            KeyCode::Up => {
                if picker.command_input.trim().is_empty() {
                    picker.selected = picker.selected.saturating_sub(1);
                    ensure_picker_selection_visible(picker);
                } else {
                    picker_history_up(picker);
                }
            }
            KeyCode::Down => {
                if picker.command_input.trim().is_empty() {
                    let max = picker.entries.len().saturating_sub(1);
                    picker.selected = (picker.selected + 1).min(max);
                    ensure_picker_selection_visible(picker);
                } else {
                    picker_history_down(picker);
                }
            }
            KeyCode::Backspace => {
                if !picker.command_input.is_empty() {
                    picker.command_input.pop();
                    picker.history_index = None;
                    reset_picker_completion(picker);
                    picker.error = None;
                    picker.status = None;
                }
            }
            KeyCode::Left => {
                if picker.command_input.trim().is_empty()
                    && let Some(parent) = parent_dir_str(&picker.current_dir)
                {
                    move_picker_to_dir(picker, &parent);
                }
            }
            KeyCode::Right => {
                if picker.command_input.trim().is_empty()
                    && let Some(entry) = picker.entries.get(picker.selected)
                    && entry.is_dir
                {
                    let path = entry.path.clone();
                    move_picker_to_dir(picker, &path);
                }
            }
            KeyCode::Tab => {
                apply_picker_tab_completion(picker);
            }
            KeyCode::Char(c)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                picker.command_input.push(c);
                picker.history_index = None;
                reset_picker_completion(picker);
                picker.error = None;
                picker.status = None;
            }
            KeyCode::Enter => {
                if !picker.command_input.trim().is_empty() {
                    if execute_picker_command(form, picker) {
                        modal.key_picker = None;
                    }
                    return None;
                }

                if let Some(entry) = picker.entries.get(picker.selected).cloned() {
                    if entry.is_dir {
                        move_picker_to_dir(picker, &entry.path);
                        return None;
                    }

                    if let Err(err) = apply_picker_file_selection(form, picker, &entry.path) {
                        form.error = Some(err);
                        return None;
                    }
                    modal.key_picker = None;
                }
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
        if key.code == KeyCode::Left || key.code == KeyCode::Right {
            cycle_auth_mode(&mut modal.form, key.code == KeyCode::Left);
            modal.form.error = None;
            return None;
        }
    }

    if modal.form.focus == HostFormField::AuthValue
        && modal.form.auth_mode == HostAuthMode::Key
        && modal.form.key_input_mode == HostKeyInputMode::Path
        && (key.code == KeyCode::Enter || key.code == KeyCode::Right)
    {
        modal.form.error = None;
        modal.key_picker = Some(build_key_picker(&modal.form));
        return None;
    }

    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthSelection {
    KeyPath,
    KeyInline,
    Password,
}

fn auth_selection(form: &HostFormState) -> AuthSelection {
    match (form.auth_mode, form.key_input_mode) {
        (HostAuthMode::Password, _) => AuthSelection::Password,
        (HostAuthMode::Key, HostKeyInputMode::Path) => AuthSelection::KeyPath,
        (HostAuthMode::Key, HostKeyInputMode::Inline) => AuthSelection::KeyInline,
    }
}

fn set_auth_selection(form: &mut HostFormState, selection: AuthSelection) {
    match selection {
        AuthSelection::KeyPath => {
            form.auth_mode = HostAuthMode::Key;
            form.key_input_mode = HostKeyInputMode::Path;
        }
        AuthSelection::KeyInline => {
            form.auth_mode = HostAuthMode::Key;
            form.key_input_mode = HostKeyInputMode::Inline;
        }
        AuthSelection::Password => {
            form.auth_mode = HostAuthMode::Password;
        }
    }
}

fn cycle_auth_mode(form: &mut HostFormState, reverse: bool) {
    let modes = [
        AuthSelection::KeyPath,
        AuthSelection::KeyInline,
        AuthSelection::Password,
    ];
    let current = auth_selection(form);
    let current_idx = modes.iter().position(|m| *m == current).unwrap_or(0);

    let next_idx = if reverse {
        current_idx.checked_sub(1).unwrap_or(modes.len() - 1)
    } else {
        (current_idx + 1) % modes.len()
    };
    set_auth_selection(form, modes[next_idx]);
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
                HostKeyInputMode::Path => None,
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
                HostKeyInputMode::Path => None,
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

fn build_key_picker(form: &HostFormState) -> HostKeyPickerState {
    let target_mode = form.key_input_mode;
    let start_dir = starting_picker_dir(form, target_mode);
    let (entries, error) = match read_picker_entries(&start_dir) {
        Ok(entries) => (entries, None),
        Err(err) => (Vec::new(), Some(err)),
    };

    HostKeyPickerState {
        target_mode,
        current_dir: start_dir.to_string_lossy().to_string(),
        entries,
        selected: 0,
        scroll: 0,
        command_input: String::new(),
        completion_prefix: String::new(),
        completion_matches: Vec::new(),
        completion_index: 0,
        command_history: Vec::new(),
        history_index: None,
        status: None,
        error,
    }
}

fn resolve_typed_picker_path(current_dir: &str, typed_path: &str) -> Result<PathBuf, String> {
    let trimmed = typed_path.trim();
    if trimmed.is_empty() {
        return Err("Path input is empty".to_string());
    }

    let absolute = if let Some(stripped) = trimmed.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            home.join(stripped)
        } else {
            PathBuf::from(trimmed)
        }
    } else if trimmed == "~" {
        home_dir().unwrap_or_else(|| PathBuf::from("/"))
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        Path::new(current_dir).join(trimmed)
    };

    if !absolute.exists() {
        return Err(format!("Path does not exist: {}", absolute.display()));
    }

    Ok(absolute)
}

fn ensure_picker_selection_visible(picker: &mut HostKeyPickerState) {
    if picker.selected < picker.scroll {
        picker.scroll = picker.selected;
    }
}

fn move_picker_to_dir(picker: &mut HostKeyPickerState, dir: &str) {
    let path = Path::new(dir);
    match read_picker_entries(path) {
        Ok(entries) => {
            picker.current_dir = path.to_string_lossy().to_string();
            picker.entries = entries;
            picker.selected = 0;
            picker.scroll = 0;
            picker.error = None;
            picker.status = None;
        }
        Err(err) => {
            picker.error = Some(err);
        }
    }
}

fn parent_dir_str(dir: &str) -> Option<String> {
    Path::new(dir)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn starting_picker_dir(form: &HostFormState, mode: HostKeyInputMode) -> PathBuf {
    if mode == HostKeyInputMode::Path {
        let trimmed = form.key_path.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_dir() {
                return path;
            }
            if path.is_file() {
                return path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("/"));
            }
            if let Some(parent) = path.parent()
                && parent.exists()
            {
                return parent.to_path_buf();
            }
        }
    }

    let home = home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let ssh = home.join(".ssh");
    if ssh.is_dir() { ssh } else { home }
}

fn read_picker_entries(dir: &Path) -> Result<Vec<HostKeyPickerEntry>, String> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    let entries = fs::read_dir(dir).map_err(|e| format!("Cannot open {}: {e}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if path.is_dir() {
            dirs.push(HostKeyPickerEntry {
                label: format!("{name}/"),
                path: path.to_string_lossy().to_string(),
                is_dir: true,
            });
            continue;
        }

        if path.is_file() {
            files.push(HostKeyPickerEntry {
                label: name.to_string(),
                path: path.to_string_lossy().to_string(),
                is_dir: false,
            });
        }
    }

    dirs.sort_by(|a, b| {
        a.label
            .to_ascii_lowercase()
            .cmp(&b.label.to_ascii_lowercase())
    });
    files.sort_by(|a, b| {
        a.label
            .to_ascii_lowercase()
            .cmp(&b.label.to_ascii_lowercase())
    });

    let mut merged = Vec::new();
    if let Some(parent) = dir.parent() {
        merged.push(HostKeyPickerEntry {
            label: "../".to_string(),
            path: parent.to_string_lossy().to_string(),
            is_dir: true,
        });
    }
    merged.extend(dirs);
    merged.extend(files);

    Ok(merged)
}

fn load_key_file_text(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Cannot read key file {path}: {e}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("Cannot import {path}: file is not valid UTF-8 text"))?;
    Ok(text)
}

fn apply_picker_file_selection(
    form: &mut HostFormState,
    picker: &mut HostKeyPickerState,
    path: &str,
) -> Result<(), String> {
    match picker.target_mode {
        HostKeyInputMode::Path => {
            form.key_path = path.to_string();
            form.caret = form.key_path.len();
            form.error = None;
            Ok(())
        }
        HostKeyInputMode::Inline => {
            let private_key = load_key_file_text(path)?;
            form.key_inline = private_key;
            form.caret = form.key_inline.len();
            form.error = None;
            Ok(())
        }
    }
}

fn reset_picker_completion(picker: &mut HostKeyPickerState) {
    picker.completion_prefix.clear();
    picker.completion_matches.clear();
    picker.completion_index = 0;
}

fn picker_history_up(picker: &mut HostKeyPickerState) {
    if picker.command_history.is_empty() {
        return;
    }

    let next_index = match picker.history_index {
        Some(idx) if idx > 0 => idx - 1,
        Some(idx) => idx,
        None => picker.command_history.len().saturating_sub(1),
    };
    picker.history_index = Some(next_index);
    if let Some(cmd) = picker.command_history.get(next_index) {
        picker.command_input = cmd.clone();
        reset_picker_completion(picker);
    }
}

fn picker_history_down(picker: &mut HostKeyPickerState) {
    let Some(current) = picker.history_index else {
        return;
    };

    if current + 1 >= picker.command_history.len() {
        picker.history_index = None;
        picker.command_input.clear();
        reset_picker_completion(picker);
        return;
    }

    let next = current + 1;
    picker.history_index = Some(next);
    if let Some(cmd) = picker.command_history.get(next) {
        picker.command_input = cmd.clone();
        reset_picker_completion(picker);
    }
}

fn execute_picker_command(form: &mut HostFormState, picker: &mut HostKeyPickerState) -> bool {
    let command = picker.command_input.trim().to_string();
    if command.is_empty() {
        return false;
    }

    picker.command_history.push(command.clone());
    picker.history_index = None;
    reset_picker_completion(picker);
    picker.error = None;
    picker.status = None;

    let mut parts = command.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or_default();
    let arg = parts.next().unwrap_or_default().trim();

    match cmd {
        "cd" => {
            if arg.is_empty() {
                picker.error = Some("Usage: cd <path>".to_string());
                return false;
            }

            match resolve_typed_picker_path(&picker.current_dir, arg) {
                Ok(path) if path.is_dir() => {
                    let next = path.to_string_lossy().to_string();
                    move_picker_to_dir(picker, &next);
                    picker.status = Some(format!("cd {}", picker.current_dir));
                    picker.command_input.clear();
                }
                Ok(path) => {
                    picker.error = Some(format!("Not a directory: {}", path.display()));
                }
                Err(err) => {
                    picker.error = Some(err);
                }
            }
            false
        }
        "select" => {
            if arg.is_empty() {
                picker.error = Some("Usage: select <path|name>".to_string());
                return false;
            }

            let target = resolve_select_target_path(picker, arg);
            match target {
                Ok(path) if path.is_file() => {
                    let chosen = path.to_string_lossy().to_string();
                    match apply_picker_file_selection(form, picker, &chosen) {
                        Ok(()) => {
                            picker.status = Some(format!("selected {chosen}"));
                            picker.command_input.clear();
                            true
                        }
                        Err(err) => {
                            picker.error = Some(err);
                            false
                        }
                    }
                }
                Ok(path) if path.is_dir() => {
                    picker.error = Some(format!(
                        "{} is a directory; use ls {}",
                        path.display(),
                        path.display()
                    ));
                    false
                }
                Ok(path) => {
                    picker.error = Some(format!("Cannot select {}", path.display()));
                    false
                }
                Err(err) => {
                    picker.error = Some(err);
                    false
                }
            }
        }
        _ => {
            picker.error = Some(format!("Unknown command: {cmd}. Try cd or select"));
            false
        }
    }
}

fn resolve_select_target_path(picker: &HostKeyPickerState, arg: &str) -> Result<PathBuf, String> {
    if let Ok(index) = arg.parse::<usize>()
        && index > 0
        && let Some(entry) = picker.entries.get(index - 1)
    {
        return Ok(PathBuf::from(&entry.path));
    }

    if let Some(entry) = picker.entries.iter().find(|entry| {
        entry.label == arg
            || entry.label.trim_end_matches('/') == arg
            || Path::new(&entry.path)
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == arg)
    }) {
        return Ok(PathBuf::from(&entry.path));
    }

    resolve_typed_picker_path(&picker.current_dir, arg)
}

fn apply_picker_tab_completion(picker: &mut HostKeyPickerState) {
    let input = picker.command_input.clone();
    let trimmed = input.trim_start();

    if !trimmed.contains(' ') {
        let commands = ["cd", "select"];
        let token = trimmed;
        let matches = commands
            .iter()
            .copied()
            .filter(|cmd| cmd.starts_with(token))
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        if matches.is_empty() {
            picker.error = Some("No completion matches".to_string());
            picker.status = None;
            return;
        }

        if matches.len() == 1 {
            picker.command_input = format!("{} ", matches[0]);
            picker.error = None;
            picker.status = None;
            return;
        }

        let common = longest_common_prefix(&matches);
        if common.len() > token.len() {
            picker.command_input = common;
            picker.error = None;
            picker.status = None;
            return;
        }

        picker.error = None;
        picker.status = Some(format!("matches: {}", matches.join("  ")));
        return;
    } else {
        let mut split = trimmed.splitn(2, char::is_whitespace);
        let cmd = split.next().unwrap_or_default();
        let arg_raw = split.next().unwrap_or_default().trim_start();
        if !matches!(cmd, "cd" | "select") {
            picker.error = Some("Tab completion supports cd/select paths".to_string());
            picker.status = None;
            return;
        }

        let matches = complete_path_candidates(&picker.current_dir, arg_raw);

        if matches.is_empty() {
            picker.error = Some("No completion matches".to_string());
            picker.status = None;
            return;
        }

        if matches.len() == 1 {
            picker.command_input = format!("{cmd} {}", matches[0]);
            picker.error = None;
            picker.status = None;
            return;
        }

        let common = longest_common_prefix(&matches);
        if common.len() > arg_raw.len() {
            picker.command_input = format!("{cmd} {common}");
            picker.error = None;
            picker.status = None;
            return;
        }

        picker.error = None;
        picker.status = Some(format!("matches: {}", matches.join("  ")));
        return;
    }
}

fn longest_common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for value in values.iter().skip(1) {
        let mut end = 0;
        for (a, b) in prefix.chars().zip(value.chars()) {
            if a != b {
                break;
            }
            end += a.len_utf8();
        }
        prefix.truncate(end);
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

fn complete_path_candidates(current_dir: &str, arg: &str) -> Vec<String> {
    let partial = arg.trim();

    let (base_dir, fragment, prefix_kind) = if partial.is_empty() {
        (PathBuf::from(current_dir), String::new(), "relative")
    } else if partial == "~" {
        (
            home_dir().unwrap_or_else(|| PathBuf::from("/")),
            String::new(),
            "home",
        )
    } else if partial.starts_with("~/") {
        let home = home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let rest = partial.trim_start_matches("~/");
        let pb = PathBuf::from(rest);
        let base = if partial.ends_with('/') {
            home.join(&pb)
        } else {
            home.join(pb.parent().unwrap_or_else(|| Path::new("")))
        };
        let frag = if partial.ends_with('/') {
            String::new()
        } else {
            pb.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        };
        (base, frag, "home")
    } else if Path::new(partial).is_absolute() {
        let pb = PathBuf::from(partial);
        let base = if partial.ends_with('/') {
            pb.clone()
        } else {
            pb.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| pb.clone())
        };
        let frag = if partial.ends_with('/') {
            String::new()
        } else {
            pb.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        };
        (base, frag, "absolute")
    } else {
        let pb = PathBuf::from(partial);
        let base = if partial.ends_with('/') {
            Path::new(current_dir).join(&pb)
        } else {
            Path::new(current_dir).join(pb.parent().unwrap_or_else(|| Path::new("")))
        };
        let frag = if partial.ends_with('/') {
            String::new()
        } else {
            pb.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        };
        (base, frag, "relative")
    };

    let mut results = Vec::new();
    let Ok(entries) = fs::read_dir(&base_dir) else {
        return results;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&fragment) {
            continue;
        }

        let mut rendered = match prefix_kind {
            "home" => {
                if let Some(home) = home_dir() {
                    if let Ok(rel) = path.strip_prefix(home) {
                        format!("~/{}", rel.display())
                    } else {
                        path.to_string_lossy().to_string()
                    }
                } else {
                    path.to_string_lossy().to_string()
                }
            }
            "absolute" => path.to_string_lossy().to_string(),
            _ => path
                .strip_prefix(current_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string()),
        };

        if path.is_dir() && !rendered.ends_with('/') {
            rendered.push('/');
        }
        results.push(rendered);
    }

    results.sort_by_key(|a| a.to_ascii_lowercase());
    results
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
    let header_title = match state.active_page {
        DashboardPage::Home => "Stassh Dashboard",
        DashboardPage::Settings => "Stassh Settings",
        DashboardPage::Debug => "Stassh Debug",
        DashboardPage::Ssh => "Stassh SSH Session",
    };
    let (inner, area) = full_rect(a, header_title, footer);
    frame.render_widget(inner, a);
    let content_block = frame_block();
    let content_area = content_block.inner(area);
    frame.render_widget(content_block, area);

    match state.active_page {
        DashboardPage::Home => pages::home::render(frame, content_area, app, state),
        DashboardPage::Settings => pages::settings::render(frame, content_area, app, state),
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
        "Tab/Up/Down move | Ctrl+S save | Esc",
    );

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let auth_value_height = if modal.form.auth_mode == HostAuthMode::Key
        && modal.form.key_input_mode == HostKeyInputMode::Inline
    {
        5
    } else {
        3
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(auth_value_height),
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
        None,
    );
    render_input_field(
        frame,
        chunks[1],
        "User",
        &modal.form.user,
        modal.form.focus == HostFormField::User,
        modal.form.caret,
        false,
        None,
    );
    render_input_field(
        frame,
        chunks[2],
        "Endpoints (host:port, one per line)",
        &modal.form.endpoints,
        modal.form.focus == HostFormField::Endpoints,
        modal.form.caret,
        false,
        None,
    );

    let auth_text = match auth_selection(&modal.form) {
        AuthSelection::KeyPath => "key path on system".to_string(),
        AuthSelection::KeyInline => "key in database".to_string(),
        AuthSelection::Password => "password".to_string(),
    };
    render_input_field(
        frame,
        chunks[3],
        "Auth mode [Left/Right cycles: path key, db key, password]",
        &auth_text,
        modal.form.focus == HostFormField::AuthMode,
        modal.form.caret,
        false,
        None,
    );

    let auth_value_label = if modal.form.auth_mode == HostAuthMode::Key {
        if modal.form.key_input_mode == HostKeyInputMode::Path {
            "System key [Enter/Right select]"
        } else {
            "Load key into DB (paste or import from picker)"
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
        if modal.form.auth_mode == HostAuthMode::Key
            && modal.form.key_input_mode == HostKeyInputMode::Path
        {
            Some("no key selected")
        } else {
            None
        },
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
    placeholder: Option<&str>,
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

    let showing_placeholder = value.is_empty() && placeholder.is_some();
    let text_value = if secret {
        mask(value)
    } else if let Some(placeholder_text) = placeholder {
        if value.is_empty() {
            placeholder_text.to_string()
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    };

    let line = if selected && !showing_placeholder {
        line_with_caret_value(&text_value, caret)
    } else {
        Line::from(text_value)
    };
    let value_style = if showing_placeholder {
        muted_text()
    } else {
        text()
    };
    frame.render_widget(Paragraph::new(line).style(value_style), inner);
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

fn line_with_caret_prefix(prefix: &str, text_value: &str, caret: usize) -> Line<'static> {
    let safe_caret = caret.min(text_value.len());
    let before = text_value[..safe_caret].to_string();
    let current = text_value[safe_caret..].chars().next().unwrap_or(' ');
    let after = if safe_caret < text_value.len() {
        text_value[safe_caret + current.len_utf8()..].to_string()
    } else {
        String::new()
    };

    Line::from(vec![
        Span::styled(prefix.to_string(), muted_text()),
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
    let height = 16;
    let area = centered_rect_no_border(width, height, host_popup);
    frame.render_widget(Clear, area);
    let mode_hint = if picker.target_mode == HostKeyInputMode::Path {
        "Path mode"
    } else {
        "Load-into-DB mode"
    };
    let block = modal_block(
        "File Tree Picker",
        "Commands: cd <path>, select <path|name> | Tab complete | Enter run | Esc close",
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("{mode_hint} | {}", picker.current_dir)).style(muted_text()),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(line_with_caret_prefix(
            "> ",
            &picker.command_input,
            picker.command_input.len(),
        ))
        .style(accent_text()),
        chunks[1],
    );

    let mut lines = Vec::new();
    let visible = chunks[2].height as usize;
    let mut start = picker.scroll;
    let max_selected = picker.entries.len().saturating_sub(1);
    let selected = picker.selected.min(max_selected);
    if visible > 0 && selected >= start.saturating_add(visible) {
        start = selected + 1 - visible;
    }

    if picker.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no files or directories found",
            muted_text(),
        )));
    } else {
        for (idx, entry) in picker.entries.iter().enumerate().skip(start).take(visible) {
            let marker = if idx == selected { ">" } else { " " };
            let entry_style = if entry.is_dir { muted_text() } else { text() };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), text()),
                Span::styled(entry.label.clone(), entry_style),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), chunks[2]);

    let status_line = picker
        .error
        .as_deref()
        .map(|msg| (msg, danger_text()))
        .or_else(|| picker.status.as_deref().map(|msg| (msg, muted_text())))
        .unwrap_or((" ", muted_text()));
    frame.render_widget(
        Paragraph::new(status_line.0).style(status_line.1),
        chunks[3],
    );
}

fn keybind_hint(state: &DashboardState, app: &AppState, area: Rect) -> &'static str {
    if state.host_modal.is_some() {
        return "HOST form: Tab/Up/Down move | Ctrl+S save | Esc";
    }

    if state.quick_switcher.is_some() {
        return "SWITCHER: type to filter | Up/Down select | Enter open | Esc close";
    }

    match state.active_page {
        DashboardPage::Home => pages::home::footer_hint(),
        DashboardPage::Settings => pages::settings::footer_hint(state),
        DashboardPage::Debug => pages::debug::footer_hint(pages::debug::has_scrollbar(app, area)),
        DashboardPage::Ssh => pages::ssh::footer_hint(),
    }
}
