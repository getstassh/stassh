use backend::{AppState, HostAuth, SshHost};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostConnectionStatus, HostFormState, HostModalMode,
        HostModalState, SshSessionState,
    },
    screens::AppEffect,
};

const HOME_GRID_COLUMNS: usize = 3;
const HOST_CARD_HEIGHT: u16 = 6;

pub(crate) fn handle_key(
    app: &AppState,
    key: KeyEvent,
    state: &mut DashboardState,
) -> Option<AppEffect> {
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

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    if app.db.hosts.is_empty() {
        let message = if let Some(status) = &state.last_status {
            format!(
                "No hosts yet.\n\nPress A to create your first SSH host.\n\nLast status: {status}"
            )
        } else {
            "No hosts yet.\n\nPress A to create your first SSH host.".to_string()
        };
        frame.render_widget(Paragraph::new(message).alignment(Alignment::Left), area);
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

pub(crate) fn footer_hint() -> &'static str {
    "HOME: arrows or hjkl move | A add | E edit | Enter connect | R refresh | Ctrl+B toggle sidebar | Esc exit"
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

fn form_from_host(host: &SshHost) -> HostFormState {
    let mut form = HostFormState::new();
    form.name = host.name.clone();
    form.host = host.host.clone();
    form.user = host.user.clone();
    form.port = host.port.to_string();
    match &host.auth {
        HostAuth::Key { key_path } => {
            form.auth_mode = crate::navigation::HostAuthMode::Key;
            form.key_path = key_path.clone();
        }
        HostAuth::Password { password } => {
            form.auth_mode = crate::navigation::HostAuthMode::Password;
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
