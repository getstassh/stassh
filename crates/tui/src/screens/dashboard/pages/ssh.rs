use backend::{AppState, TrustedHostKey};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph},
};

use crate::{
    inputs::handle_yes_no_input,
    navigation::{DashboardPage, DashboardState, SshSessionPhase, SshSessionState},
    screens::AppEffect,
    ssh_client::{
        SessionEvent, SessionInput, StartSessionResult, TrustChallenge, start_session_async,
    },
    ui::{accent_text, button, centered_rect_no_border, modal_block, muted_text, text},
};

const DASHBOARD_SHELL_BORDER: u16 = 2;
const SCROLLBACK_STEP_MIN: usize = 8;

pub(crate) fn dashboard_ssh_viewport_size_from_terminal(cols: u16, rows: u16) -> (u16, u16) {
    (
        cols.saturating_sub(DASHBOARD_SHELL_BORDER).max(1),
        rows.saturating_sub(DASHBOARD_SHELL_BORDER).max(1),
    )
}

pub(crate) fn handle_key(key: KeyEvent, state: &mut DashboardState) -> Option<AppEffect> {
    let Some(tab_idx) = state.active_ssh_tab else {
        state.active_page = DashboardPage::Home;
        return None;
    };

    let Some(tab) = state.ssh_tabs.get_mut(tab_idx) else {
        state.active_ssh_tab = None;
        state.active_page = DashboardPage::Home;
        return None;
    };

    let mut close_status: Option<String> = None;
    let mut trust_key: Option<(u32, Option<usize>, TrustedHostKey)> = None;

    match &mut tab.phase {
        SshSessionPhase::Starting { pending, .. } => {
            if key.code == KeyCode::Esc {
                if let Some(pending) = pending {
                    pending.cancel();
                }
                close_status = Some("Connection canceled".to_string());
            }
        }
        SshSessionPhase::TrustPrompt {
            host_id,
            selected_endpoint_index,
            challenge,
            choice,
        } => {
            if key.code == KeyCode::Esc {
                close_status = Some("Connection canceled: host key not trusted".to_string());
            } else if let Some(trust_now) = handle_yes_no_input(choice, key.code) {
                if trust_now {
                    trust_key = Some((
                        *host_id,
                        *selected_endpoint_index,
                        challenge.proposed_key.clone(),
                    ));
                } else {
                    close_status = Some("Connection canceled: host key not trusted".to_string());
                }
            }
        }
        SshSessionPhase::Running { .. } => {
            if key.code == KeyCode::Esc {
                if let SshSessionPhase::Running { live } = &mut tab.phase {
                    live.send_input(SessionInput::Disconnect);
                }
                return None;
            }

            if handle_scrollback_key(key, tab) {
                return None;
            }

            if let Some(bytes) = key_to_bytes(key) {
                tab.parser.screen_mut().set_scrollback(0);
                if let SshSessionPhase::Running { live } = &mut tab.phase {
                    live.send_input(SessionInput::Data(bytes));
                }
            }
        }
    }

    if let Some((host_id, selected_endpoint_index, key)) = trust_key {
        if let Some(tab) = state.ssh_tabs.get_mut(tab_idx) {
            tab.phase = SshSessionPhase::starting(host_id, selected_endpoint_index);
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

pub(crate) fn handle_paste(text: &str, state: &mut DashboardState) {
    let Some(tab_idx) = state.active_ssh_tab else {
        return;
    };
    let Some(tab) = state.ssh_tabs.get_mut(tab_idx) else {
        return;
    };

    if let SshSessionPhase::Running { live } = &tab.phase {
        tab.parser.screen_mut().set_scrollback(0);
        live.send_input(SessionInput::Data(text.as_bytes().to_vec()));
    }
}

pub(crate) fn handle_resize(cols: u16, rows: u16, state: &mut DashboardState) {
    if cols == 0 || rows == 0 {
        return;
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
}

pub(crate) fn tick_tabs(app: &AppState, state: &mut DashboardState) {
    let mut idx = 0;
    while idx < state.ssh_tabs.len() {
        let mut close_status = None;
        let tab = &mut state.ssh_tabs[idx];

        let mut next_phase = None;
        match &mut tab.phase {
            SshSessionPhase::Starting {
                host_id,
                selected_endpoint_index,
                pending,
                spinner_frame,
                ..
            } => {
                *spinner_frame = spinner_frame.wrapping_add(1);

                if pending.is_none() {
                    if let Some(host) = app.db.hosts.iter().find(|h| h.id == *host_id).cloned() {
                        *pending = Some(start_session_async(
                            &host,
                            *selected_endpoint_index,
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
                                        selected_endpoint_index: *selected_endpoint_index,
                                        challenge,
                                        choice: crate::navigation::YesNoState { selected: false },
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

pub(crate) fn render(frame: &mut Frame, app_area: Rect, area: Rect, state: &DashboardState) {
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
            // center widget
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(50),
                        Constraint::Length(3),
                        Constraint::Percentage(50),
                    ]
                    .as_ref(),
                )
                .split(area);
            frame.render_widget(
                Paragraph::new(format!(
                    "{spinner} Connecting to {}\n\nPlease wait... ({elapsed:.1}s)",
                    tab.title
                ))
                .alignment(Alignment::Center)
                .style(text()),
                split[1],
            );
        }
        SshSessionPhase::TrustPrompt {
            challenge, choice, ..
        } => {
            frame.render_widget(
                Paragraph::new(render_vt100_text(&tab.parser)).alignment(Alignment::Left),
                area,
            );
            render_trust_modal(frame, app_area, challenge, choice);
        }
        SshSessionPhase::Running { .. } => {
            frame.render_widget(
                Paragraph::new(render_vt100_text(&tab.parser)).alignment(Alignment::Left),
                area,
            );
        }
    }
}

pub(crate) fn footer_hint() -> &'static str {
    "SSH: type input | PgUp/PgDn/Home/End scroll | Esc disconnect | Ctrl+Q quick switch"
}

fn handle_scrollback_key(key: KeyEvent, tab: &mut SshSessionState) -> bool {
    let page = usize::from(tab.last_good_rows.max(1)).saturating_sub(1).max(SCROLLBACK_STEP_MIN);
    let screen = tab.parser.screen_mut();
    let current = screen.scrollback();

    match key.code {
        KeyCode::PageUp => {
            screen.set_scrollback(current.saturating_add(page));
            true
        }
        KeyCode::PageDown => {
            screen.set_scrollback(current.saturating_sub(page));
            true
        }
        KeyCode::Home => {
            screen.set_scrollback(usize::MAX);
            true
        }
        KeyCode::End => {
            screen.set_scrollback(0);
            true
        }
        _ => false,
    }
}

pub(crate) fn close_ssh_tab(state: &mut DashboardState, idx: usize, status: String) {
    if idx >= state.ssh_tabs.len() {
        return;
    }

    if let Some(tab) = state.ssh_tabs.get_mut(idx)
        && let SshSessionPhase::Running { live } = &mut tab.phase
    {
        live.stop();
    }

    state.ssh_tabs.remove(idx);
    state.last_status = Some(status);

    if state.ssh_tabs.is_empty() {
        state.active_ssh_tab = None;
        state.active_page = DashboardPage::Home;
        return;
    }

    let next_idx = idx.min(state.ssh_tabs.len().saturating_sub(1));
    state.active_ssh_tab = Some(next_idx);
}

fn trust_host_key(app: &mut crate::app::App, key: TrustedHostKey) {
    app.db
        .trusted_host_keys
        .retain(|k| !(k.host == key.host && k.port == key.port));
    app.db.trusted_host_keys.push(key);
    let _ = app.save_db();
}

fn render_trust_modal(
    frame: &mut Frame,
    app_area: Rect,
    challenge: &TrustChallenge,
    choice: &crate::navigation::YesNoState,
) {
    let width = (app_area.width.saturating_sub(4)).min(90);
    let height = 14;
    let popup_area = centered_rect_no_border(width, height, app_area);

    frame.render_widget(Clear, popup_area);
    let block = modal_block(
        "Host Key Verification",
        "<-/-> or Tab switch | Enter confirm | Esc cancel",
    );

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

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

    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Left)
            .style(text()),
        chunks[0],
    );

    let actions = Paragraph::new(Line::from(vec![
        Span::styled(
            button("Trust and connect", choice.is_yes()),
            if choice.is_yes() {
                accent_text()
            } else {
                muted_text()
            },
        ),
        Span::styled(" ", muted_text()),
        Span::styled(
            button("Cancel", choice.is_no()),
            if choice.is_no() {
                accent_text()
            } else {
                muted_text()
            },
        ),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(actions, chunks[2]);
}

fn render_vt100_text(parser: &vt100::Parser) -> Text<'static> {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 {
        return Text::from(Vec::<Line<'static>>::new());
    }

    let (raw_cursor_row, raw_cursor_col) = screen.cursor_position();
    let cursor_visible = !screen.hide_cursor() && screen.scrollback() == 0;
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
