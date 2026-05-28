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
        DashboardPage, DashboardState, EndpointPickerState, HostConnectionStatus, HostFormState,
        HostModalMode, HostModalState, SshSessionState,
    },
    screens::AppEffect,
    ui::{
        accent_text, border, centered_rect_no_border, danger_text, muted_text,
        panel_alt_background, selected_border, success_text, text, warning_text,
    },
};

const HOME_GRID_COLUMNS: usize = 3;
const HOST_CARD_HEIGHT: u16 = 7;
const GROUP_HEADER_HEIGHT: u16 = 1;
const UNGROUPED_LABEL: &str = "Ungrouped";

struct HostGroupView {
    name: String,
    host_indices: Vec<usize>,
    ungrouped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisibleHostPosition {
    visible_index: usize,
    group_index: usize,
    row: usize,
    col: usize,
}

#[derive(Clone, Copy)]
enum MoveDirection {
    Left,
    Right,
    Up,
    Down,
}

pub(crate) fn handle_key(
    app: &AppState,
    key: KeyEvent,
    state: &mut DashboardState,
) -> Option<AppEffect> {
    let groups = grouped_hosts(&app.db.hosts);
    let visible_len = visible_host_indices(&groups).len();
    let columns = HOME_GRID_COLUMNS.min(visible_len.max(1));
    match key.code {
        KeyCode::Char('a') => {
            state.host_modal = Some(HostModalState {
                mode: HostModalMode::Create,
                form: HostFormState::new(),
                key_picker: None,
            });
        }
        KeyCode::Char('e') => {
            if let Some(host) = selected_host(app, state.selected_host) {
                state.host_modal = Some(HostModalState {
                    mode: HostModalMode::Edit { host_id: host.id },
                    form: form_from_host(host),
                    key_picker: None,
                });
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            state.selected_host =
                move_selection(state.selected_host, &groups, columns, MoveDirection::Left);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.selected_host =
                move_selection(state.selected_host, &groups, columns, MoveDirection::Right);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected_host =
                move_selection(state.selected_host, &groups, columns, MoveDirection::Up);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected_host =
                move_selection(state.selected_host, &groups, columns, MoveDirection::Down);
        }
        KeyCode::Enter => {
            if let Some(host) = selected_host(app, state.selected_host) {
                if host.endpoints.len() > 1 {
                    let preferred = app
                        .db
                        .remembered_endpoint_indices
                        .get(&host.id)
                        .copied()
                        .unwrap_or(0)
                        .min(host.endpoints.len().saturating_sub(1));
                    state.endpoint_picker = Some(EndpointPickerState {
                        host_id: host.id,
                        host_name: host.name.clone(),
                        host_user: host.user.clone(),
                        endpoints: host.endpoints.clone(),
                        selected: preferred,
                    });
                } else {
                    let endpoint = host
                        .endpoints
                        .first()
                        .map(|e| format!("{}:{}", e.host, e.port))
                        .unwrap_or_else(|| "n/a".to_string());
                    let title = format!("{} - {}@{}", host.name, host.user, endpoint);
                    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
                    let (cols, rows) = super::super::ssh_viewport_size_from_terminal(
                        cols,
                        rows,
                        app.config.ssh_fullscreen,
                    );
                    state.ssh_tabs.push(SshSessionState::new_starting(
                        title,
                        rows,
                        cols,
                        host.id,
                        Some(0),
                    ));
                    let idx = state.ssh_tabs.len().saturating_sub(1);
                    state.active_ssh_tab = Some(idx);
                    state.active_page = DashboardPage::Ssh;
                }
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

    let groups = grouped_hosts(&app.db.hosts);
    let visible_indices = visible_host_indices(&groups);
    if visible_indices.is_empty() {
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

    let content_area = layout[1];
    let columns = HOME_GRID_COLUMNS.min(visible_indices.len().max(1));
    let mut row_constraints = Vec::new();
    for group in &groups {
        row_constraints.push(Constraint::Length(GROUP_HEADER_HEIGHT));
        row_constraints.extend(vec![
            Constraint::Length(HOST_CARD_HEIGHT);
            group.host_indices.len().div_ceil(columns)
        ]);
    }
    row_constraints.push(Constraint::Fill(1));
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(content_area);

    let selected_visible = state
        .selected_host
        .min(visible_indices.len().saturating_sub(1));
    let mut row_area_idx = 0;
    let mut visible_idx = 0;

    for group in &groups {
        if let Some(header_area) = row_areas.get(row_area_idx) {
            render_group_header(frame, *header_area, group);
        }
        row_area_idx += 1;

        let rows = group.host_indices.len().div_ceil(columns);
        for row_idx in 0..rows {
            let Some(row_area) = row_areas.get(row_area_idx) else {
                return;
            };
            row_area_idx += 1;

            let column_areas = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, columns as u32); columns])
                .split(*row_area);

            for (col_idx, col_area) in column_areas.iter().enumerate() {
                let index = row_idx * columns + col_idx;
                if let Some(host_index) = group.host_indices.get(index).copied()
                    && let Some(host) = app.db.hosts.get(host_index)
                {
                    let selected = visible_idx == selected_visible;
                    let statuses =
                        state
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
                    visible_idx += 1;
                }
            }
        }
    }
}

pub(crate) fn footer_hint() -> &'static str {
    "Arrows/hjkl move | A add | E edit | Enter connect | Ctrl+Alt+F SSH fullscreen | Ctrl+Q switch | Esc exit"
}

pub(crate) fn visible_index_for_host_id(hosts: &[SshHost], host_id: u32) -> Option<usize> {
    let groups = grouped_hosts(hosts);
    visible_host_indices(&groups).iter().position(|host_index| {
        hosts
            .get(*host_index)
            .is_some_and(|host| host.id == host_id)
    })
}

fn selected_host(app: &AppState, selected: usize) -> Option<&SshHost> {
    let groups = grouped_hosts(&app.db.hosts);
    let visible_indices = visible_host_indices(&groups);
    let host_index = visible_indices.get(selected).copied()?;
    app.db.hosts.get(host_index)
}

fn grouped_hosts(hosts: &[SshHost]) -> Vec<HostGroupView> {
    let mut named_groups: Vec<String> = Vec::new();
    let mut has_ungrouped = false;

    for host in hosts {
        let group = host.group.trim();
        if group.is_empty() {
            has_ungrouped = true;
            continue;
        }
        if !named_groups
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(group))
        {
            named_groups.push(group.to_string());
        }
    }

    named_groups.sort_by_key(|group| group.to_ascii_lowercase());
    let mut groups = Vec::new();

    for group_name in named_groups {
        let mut host_indices = hosts
            .iter()
            .enumerate()
            .filter(|(_, host)| host.group.trim().eq_ignore_ascii_case(&group_name))
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>();
        sort_host_indices(hosts, &mut host_indices);
        groups.push(HostGroupView {
            name: group_name,
            host_indices,
            ungrouped: false,
        });
    }

    if has_ungrouped {
        let mut host_indices = hosts
            .iter()
            .enumerate()
            .filter(|(_, host)| host.group.trim().is_empty())
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>();
        sort_host_indices(hosts, &mut host_indices);
        groups.push(HostGroupView {
            name: UNGROUPED_LABEL.to_string(),
            host_indices,
            ungrouped: true,
        });
    }

    groups
}

fn sort_host_indices(hosts: &[SshHost], indices: &mut [usize]) {
    indices.sort_by(|a, b| {
        let host_a = &hosts[*a];
        let host_b = &hosts[*b];
        host_a
            .name
            .to_ascii_lowercase()
            .cmp(&host_b.name.to_ascii_lowercase())
            .then_with(|| host_a.id.cmp(&host_b.id))
    });
}

fn visible_host_indices(groups: &[HostGroupView]) -> Vec<usize> {
    groups
        .iter()
        .flat_map(|group| group.host_indices.iter().copied())
        .collect()
}

fn render_group_header(frame: &mut Frame, area: Rect, group: &HostGroupView) {
    let style = if group.ungrouped {
        muted_text()
    } else {
        accent_text()
    };
    let line = Line::from(vec![
        Span::styled(group.name.clone(), style),
        Span::styled(
            format!(
                "  {} host{}",
                group.host_indices.len(),
                if group.host_indices.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            muted_text(),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
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
    form.group = host.group.clone();
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

fn move_selection(
    selected: usize,
    groups: &[HostGroupView],
    columns: usize,
    direction: MoveDirection,
) -> usize {
    let positions = visible_host_positions(groups, columns);
    if positions.is_empty() {
        return 0;
    }

    let selected = selected.min(positions.len().saturating_sub(1));
    let current = positions[selected];
    match direction {
        MoveDirection::Left => positions
            .iter()
            .find(|position| {
                position.group_index == current.group_index
                    && position.row == current.row
                    && position.col + 1 == current.col
            })
            .map_or(selected, |position| position.visible_index),
        MoveDirection::Right => positions
            .iter()
            .find(|position| {
                position.group_index == current.group_index
                    && position.row == current.row
                    && position.col == current.col + 1
            })
            .map_or(selected, |position| position.visible_index),
        MoveDirection::Up => move_vertical(current, &positions, -1).unwrap_or(selected),
        MoveDirection::Down => move_vertical(current, &positions, 1).unwrap_or(selected),
    }
}

fn move_vertical(
    current: VisibleHostPosition,
    positions: &[VisibleHostPosition],
    step: isize,
) -> Option<usize> {
    let mut rows = positions
        .iter()
        .map(|position| (position.group_index, position.row))
        .collect::<Vec<_>>();
    rows.dedup();

    let current_row_idx = rows
        .iter()
        .position(|row| *row == (current.group_index, current.row))?;
    let target_row_idx = if step < 0 {
        current_row_idx.checked_sub(1)?
    } else {
        let next = current_row_idx + 1;
        if next >= rows.len() {
            return None;
        }
        next
    };
    let (target_group, target_row) = rows[target_row_idx];

    positions
        .iter()
        .filter(|position| position.group_index == target_group && position.row == target_row)
        .min_by_key(|position| position.col.abs_diff(current.col))
        .map(|position| position.visible_index)
}

fn visible_host_positions(groups: &[HostGroupView], columns: usize) -> Vec<VisibleHostPosition> {
    let columns = columns.max(1);
    let mut positions = Vec::new();

    for (group_index, group) in groups.iter().enumerate() {
        for idx in 0..group.host_indices.len() {
            positions.push(VisibleHostPosition {
                visible_index: positions.len(),
                group_index,
                row: idx / columns,
                col: idx % columns,
            });
        }
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::{HostGroupView, MoveDirection, move_selection};

    fn group(count: usize) -> HostGroupView {
        HostGroupView {
            name: "group".to_string(),
            host_indices: (0..count).collect(),
            ungrouped: false,
        }
    }

    #[test]
    fn down_skips_group_headers_and_preserves_column() {
        let groups = vec![group(1), group(3)];
        assert_eq!(move_selection(0, &groups, 3, MoveDirection::Down), 1);
    }

    #[test]
    fn up_from_second_group_returns_to_previous_group_row() {
        let groups = vec![group(2), group(3)];
        assert_eq!(move_selection(3, &groups, 3, MoveDirection::Up), 1);
    }

    #[test]
    fn down_clamps_to_last_column_in_shorter_row() {
        let groups = vec![group(3), group(1)];
        assert_eq!(move_selection(2, &groups, 3, MoveDirection::Down), 3);
    }

    #[test]
    fn horizontal_navigation_stays_within_group_row() {
        let groups = vec![group(1), group(2)];
        assert_eq!(move_selection(0, &groups, 3, MoveDirection::Right), 0);
        assert_eq!(move_selection(1, &groups, 3, MoveDirection::Right), 2);
        assert_eq!(move_selection(1, &groups, 3, MoveDirection::Left), 1);
    }
}
