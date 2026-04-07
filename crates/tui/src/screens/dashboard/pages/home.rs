use backend::{AppState, HostAuth::*, SshHost};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    navigation::{
        DashboardPage, DashboardState, HostConnectionStatus, HostFormState, HostModalMode,
        HostModalState, SshSessionState,
    },
    screens::AppEffect,
    ui::{
        accent_text, border, centered_rect_no_border, danger_text, muted_text,
        panel_alt_background, selected_border, success_text, text, warning_text,
    },
};

const HOME_GRID_COLUMNS: usize = 3;
const HOST_CARD_HEIGHT: u16 = 7;

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
                key_picker: None,
            });
        }
        KeyCode::Char('e') => {
            if let Some(host) = app.db.hosts.get(state.selected_host) {
                state.host_modal = Some(HostModalState {
                    mode: HostModalMode::Edit { host_id: host.id },
                    form: form_from_host(host),
                    key_picker: None,
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
                let primary = host.endpoints.first();
                let endpoint = primary
                    .map(|e| format!("{}:{}", e.host, e.port))
                    .unwrap_or_else(|| "n/a".to_string());
                let title = format!("{} - {}@{}", host.name, host.user, endpoint);
                let rows_cols = crossterm::terminal::size().unwrap_or((120, 40));
                let rows = rows_cols.1;
                let cols = rows_cols.0;
                state
                    .ssh_tabs
                    .push(SshSessionState::new_starting(title, rows, cols, host_id));
                let idx = state.ssh_tabs.len().saturating_sub(1);
                state.active_ssh_tab = Some(idx);
                state.active_page = DashboardPage::Ssh;
            }
        }
        _ => {}
    }

    None
}

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    if app.db.hosts.is_empty() {
        let center_area = centered_rect_no_border(60, 30, area);
        let main_text = Paragraph::new("No hosts yet.")
            .alignment(Alignment::Center)
            .style(accent_text());
        let guide_text = Paragraph::new("Press A to register your first SSH target.")
            .alignment(Alignment::Center)
            .style(muted_text());
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(center_area);
        frame.render_widget(main_text, layout[0]);
        frame.render_widget(guide_text, layout[1]);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    if let Some(status) = &state.last_status {
        let header = Line::from(vec![
            Span::styled("Last status: ", muted_text()),
            Span::styled(status.clone(), warning_text()),
        ]);
        frame.render_widget(
            Paragraph::new(header).alignment(Alignment::Center),
            layout[0],
        );
    }

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
                let statuses = state
                    .host_statuses
                    .get(&host.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        host.endpoints
                            .clone()
                            .into_iter()
                            .map(|_| HostConnectionStatus::Unknown)
                            .collect()
                    });
                render_host_card(frame, *col_area, host, selected, &statuses);
            }
        }
    }
}

pub(crate) fn footer_hint() -> &'static str {
    "Arrows or hjkl move | A add | E edit | Enter connect | Ctrl+Q quick switch | Esc exit"
}

fn render_host_card(
    frame: &mut Frame,
    area: Rect,
    host: &SshHost,
    selected: bool,
    statuses: &[HostConnectionStatus],
) {
    let any_reachable = statuses
        .iter()
        .any(|s| matches!(s, HostConnectionStatus::Reachable));
    let any_unknown = statuses
        .iter()
        .any(|s| matches!(s, HostConnectionStatus::Unknown));
    let border_style = if selected {
        selected_border()
    } else if !any_reachable && !any_unknown {
        danger_text()
    } else {
        border()
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ", muted_text()),
            Span::styled(
                host.name.clone(),
                if selected { accent_text() } else { text() },
            ),
            Span::styled(" ", muted_text()),
        ]))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(panel_alt_background());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let auth_label = match &host.auth {
        KeyPath { .. } => "key(path)",
        KeyInline { .. } => "key(inline)",
        Password { .. } => "password",
    };

    let statuses_line = status_letters(statuses);
    let primary_endpoint = host
        .endpoints
        .first()
        .map(|e| format!("{}@{}:{}", host.user, e.host, e.port))
        .unwrap_or_else(|| format!("{}@n/a", host.user));

    let content = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("endpoint: ", muted_text()),
            Span::styled(primary_endpoint, text()),
        ]),
        statuses_line,
        Line::from(vec![
            Span::styled("auth:     ", muted_text()),
            Span::styled(auth_label, text()),
        ]),
    ])
    .style(Style::default().add_modifier(if selected {
        Modifier::BOLD
    } else {
        Modifier::empty()
    }));
    frame.render_widget(content, inner);
}

fn form_from_host(host: &SshHost) -> HostFormState {
    let mut form = HostFormState::new();
    form.name = host.name.clone();
    form.user = host.user.clone();
    form.endpoints = host
        .endpoints
        .iter()
        .map(|e| format!("{}:{}", e.host, e.port))
        .collect::<Vec<_>>()
        .join(", ");
    match &host.auth {
        KeyPath { key_path } => {
            form.auth_mode = crate::navigation::HostAuthMode::Key;
            form.key_path = key_path.clone();
        }
        KeyInline { private_key } => {
            form.auth_mode = crate::navigation::HostAuthMode::Key;
            form.key_input_mode = crate::navigation::HostKeyInputMode::Inline;
            form.key_inline = private_key.clone();
        }
        Password { password } => {
            form.auth_mode = crate::navigation::HostAuthMode::Password;
            form.password = password.clone();
        }
    }
    form
}

fn status_letters(statuses: &[HostConnectionStatus]) -> Line<'static> {
    let single_endpoint = statuses.len() == 1;
    let mut spans = vec![Span::styled("status:   ", muted_text())];
    for s in statuses {
        let (label, style) = match s {
            HostConnectionStatus::Reachable => (
                if single_endpoint { "Reachable" } else { "G" },
                success_text(),
            ),
            HostConnectionStatus::Unreachable => (
                if single_endpoint { "Unreachable" } else { "R" },
                danger_text(),
            ),
            HostConnectionStatus::Unknown => {
                (if single_endpoint { "Unknown" } else { "?" }, muted_text())
            }
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::styled(" ", muted_text()));
    }

    Line::from(spans)
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
