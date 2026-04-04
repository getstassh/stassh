use backend::{AppState, TrustedHostKey};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    navigation::{DashboardState, Screen, SshSessionPhase, SshSessionState},
    screens::{AppEffect, ScreenHandler},
    ssh_client::{SessionEvent, SessionInput, StartSessionResult, start_session_async},
    ui::full_rect,
};

pub(crate) static HANDLER: ScreenHandler<SshSessionState> = ScreenHandler {
    matches: |s| matches!(s, Screen::SshSession { .. }),
    get: |s| match s {
        Screen::SshSession { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::SshSession { state } => Some(state),
        _ => None,
    },
    render: ui,
    handle_key,
    handle_paste,
    handle_resize,
    handle_tick,
};

fn handle_key(_: &AppState, key: KeyEvent, state: &mut SshSessionState) -> Option<AppEffect> {
    match &mut state.phase {
        SshSessionPhase::Starting { pending, .. } => {
            if key.code == KeyCode::Esc {
                if let Some(pending) = pending {
                    pending.cancel();
                }
                return Some(back_to_dashboard("Connection canceled".to_string()));
            }
            None
        }
        SshSessionPhase::TrustPrompt { host_id, challenge } => {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) {
                let host_id = *host_id;
                let key = challenge.proposed_key.clone();
                state.phase = SshSessionPhase::starting(host_id);
                return Some(Box::new(move |app| {
                    trust_host_key(app, key);
                }));
            }

            if matches!(key.code, KeyCode::Char('n') | KeyCode::Esc) {
                return Some(back_to_dashboard(
                    "Connection canceled: host key not trusted".to_string(),
                ));
            }

            None
        }
        SshSessionPhase::Running { live } => {
            if key.code == KeyCode::Esc {
                live.send_input(SessionInput::Disconnect);
                return None;
            }

            let bytes = key_to_bytes(key)?;
            live.send_input(SessionInput::Data(bytes));
            None
        }
        SshSessionPhase::Error(_) => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                return Some(back_to_dashboard("SSH session closed".to_string()));
            }
            None
        }
    }
}

fn handle_paste(_: &AppState, text: &str, state: &mut SshSessionState) -> Option<AppEffect> {
    if let SshSessionPhase::Running { live } = &state.phase {
        live.send_input(SessionInput::Data(text.as_bytes().to_vec()));
    }
    None
}

fn handle_resize(
    _: &AppState,
    cols: u16,
    rows: u16,
    state: &mut SshSessionState,
) -> Option<AppEffect> {
    if cols == 0 || rows == 0 {
        return None;
    }

    state.resize(rows, cols);
    if let SshSessionPhase::Running { live } = &state.phase {
        live.send_input(SessionInput::Resize {
            cols: cols.max(1),
            rows: rows.max(1),
        });
    }
    None
}

fn handle_tick(app: &AppState, state: &mut SshSessionState) -> Option<AppEffect> {
    let mut next_phase = None;

    match &mut state.phase {
        SshSessionPhase::Starting {
            host_id,
            pending,
            spinner_frame,
            ..
        } => {
            *spinner_frame = spinner_frame.wrapping_add(1);

            if pending.is_none() {
                let Some(host) = app.db.hosts.iter().find(|h| h.id == *host_id).cloned() else {
                    return Some(back_to_dashboard(
                        "Selected host no longer exists".to_string(),
                    ));
                };

                *pending = Some(start_session_async(
                    &host,
                    &app.db.trusted_host_keys,
                    state.last_good_rows,
                    state.last_good_cols,
                ));
            }

            let Some(pending_start) = pending.as_mut() else {
                return None;
            };

            let Some(result) = pending_start.try_recv() else {
                return None;
            };

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
                    next_phase = Some(SshSessionPhase::Error(error));
                }
            }
        }
        SshSessionPhase::TrustPrompt { .. } => {}
        SshSessionPhase::Error(_) => {}
        SshSessionPhase::Running { live } => {
            let parser = &mut state.parser;
            let mut events = Vec::new();
            let mut close_status = None;
            while let Some(event) = live.try_recv() {
                events.push(event);
            }

            for event in events {
                match event {
                    SessionEvent::OutputBytes(bytes) => parser.process(&bytes),
                    SessionEvent::Error(error) => {
                        close_status = Some(format!("SSH error: {error}"))
                    }
                    SessionEvent::Closed(status) => close_status = Some(status),
                }
            }

            if let Some(status) = close_status {
                live.stop();
                return Some(back_to_dashboard(status));
            }
        }
    }

    if let Some(phase) = next_phase {
        state.phase = phase;
    }

    None
}

fn back_to_dashboard(status: String) -> AppEffect {
    Box::new(move |app| {
        let mut dashboard = DashboardState::new();
        dashboard.last_status = Some(status);
        app.screen = Screen::Dashboard { state: dashboard };
    })
}

fn trust_host_key(app: &mut crate::app::App, key: TrustedHostKey) {
    app.db
        .trusted_host_keys
        .retain(|k| !(k.host == key.host && k.port == key.port));
    app.db.trusted_host_keys.push(key);
    let _ = app.save_db();
}

fn ui(frame: &mut Frame, _app: &AppState, state: &SshSessionState) {
    let a = frame.area();
    let help = match &state.phase {
        SshSessionPhase::Starting { .. } => "Connecting... Esc cancel",
        SshSessionPhase::TrustPrompt { .. } => "Y/Enter trust, N/Esc cancel",
        SshSessionPhase::Running { .. } => "Esc disconnect | Ctrl+C sends SIGINT to remote",
        SshSessionPhase::Error(_) => "Esc/Enter return to dashboard",
    };
    let title = format!("SSH Session - {}", state.title);
    let (inner, area) = full_rect(a, &title, help);
    frame.render_widget(inner, a);

    match &state.phase {
        SshSessionPhase::Starting { .. } => {
            let (spinner, elapsed) = match &state.phase {
                SshSessionPhase::Starting {
                    spinner_frame,
                    started_at,
                    ..
                } => {
                    const FRAMES: [&str; 8] = ["-", "\\", "|", "/", "-", "\\", "|", "/"];
                    let spinner = FRAMES[*spinner_frame % FRAMES.len()];
                    let elapsed = started_at.elapsed().as_secs_f32();
                    (spinner, elapsed)
                }
                _ => ("-", 0.0),
            };

            frame.render_widget(
                Paragraph::new(format!(
                    "{spinner} Connecting to {}\n\nPlease wait... ({elapsed:.1}s)",
                    state.title
                ))
                .alignment(Alignment::Center),
                area,
            );
        }
        SshSessionPhase::TrustPrompt { challenge, .. } => {
            frame.render_widget(
                Paragraph::new(render_vt100_text(&state.parser)).alignment(Alignment::Left),
                area,
            );
            render_trust_modal(frame, a, challenge);
        }
        SshSessionPhase::Running { .. } => {
            frame.render_widget(
                Paragraph::new(render_vt100_text(&state.parser)).alignment(Alignment::Left),
                area,
            );
        }
        SshSessionPhase::Error(error) => {
            frame.render_widget(
                Paragraph::new(error.clone()).alignment(Alignment::Center),
                area,
            );
        }
    }
}

fn render_trust_modal(
    frame: &mut Frame,
    app_area: Rect,
    challenge: &crate::ssh_client::TrustChallenge,
) {
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
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Char(c) => Some(c.to_string().into_bytes()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        _ => None,
    }
}
