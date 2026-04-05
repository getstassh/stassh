use backend::UpdateInstallStatus;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Gauge, Paragraph},
};

use crate::{
    navigation::{Screen, StartupUpdatePhase, StartupUpdateState},
    screens::{AppEffect, ScreenHandler, components::page_with_logo},
    ui::{accent_text, muted_text, success_text, text, warning_text},
};

pub(crate) static HANDLER: ScreenHandler<StartupUpdateState> = ScreenHandler {
    matches: |s| {
        matches!(
            s,
            Screen::StartupUpdateCheck { .. } | Screen::StartupUpdatePrompt { .. }
        )
    },
    get: |s| match s {
        Screen::StartupUpdateCheck { state } | Screen::StartupUpdatePrompt { state } => Some(state),
        _ => None,
    },
    get_mut: |s| match s {
        Screen::StartupUpdateCheck { state } | Screen::StartupUpdatePrompt { state } => Some(state),
        _ => None,
    },
    render: ui,
    handle_key,
    handle_paste: |_, _, _| None,
    handle_resize: |_, _, _, _| None,
    handle_tick,
};

fn handle_key(
    _: &backend::AppState,
    key: KeyEvent,
    state: &mut StartupUpdateState,
) -> Option<AppEffect> {
    match state.phase {
        StartupUpdatePhase::Prompt => match key.code {
            KeyCode::Enter => Some(Box::new(|app| app.start_update_install())),
            KeyCode::Char('s') | KeyCode::Esc => Some(Box::new(|app| app.skip_update_gate())),
            _ => None,
        },
        StartupUpdatePhase::Done | StartupUpdatePhase::Failed => match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('s') => {
                Some(Box::new(|app| app.skip_update_gate()))
            }
            _ => None,
        },
        _ => None,
    }
}

fn handle_tick(_: &backend::AppState, state: &mut StartupUpdateState) -> Option<AppEffect> {
    state.spinner_frame = state.spinner_frame.wrapping_add(1);

    if let Some(rx) = &state.install_receiver {
        while let Ok(status) = rx.try_recv() {
            match status {
                UpdateInstallStatus::Downloading { downloaded, total } => {
                    state.phase = StartupUpdatePhase::Downloading;
                    state.downloaded = downloaded;
                    state.total = total;
                }
                UpdateInstallStatus::Verifying => state.phase = StartupUpdatePhase::Verifying,
                UpdateInstallStatus::Installing => state.phase = StartupUpdatePhase::Installing,
                UpdateInstallStatus::Done => {
                    state.phase = StartupUpdatePhase::Done;
                    return Some(Box::new(|app| app.request_restart_and_exit()));
                }
                UpdateInstallStatus::Failed(err) => {
                    state.phase = StartupUpdatePhase::Failed;
                    state.message = Some(err);
                }
            }
        }
    }

    None
}

fn ui(frame: &mut Frame, _app: &backend::AppState, state: &StartupUpdateState) {
    let area = page_with_logo(
        frame,
        frame.area(),
        crate::screens::components::LogoType::Simple,
        "Update available",
        "Enter install | S skip this launch",
    );

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let spinner = ["-", "\\", "|", "/"][state.spinner_frame % 4];
    let title = match state.phase {
        StartupUpdatePhase::Checking => format!("{spinner} Checking for updates..."),
        StartupUpdatePhase::Prompt => "New version found".to_string(),
        StartupUpdatePhase::Downloading => format!("{spinner} Downloading update..."),
        StartupUpdatePhase::Verifying => format!("{spinner} Verifying download..."),
        StartupUpdatePhase::Installing => format!("{spinner} Installing update..."),
        StartupUpdatePhase::Done => "Update installed".to_string(),
        StartupUpdatePhase::Failed => "Update failed".to_string(),
    };

    frame.render_widget(
        Paragraph::new(title)
            .alignment(Alignment::Center)
            .style(accent_text()),
        split[0],
    );

    match state.phase {
        StartupUpdatePhase::Prompt => {
            frame.render_widget(
                Paragraph::new(format!(
                    "Current: {}\nLatest: {}\n{}",
                    state.current_version,
                    state.latest_version.clone().unwrap_or_default(),
                    state.release_url.clone().unwrap_or_default(),
                ))
                .alignment(Alignment::Center)
                .style(text()),
                split[1],
            );
            frame.render_widget(
                Paragraph::new("Press Enter to install or S to skip this launch")
                    .alignment(Alignment::Center)
                    .style(muted_text()),
                split[2],
            );
        }
        StartupUpdatePhase::Downloading => {
            let ratio = match state.total {
                Some(total) if total > 0 => {
                    (state.downloaded as f64 / total as f64).clamp(0.0, 1.0)
                }
                _ => 0.0,
            };
            frame.render_widget(
                Gauge::default()
                    .ratio(ratio)
                    .label(match state.total {
                        Some(total) => format!("{} / {} bytes", state.downloaded, total),
                        None => format!("{} bytes downloaded", state.downloaded),
                    })
                    .style(success_text()),
                split[1],
            );
        }
        StartupUpdatePhase::Verifying => {
            frame.render_widget(
                Paragraph::new("Checking checksum...")
                    .alignment(Alignment::Center)
                    .style(text()),
                split[1],
            );
        }
        StartupUpdatePhase::Installing => {
            frame.render_widget(
                Paragraph::new("Replacing current binary...")
                    .alignment(Alignment::Center)
                    .style(text()),
                split[1],
            );
        }
        StartupUpdatePhase::Done => {
            frame.render_widget(
                Paragraph::new("Restart the app to use the new version.")
                    .alignment(Alignment::Center)
                    .style(success_text()),
                split[1],
            );
        }
        StartupUpdatePhase::Failed => {
            frame.render_widget(
                Paragraph::new(
                    state
                        .message
                        .clone()
                        .unwrap_or_else(|| "Update unavailable".to_string()),
                )
                .alignment(Alignment::Center)
                .style(warning_text()),
                split[1],
            );
            frame.render_widget(
                Paragraph::new("Press S to continue without updating")
                    .alignment(Alignment::Center)
                    .style(muted_text()),
                split[2],
            );
        }
        StartupUpdatePhase::Checking => {
            frame.render_widget(
                Paragraph::new("Contacting GitHub releases...")
                    .alignment(Alignment::Center)
                    .style(text()),
                split[1],
            );
        }
    }
}
