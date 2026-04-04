use backend::{AppState, HostAuth, SshHost};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostAuthMode, HostFormField, HostFormState, HostModalMode,
        HostModalState, Screen,
    },
    screens::{AppEffect, ScreenHandler},
    ui::full_rect,
};

const HOME_GRID_COLUMNS: usize = 3;
const HOST_CARD_HEIGHT: u16 = 6;

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
    handle_tick: |_app, _| None,
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
        KeyCode::Char('1') => state.active_page = DashboardPage::Home,
        KeyCode::Char('2') => state.active_page = DashboardPage::Settings,
        KeyCode::Char('3') => state.active_page = DashboardPage::Debug,
        KeyCode::Char('4') => state.active_page = DashboardPage::Credits,
        _ => {}
    }

    if app.config.show_sidebar {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                state.active_page = prev_page(state.active_page);
                return None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.active_page = next_page(state.active_page);
                return None;
            }
            _ => {}
        }

        return None;
    }

    if state.active_page != DashboardPage::Home {
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
        KeyCode::Enter | KeyCode::Char('c') => {
            if app.db.hosts.get(state.selected_host).is_some() {
                return None;
            }
        }
        _ => {}
    }

    None
}

fn handle_paste(_app: &AppState, text: &str, state: &mut DashboardState) -> Option<AppEffect> {
    if let Some(modal) = &mut state.host_modal {
        insert_pasted_text(&mut modal.form, text);
    }
    None
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
        DashboardPage::Debug => frame.render_widget(render_debug(app), content_area),
        DashboardPage::Credits => frame.render_widget(render_credits(), content_area),
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

    let nav_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .split(sidebar_area);

    render_sidebar_item(frame, nav_areas[0], state, DashboardPage::Home, 1, "Home");
    render_sidebar_item(
        frame,
        nav_areas[1],
        state,
        DashboardPage::Settings,
        2,
        "Settings",
    );
    render_sidebar_item(frame, nav_areas[2], state, DashboardPage::Debug, 3, "Debug");
    render_sidebar_item(
        frame,
        nav_areas[3],
        state,
        DashboardPage::Credits,
        4,
        "Credits",
    );
}

fn render_sidebar_item(
    frame: &mut Frame,
    area: Rect,
    state: &DashboardState,
    page: DashboardPage,
    index: u8,
    title: &str,
) {
    let selected = state.active_page == page;
    let border = if selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default().borders(Borders::ALL).border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let label = if selected {
        format!("> {} {}", index, title)
    } else {
        format!("  {} {}", index, title)
    };
    let text = Paragraph::new(label).alignment(Alignment::Left);
    frame.render_widget(text, inner);
}

fn render_home(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    if app.db.hosts.is_empty() {
        let empty = Paragraph::new(
            "No hosts yet.\n\nPress A to create your first SSH host.\nUse 1-4 to change pages.",
        )
        .alignment(Alignment::Left);
        frame.render_widget(empty, area);
        return;
    }

    let columns = HOME_GRID_COLUMNS.min(app.db.hosts.len().max(1));
    let rows = app.db.hosts.len().div_ceil(columns);

    let mut row_constraints = vec![Constraint::Length(HOST_CARD_HEIGHT); rows];
    row_constraints.push(Constraint::Fill(1));
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    for (row_idx, row_area) in row_areas.iter().take(rows).enumerate() {
        let column_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Ratio(1, columns as u32); columns])
            .split(*row_area);

        for (col_idx, col_area) in column_areas.iter().enumerate() {
            let index = row_idx * columns + col_idx;
            if let Some(host) = app.db.hosts.get(index) {
                let selected = index == state.selected_host;
                render_host_card(frame, *col_area, host, selected);
            }
        }
    }
}

fn render_host_card(frame: &mut Frame, area: Rect, host: &SshHost, selected: bool) {
    let border = if selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .title(format!(" {} ", host.name))
        .borders(Borders::ALL)
        .border_style(border);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let auth_label = match &host.auth {
        HostAuth::Key { .. } => "key",
        HostAuth::Password { .. } => "password",
    };

    let content = Paragraph::new(format!(
        "{}@{}:{}\nauth: {}\n[e] edit  [enter/c] connect",
        host.user, host.host, host.port, auth_label
    ));
    frame.render_widget(content, inner);
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
        return "HOST form: Tab/Shift+Tab move field, Ctrl+S save, Esc cancel";
    }

    if sidebar_visible {
        return "Use 1-4 or Up/Down (j/k) for pages, Ctrl+B to hide sidebar";
    }

    match state.active_page {
        DashboardPage::Home => {
            "HOME: arrows or hjkl move, A add, E edit, Enter/C connect, Ctrl+B toggle sidebar"
        }
        DashboardPage::Settings => "Ctrl+B toggle sidebar",
        DashboardPage::Debug => "Ctrl+B toggle sidebar",
        DashboardPage::Credits => "Ctrl+B toggle sidebar",
    }
}

fn render_settings(app: &AppState) -> Paragraph<'static> {
    Paragraph::new(format!(
        "Settings\n\n- Telemetry enabled: {:?}\n- Database encryption: {:?}\n- Sidebar visible: {:?}",
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
