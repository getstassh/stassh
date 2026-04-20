use backend::{AppState, DbEncryption, DbOpenStatus, VersionCheckStatus};
use base64::Engine;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use std::io::{self, Write};

use crate::{
    inputs::{handle_pasted_text, handle_text_input, handle_yes_no_input},
    navigation::{
        DashboardState, Screen, SettingsBackupAction, SettingsBackupField,
        SettingsBackupModalState, SettingsBackupRestoreStage, SettingsSecurityAction,
        SettingsSecurityField, SettingsSecurityModalState,
    },
    screens::AppEffect,
    ui::{
        accent_text, border, centered_rect_no_border, danger_text, line_with_caret, modal_block,
        muted_text, panel_alt_background, selected_border, soft_accent_text, text,
    },
};

const IDLE_TIMEOUT_STEP: u64 = 30;
const IDLE_TIMEOUT_MAX: u64 = 86_400;
const CONNECT_TIMEOUT_MAX: u64 = 60;

#[derive(Clone, Copy)]
enum SettingsRow {
    Telemetry,
    IdleTimeout,
    ConnectTimeout,
    EnableEncryption,
    ChangePassphrase,
    RemovePassphrase,
    CopyDbBlob,
    RestoreDbBlob,
}

pub(crate) fn handle_key(
    _app: &AppState,
    key: KeyEvent,
    state: &mut DashboardState,
) -> Option<AppEffect> {
    if let Some(modal) = &mut state.settings_backup_modal {
        return handle_backup_modal_key(key, modal);
    }

    if let Some(modal) = &mut state.settings_modal {
        return handle_modal_key(key, modal);
    }

    let interactive_rows = build_rows(_app);
    if interactive_rows.is_empty() {
        return None;
    }

    state.settings_selected_row = state
        .settings_selected_row
        .min(interactive_rows.len().saturating_sub(1));

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if state.settings_selected_row == 0 {
                state.settings_selected_row = interactive_rows.len().saturating_sub(1);
            } else {
                state.settings_selected_row = state.settings_selected_row.saturating_sub(1);
            }
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.settings_selected_row =
                (state.settings_selected_row + 1) % interactive_rows.len();
            None
        }
        KeyCode::Left | KeyCode::Char('h') => {
            let row = interactive_rows[state.settings_selected_row];
            apply_row_change(row, false)
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') | KeyCode::Enter => {
            let row = interactive_rows[state.settings_selected_row];
            apply_row_change(row, true)
        }
        _ => None,
    }
}

pub(crate) fn handle_paste(text: &str, state: &mut DashboardState) {
    let Some(modal) = &mut state.settings_backup_modal else {
        let Some(modal) = &mut state.settings_modal else {
            return;
        };

        match modal.focus {
            SettingsSecurityField::Current => {
                handle_pasted_text(&mut modal.current_passphrase, text)
            }
            SettingsSecurityField::New => handle_pasted_text(&mut modal.new_passphrase, text),
            SettingsSecurityField::Confirm => {
                handle_pasted_text(&mut modal.confirm_passphrase, text)
            }
            SettingsSecurityField::DangerConfirm => {}
        }
        return;
    };

    match modal.action {
        SettingsBackupAction::CopyDbBlob => {}
        SettingsBackupAction::RestoreDbBlob => match modal.restore_stage {
            SettingsBackupRestoreStage::Blob => handle_pasted_text(&mut modal.blob, text),
            SettingsBackupRestoreStage::Passphrase => {
                handle_pasted_text(&mut modal.passphrase, text)
            }
            SettingsBackupRestoreStage::Confirm => {}
        },
    }
}

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(13), Constraint::Min(6)])
        .split(area);

    render_controls_panel(frame, split[0], app, state);
    render_info_panel(frame, split[1], app);

    if let Some(modal) = &state.settings_backup_modal {
        render_backup_modal(frame, frame.area(), modal, app);
        return;
    }

    if let Some(modal) = &state.settings_modal {
        render_security_modal(frame, frame.area(), modal);
    }
}

pub(crate) fn footer_hint(state: &DashboardState) -> &'static str {
    if let Some(modal) = &state.settings_backup_modal {
        return match modal.action {
            SettingsBackupAction::CopyDbBlob => "Press C to copy | Esc/Enter close",
            SettingsBackupAction::RestoreDbBlob => match modal.restore_stage {
                SettingsBackupRestoreStage::Blob => {
                    "Paste backup blob | Enter continue | Esc cancel"
                }
                SettingsBackupRestoreStage::Passphrase => {
                    "Enter backup passphrase | Enter verify | Esc cancel"
                }
                SettingsBackupRestoreStage::Confirm => {
                    "Left/Right choose NO/YES | Enter restore | Esc cancel"
                }
            },
        };
    }

    if state.settings_modal.is_some() {
        return "Tab move | Enter submit/next | Left/Right confirm | Esc cancel";
    }

    "Up/Down/j/k select | Left/Right/h/l edit | Enter action | Ctrl+Q quick switch | Esc exit"
}

fn render_controls_panel(frame: &mut Frame, area: Rect, app: &AppState, state: &DashboardState) {
    let block = Block::default()
        .title(Span::styled(" Controls ", soft_accent_text()))
        .borders(Borders::ALL)
        .border_style(border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = build_rows(app);
    let selected_idx = state
        .settings_selected_row
        .min(rows.len().saturating_sub(1));
    let mut lines = Vec::new();

    for (idx, row) in rows.iter().enumerate() {
        let is_selected = idx == selected_idx;
        lines.push(render_row_label(*row, app, is_selected));
    }

    frame.render_widget(Paragraph::new(lines).style(text()), inner);
}

fn render_info_panel(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default()
        .title(Span::styled(" Runtime ", soft_accent_text()))
        .borders(Borders::ALL)
        .border_style(border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let encryption_mode = match app.config.db_encryption {
        Some(DbEncryption::Passphrase) => "passphrase",
        Some(DbEncryption::None) => "none",
        None => "unset",
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Database encryption: ", muted_text()),
            Span::styled(encryption_mode, text()),
        ]),
        Line::from(vec![
            Span::styled("Database backups: ", muted_text()),
            Span::styled(
                match app.backup_count() {
                    Some(count) => format!(
                        "{} (auto, keep {})",
                        count,
                        app.automatic_backup_retention_count()
                    ),
                    None => "unknown (auto, keep 14)".to_string(),
                },
                text(),
            ),
        ]),
        Line::from(vec![
            Span::styled("App version: ", muted_text()),
            Span::styled(env!("CARGO_PKG_VERSION"), text()),
        ]),
        Line::from(vec![
            Span::styled("Update status: ", muted_text()),
            Span::styled(describe_update_status(&app.version_status), text()),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_row_label(row: SettingsRow, app: &AppState, selected: bool) -> Line<'static> {
    let prefix = if selected { ">" } else { " " };

    let (label, value, action_like) = match row {
        SettingsRow::Telemetry => (
            "Anonymous telemetry",
            if app.config.enable_telemetry == Some(true) {
                "ON".to_string()
            } else {
                "OFF".to_string()
            },
            false,
        ),
        SettingsRow::IdleTimeout => (
            "SSH idle timeout",
            format!("{}s", app.config.ssh_idle_timeout_seconds),
            false,
        ),
        SettingsRow::ConnectTimeout => (
            "SSH connect timeout",
            format!("{}s", app.config.ssh_connect_timeout_seconds),
            false,
        ),
        SettingsRow::EnableEncryption => ("Enable DB passphrase", "open modal".to_string(), true),
        SettingsRow::ChangePassphrase => ("Change DB passphrase", "open modal".to_string(), true),
        SettingsRow::RemovePassphrase => ("Remove DB passphrase", "open modal".to_string(), true),
        SettingsRow::CopyDbBlob => ("Create DB backup blob", "open modal".to_string(), true),
        SettingsRow::RestoreDbBlob => (
            "Restore DB from backup blob",
            "open modal".to_string(),
            true,
        ),
    };

    let value_style = if action_like {
        if matches!(row, SettingsRow::RemovePassphrase) {
            danger_text()
        } else {
            accent_text()
        }
    } else {
        text()
    };

    Line::from(vec![
        Span::styled(
            format!("{prefix} {label:<24} "),
            if selected {
                accent_text()
            } else {
                muted_text()
            },
        ),
        Span::styled(value, value_style),
    ])
}

fn build_rows(app: &AppState) -> Vec<SettingsRow> {
    let mut rows = vec![
        SettingsRow::Telemetry,
        SettingsRow::IdleTimeout,
        SettingsRow::ConnectTimeout,
    ];

    match app.config.db_encryption {
        Some(DbEncryption::Passphrase) => {
            rows.push(SettingsRow::ChangePassphrase);
            rows.push(SettingsRow::RemovePassphrase);
        }
        _ => rows.push(SettingsRow::EnableEncryption),
    }

    rows.push(SettingsRow::CopyDbBlob);
    rows.push(SettingsRow::RestoreDbBlob);

    rows
}

fn apply_row_change(row: SettingsRow, positive: bool) -> Option<AppEffect> {
    match row {
        SettingsRow::Telemetry => Some(Box::new(move |app| {
            if positive {
                let enabled = app.config.enable_telemetry == Some(true);
                app.config.enable_telemetry = Some(!enabled);
            } else {
                app.config.enable_telemetry = Some(false);
            }
            let _ = app.save_config();
        })),
        SettingsRow::IdleTimeout => Some(Box::new(move |app| {
            let current = app.config.ssh_idle_timeout_seconds;
            let next = if positive {
                (current.saturating_add(IDLE_TIMEOUT_STEP)).min(IDLE_TIMEOUT_MAX)
            } else {
                current.saturating_sub(IDLE_TIMEOUT_STEP).max(1)
            };
            app.config.ssh_idle_timeout_seconds = next;
            let _ = app.save_config();
        })),
        SettingsRow::ConnectTimeout => Some(Box::new(move |app| {
            let current = app.config.ssh_connect_timeout_seconds;
            let next = if positive {
                (current.saturating_add(1)).min(CONNECT_TIMEOUT_MAX)
            } else {
                current.saturating_sub(1).max(1)
            };
            app.config.ssh_connect_timeout_seconds = next;
            let _ = app.save_config();
        })),
        SettingsRow::EnableEncryption => Some(Box::new(|app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.settings_modal = Some(SettingsSecurityModalState::for_action(
                    SettingsSecurityAction::EnableEncryption,
                ));
            }
        })),
        SettingsRow::ChangePassphrase => Some(Box::new(|app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.settings_modal = Some(SettingsSecurityModalState::for_action(
                    SettingsSecurityAction::ChangePassphrase,
                ));
            }
        })),
        SettingsRow::RemovePassphrase => Some(Box::new(|app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.settings_modal = Some(SettingsSecurityModalState::for_action(
                    SettingsSecurityAction::RemovePassphrase,
                ));
            }
        })),
        SettingsRow::CopyDbBlob => Some(Box::new(|app| {
            let result = app.export_db_blob();
            if let Screen::Dashboard { state } = &mut app.screen {
                match result {
                    Ok(blob) => {
                        let mut modal =
                            SettingsBackupModalState::for_action(SettingsBackupAction::CopyDbBlob);
                        let encoded = base64::engine::general_purpose::STANDARD.encode(blob);
                        modal.blob.text = wrap_token_for_display(&encoded, 96);
                        modal.blob.caret_position = modal.blob.text.len();
                        state.settings_backup_modal = Some(modal);
                    }
                    Err(err) => {
                        state.last_status = Some(format!("Failed to create backup blob: {}", err));
                    }
                }
            }
        })),
        SettingsRow::RestoreDbBlob => Some(Box::new(|app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.settings_backup_modal = Some(SettingsBackupModalState::for_action(
                    SettingsBackupAction::RestoreDbBlob,
                ));
            }
        })),
    }
}

fn handle_backup_modal_key(
    key: KeyEvent,
    modal: &mut SettingsBackupModalState,
) -> Option<AppEffect> {
    if key.code == KeyCode::Esc {
        return Some(Box::new(|app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.settings_backup_modal = None;
            }
        }));
    }

    match modal.action {
        SettingsBackupAction::CopyDbBlob => {
            if key.code == KeyCode::Char('c') {
                let token = modal.blob.text.clone();
                return Some(Box::new(move |app| {
                    let result = copy_text_to_clipboard_osc52(&token);
                    if let Screen::Dashboard { state } = &mut app.screen {
                        if let Some(modal) = &mut state.settings_backup_modal {
                            modal.copy_feedback = match result {
                                Ok(()) => Some("Copied to clipboard".to_string()),
                                Err(err) => Some(format!("Copy failed: {}", err)),
                            };
                        }
                    }
                }));
            }

            if key.code == KeyCode::Enter {
                return Some(Box::new(|app| {
                    if let Screen::Dashboard { state } = &mut app.screen {
                        state.settings_backup_modal = None;
                    }
                }));
            }
            None
        }
        SettingsBackupAction::RestoreDbBlob => {
            match modal.restore_stage {
                SettingsBackupRestoreStage::Blob => {
                    if key.code == KeyCode::Enter {
                        return advance_restore_stage(modal.clone());
                    }
                    let _ = handle_text_input(&mut modal.blob, key);
                    modal.error = None;
                }
                SettingsBackupRestoreStage::Passphrase => {
                    if key.code == KeyCode::Enter {
                        return advance_restore_stage(modal.clone());
                    }
                    let _ = handle_text_input(&mut modal.passphrase, key);
                    modal.error = None;
                }
                SettingsBackupRestoreStage::Confirm => {
                    let _ = handle_yes_no_input(&mut modal.danger_confirm, key.code);
                    if key.code == KeyCode::Enter {
                        return advance_restore_stage(modal.clone());
                    }
                    modal.error = None;
                }
            }
            None
        }
    }
}

fn advance_restore_stage(modal: SettingsBackupModalState) -> Option<AppEffect> {
    let blob_token = modal.blob.text.clone();
    let passphrase = modal.passphrase.text.clone();
    let confirmed = modal.danger_confirm.is_yes();
    let stage = modal.restore_stage;

    Some(Box::new(move |app| {
        let mut next = modal.clone();
        next.error = None;

        let result = (|| -> anyhow::Result<()> {
            let token = compact_token(&blob_token);
            if token.is_empty() {
                anyhow::bail!("Backup blob cannot be empty");
            }

            let decoded = base64::engine::general_purpose::STANDARD
                .decode(token)
                .map_err(|_| anyhow::anyhow!("Backup blob is not valid base64"))?;

            match stage {
                SettingsBackupRestoreStage::Blob => {
                    let blob_status = app.inspect_db_blob_open_status(&decoded)?;
                    next.requires_passphrase = blob_status == DbOpenStatus::PassphraseRequired;
                    next.restore_stage = if next.requires_passphrase {
                        SettingsBackupRestoreStage::Passphrase
                    } else {
                        SettingsBackupRestoreStage::Confirm
                    };
                    next.focus = if next.requires_passphrase {
                        SettingsBackupField::Passphrase
                    } else {
                        SettingsBackupField::DangerConfirm
                    };
                    next.danger_confirm.selected = false;
                }
                SettingsBackupRestoreStage::Passphrase => {
                    if passphrase.trim().is_empty() {
                        anyhow::bail!("Backup is encrypted. Enter its passphrase to continue");
                    }
                    app.validate_db_blob_passphrase(&decoded, &passphrase)?;
                    next.restore_stage = SettingsBackupRestoreStage::Confirm;
                    next.focus = SettingsBackupField::DangerConfirm;
                    next.danger_confirm.selected = false;
                }
                SettingsBackupRestoreStage::Confirm => {
                    if !confirmed {
                        anyhow::bail!("Set confirmation to YES to restore");
                    }

                    if next.requires_passphrase {
                        app.restore_db_from_blob(&decoded, Some(passphrase.as_str()))?;
                    } else {
                        app.restore_db_from_blob(&decoded, None)?;
                    }
                }
            }

            Ok(())
        })();

        if let Screen::Dashboard { state } = &mut app.screen {
            match result {
                Ok(()) => {
                    if stage == SettingsBackupRestoreStage::Confirm {
                        state.settings_backup_modal = None;
                        state.last_status =
                            Some("Database restored from backup blob. Restarting...".to_string());
                        app.request_restart_and_exit();
                    } else {
                        state.settings_backup_modal = Some(next);
                    }
                }
                Err(err) => {
                    next.error = Some(err.to_string());
                    state.settings_backup_modal = Some(next);
                }
            }
        }
    }))
}

fn handle_modal_key(key: KeyEvent, modal: &mut SettingsSecurityModalState) -> Option<AppEffect> {
    if key.code == KeyCode::Esc {
        return Some(Box::new(|app| {
            if let Screen::Dashboard { state } = &mut app.screen {
                state.settings_modal = None;
            }
        }));
    }

    if key.code == KeyCode::Tab || key.code == KeyCode::Down {
        modal.focus = next_modal_field(modal);
        modal.error = None;
        return None;
    }

    if key.code == KeyCode::BackTab || key.code == KeyCode::Up {
        modal.focus = prev_modal_field(modal);
        modal.error = None;
        return None;
    }

    if modal.focus == SettingsSecurityField::DangerConfirm {
        if handle_yes_no_input(&mut modal.danger_confirm, key.code).is_some() {
            return submit_security_modal(modal.clone());
        }
        return None;
    }

    if key.code == KeyCode::Enter {
        if is_submit_focus(modal) {
            return submit_security_modal(modal.clone());
        }
        modal.focus = next_modal_field(modal);
        modal.error = None;
        return None;
    }

    let target = match modal.focus {
        SettingsSecurityField::Current => &mut modal.current_passphrase,
        SettingsSecurityField::New => &mut modal.new_passphrase,
        SettingsSecurityField::Confirm => &mut modal.confirm_passphrase,
        SettingsSecurityField::DangerConfirm => return None,
    };
    let _ = handle_text_input(target, key);
    modal.error = None;
    None
}

fn submit_security_modal(modal: SettingsSecurityModalState) -> Option<AppEffect> {
    let current = modal.current_passphrase.text.clone();
    let new_pass = modal.new_passphrase.text.clone();
    let confirm = modal.confirm_passphrase.text.clone();
    let danger_confirmed = modal.danger_confirm.is_yes();
    let action = modal.action;

    Some(Box::new(move |app| {
        let result = match action {
            SettingsSecurityAction::EnableEncryption => {
                if new_pass.trim().is_empty() {
                    Err(anyhow::anyhow!("Passphrase cannot be empty"))
                } else if new_pass != confirm {
                    Err(anyhow::anyhow!("Passphrase confirmation does not match"))
                } else {
                    app.enable_encryption_with_passphrase(&new_pass)
                }
            }
            SettingsSecurityAction::ChangePassphrase => {
                if current.trim().is_empty() {
                    Err(anyhow::anyhow!("Current passphrase is required"))
                } else if new_pass.trim().is_empty() {
                    Err(anyhow::anyhow!("New passphrase cannot be empty"))
                } else if new_pass != confirm {
                    Err(anyhow::anyhow!(
                        "New passphrase confirmation does not match"
                    ))
                } else {
                    app.change_db_passphrase(&current, &new_pass)
                }
            }
            SettingsSecurityAction::RemovePassphrase => {
                if current.trim().is_empty() {
                    Err(anyhow::anyhow!("Current passphrase is required"))
                } else if !danger_confirmed {
                    Err(anyhow::anyhow!("Please confirm passphrase removal"))
                } else {
                    app.remove_db_passphrase(&current)
                }
            }
        };

        if let Screen::Dashboard { state } = &mut app.screen {
            match result {
                Ok(()) => {
                    state.settings_modal = None;
                    state.last_status = Some(match action {
                        SettingsSecurityAction::EnableEncryption => {
                            "Database encryption enabled".to_string()
                        }
                        SettingsSecurityAction::ChangePassphrase => {
                            "Database passphrase updated".to_string()
                        }
                        SettingsSecurityAction::RemovePassphrase => {
                            "Database passphrase removed".to_string()
                        }
                    });
                }
                Err(err) => {
                    let mut next = SettingsSecurityModalState::for_action(action);
                    next.current_passphrase.text = current.clone();
                    next.current_passphrase.caret_position = next.current_passphrase.text.len();
                    next.new_passphrase.text = new_pass.clone();
                    next.new_passphrase.caret_position = next.new_passphrase.text.len();
                    next.confirm_passphrase.text = confirm.clone();
                    next.confirm_passphrase.caret_position = next.confirm_passphrase.text.len();
                    next.danger_confirm.selected = danger_confirmed;
                    next.error = Some(err.to_string());
                    state.settings_modal = Some(next);
                }
            }
        }
    }))
}

fn next_modal_field(modal: &SettingsSecurityModalState) -> SettingsSecurityField {
    let fields = active_fields(modal.action);
    let idx = fields
        .iter()
        .position(|field| *field == modal.focus)
        .unwrap_or(0);
    fields[(idx + 1) % fields.len()]
}

fn prev_modal_field(modal: &SettingsSecurityModalState) -> SettingsSecurityField {
    let fields = active_fields(modal.action);
    let idx = fields
        .iter()
        .position(|field| *field == modal.focus)
        .unwrap_or(0);
    if idx == 0 {
        fields[fields.len().saturating_sub(1)]
    } else {
        fields[idx - 1]
    }
}

fn is_submit_focus(modal: &SettingsSecurityModalState) -> bool {
    let fields = active_fields(modal.action);
    fields
        .last()
        .is_some_and(|last_field| *last_field == modal.focus)
}

fn active_fields(action: SettingsSecurityAction) -> &'static [SettingsSecurityField] {
    match action {
        SettingsSecurityAction::EnableEncryption => {
            &[SettingsSecurityField::New, SettingsSecurityField::Confirm]
        }
        SettingsSecurityAction::ChangePassphrase => &[
            SettingsSecurityField::Current,
            SettingsSecurityField::New,
            SettingsSecurityField::Confirm,
        ],
        SettingsSecurityAction::RemovePassphrase => &[
            SettingsSecurityField::Current,
            SettingsSecurityField::DangerConfirm,
        ],
    }
}

fn render_backup_modal(
    frame: &mut Frame,
    app_area: Rect,
    modal: &SettingsBackupModalState,
    app: &AppState,
) {
    match modal.action {
        SettingsBackupAction::CopyDbBlob => {
            let popup_area =
                centered_rect_no_border((app_area.width.saturating_sub(6)).min(110), 18, app_area);
            frame.render_widget(Clear, popup_area);

            let block = modal_block("DB backup blob", "Press C to copy | Enter/Esc close");
            let inner = block.inner(popup_area);
            frame.render_widget(block, popup_area);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(8),
                ])
                .split(inner);

            frame.render_widget(
                Paragraph::new("This token is a full snapshot of your database. Keep it private.")
                    .style(muted_text()),
                rows[0],
            );

            let password_note =
                if matches!(app.config.db_encryption, Some(DbEncryption::Passphrase)) {
                    "Your DB is encrypted. Remember the current DB password for restore."
                } else {
                    "If you later enable DB encryption, remember the password used for that backup."
                };
            frame.render_widget(Paragraph::new(password_note).style(muted_text()), rows[1]);
            frame.render_widget(
                Paragraph::new("Anyone with this token can restore your DB contents.")
                    .style(danger_text()),
                rows[2],
            );

            let copy_feedback = modal.copy_feedback.as_deref().unwrap_or("Not copied yet");
            let copy_style = if copy_feedback.starts_with("Copied") {
                crate::ui::success_text()
            } else if copy_feedback.starts_with("Copy failed") {
                danger_text()
            } else {
                muted_text()
            };
            frame.render_widget(
                Paragraph::new(format!("Copy status: {copy_feedback}")).style(copy_style),
                rows[3],
            );

            let blob_block = Block::default()
                .title(Span::styled(" Backup blob (base64) ", accent_text()))
                .borders(Borders::ALL)
                .border_style(border());
            let blob_inner = blob_block.inner(rows[4]);
            frame.render_widget(blob_block, rows[4]);
            frame.render_widget(
                Paragraph::new(modal.blob.text.clone())
                    .style(text())
                    .wrap(Wrap { trim: false }),
                blob_inner,
            );
        }
        SettingsBackupAction::RestoreDbBlob => {
            let popup_area =
                centered_rect_no_border((app_area.width.saturating_sub(8)).min(96), 20, app_area);
            frame.render_widget(Clear, popup_area);

            let footer = match modal.restore_stage {
                SettingsBackupRestoreStage::Blob => "Step 1/3: paste blob, then Enter | Esc cancel",
                SettingsBackupRestoreStage::Passphrase => {
                    "Step 2/3: enter passphrase, then Enter | Esc cancel"
                }
                SettingsBackupRestoreStage::Confirm => {
                    "Step 3/3: choose YES/NO, then Enter | Esc cancel"
                }
            };
            let block = modal_block("Restore DB from backup blob", footer);
            let inner = block.inner(popup_area);
            frame.render_widget(block, popup_area);

            match modal.restore_stage {
                SettingsBackupRestoreStage::Blob => {
                    let rows = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Min(10),
                            Constraint::Length(1),
                        ])
                        .split(inner);

                    frame.render_widget(
                        Paragraph::new("Paste your backup blob token to continue.")
                            .style(muted_text()),
                        rows[0],
                    );

                    render_blob_input(
                        frame,
                        rows[1],
                        &modal.blob,
                        modal.focus == SettingsBackupField::Blob,
                    );

                    if let Some(error) = &modal.error {
                        frame.render_widget(
                            Paragraph::new(error.clone()).style(danger_text()),
                            rows[2],
                        );
                    }
                }
                SettingsBackupRestoreStage::Passphrase => {
                    let rows = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Length(3),
                            Constraint::Length(1),
                            Constraint::Min(0),
                        ])
                        .split(inner);

                    frame.render_widget(
                        Paragraph::new(
                            "Backup is encrypted. Enter the backup passphrase to verify it.",
                        )
                        .style(muted_text()),
                        rows[0],
                    );

                    render_password_prompt_input(
                        frame,
                        rows[1],
                        &modal.passphrase,
                        modal.focus == SettingsBackupField::Passphrase,
                    );

                    if let Some(error) = &modal.error {
                        frame.render_widget(
                            Paragraph::new(error.clone()).style(danger_text()),
                            rows[2],
                        );
                    }
                }
                SettingsBackupRestoreStage::Confirm => {
                    let rows = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Length(3),
                            Constraint::Length(1),
                            Constraint::Min(0),
                        ])
                        .split(inner);

                    frame.render_widget(
                        Paragraph::new(
                            "Ready to restore. This overwrites your current DB and restarts Stassh.",
                        )
                        .style(muted_text()),
                        rows[0],
                    );

                    render_restore_confirm_toggle(
                        frame,
                        rows[1],
                        modal.danger_confirm.is_yes(),
                        modal.focus == SettingsBackupField::DangerConfirm,
                    );

                    if let Some(error) = &modal.error {
                        frame.render_widget(
                            Paragraph::new(error.clone()).style(danger_text()),
                            rows[2],
                        );
                    }
                }
            }
        }
    }
}

fn render_blob_input(
    frame: &mut Frame,
    area: Rect,
    value: &crate::navigation::StringState,
    selected: bool,
) {
    let block = Block::default()
        .title(Span::styled(
            " Backup blob (base64) ",
            if selected {
                accent_text()
            } else {
                muted_text()
            },
        ))
        .borders(Borders::ALL)
        .border_style(if selected {
            selected_border()
        } else {
            border()
        });

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(value.text.clone())
            .style(text())
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_password_prompt_input(
    frame: &mut Frame,
    area: Rect,
    value: &crate::navigation::StringState,
    selected: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if selected {
            selected_border()
        } else {
            border()
        })
        .style(panel_alt_background());

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(line_with_caret(value)).style(accent_text()),
        inner,
    );
}

fn render_restore_confirm_toggle(frame: &mut Frame, area: Rect, yes: bool, selected: bool) {
    let left = if yes {
        Span::styled("  NO  ", muted_text())
    } else {
        Span::styled("  NO  ", danger_text())
    };
    let right = if yes {
        Span::styled("  YES  ", danger_text())
    } else {
        Span::styled("  YES  ", muted_text())
    };

    let pointer = if selected { ">" } else { " " };
    let line = Line::from(vec![
        Span::styled(format!("{pointer} Confirm restore: ["), muted_text()),
        left,
        Span::styled("] [", muted_text()),
        right,
        Span::styled("]", muted_text()),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn render_security_modal(frame: &mut Frame, app_area: Rect, modal: &SettingsSecurityModalState) {
    let popup_area =
        centered_rect_no_border((app_area.width.saturating_sub(8)).min(88), 14, app_area);
    frame.render_widget(Clear, popup_area);

    let (title, footer) = match modal.action {
        SettingsSecurityAction::EnableEncryption => (
            "Enable DB passphrase",
            "Tab move | Enter next/submit | Esc cancel",
        ),
        SettingsSecurityAction::ChangePassphrase => (
            "Change DB passphrase",
            "Tab move | Enter next/submit | Esc cancel",
        ),
        SettingsSecurityAction::RemovePassphrase => (
            "Remove DB passphrase",
            "Left/Right confirm removal | Enter submit | Esc cancel",
        ),
    };

    let block = modal_block(title, footer);
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    match modal.action {
        SettingsSecurityAction::EnableEncryption => {
            render_passphrase_input(
                frame,
                rows[0],
                "New passphrase",
                &modal.new_passphrase,
                modal.focus == SettingsSecurityField::New,
            );
            render_passphrase_input(
                frame,
                rows[1],
                "Confirm passphrase",
                &modal.confirm_passphrase,
                modal.focus == SettingsSecurityField::Confirm,
            );
        }
        SettingsSecurityAction::ChangePassphrase => {
            render_passphrase_input(
                frame,
                rows[0],
                "Current passphrase",
                &modal.current_passphrase,
                modal.focus == SettingsSecurityField::Current,
            );
            render_passphrase_input(
                frame,
                rows[1],
                "New passphrase",
                &modal.new_passphrase,
                modal.focus == SettingsSecurityField::New,
            );
            render_passphrase_input(
                frame,
                rows[2],
                "Confirm passphrase",
                &modal.confirm_passphrase,
                modal.focus == SettingsSecurityField::Confirm,
            );
        }
        SettingsSecurityAction::RemovePassphrase => {
            render_passphrase_input(
                frame,
                rows[0],
                "Current passphrase",
                &modal.current_passphrase,
                modal.focus == SettingsSecurityField::Current,
            );

            let confirm_line = format!(
                "{} {}",
                if modal.focus == SettingsSecurityField::DangerConfirm {
                    ">"
                } else {
                    " "
                },
                if modal.danger_confirm.is_yes() {
                    "Confirm removal: YES"
                } else {
                    "Confirm removal: NO"
                }
            );
            frame.render_widget(
                Paragraph::new(confirm_line).style(if modal.danger_confirm.is_yes() {
                    danger_text()
                } else {
                    muted_text()
                }),
                rows[1],
            );
            frame.render_widget(
                Paragraph::new("This will decrypt all the data, including ssh keys and passwords.")
                    .style(muted_text()),
                rows[2],
            );
        }
    }

    if let Some(error) = &modal.error {
        frame.render_widget(Paragraph::new(error.clone()).style(danger_text()), rows[4]);
    }
}

fn render_passphrase_input(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &crate::navigation::StringState,
    selected: bool,
) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {label} "),
            if selected {
                accent_text()
            } else {
                muted_text()
            },
        ))
        .borders(Borders::ALL)
        .border_style(if selected {
            selected_border()
        } else {
            border()
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = if selected {
        line_with_caret(value)
    } else {
        Line::from(value.visible_text())
    };
    frame.render_widget(Paragraph::new(line).style(text()), inner);
}

fn describe_update_status(status: &VersionCheckStatus) -> String {
    match status {
        VersionCheckStatus::Idle => "idle (update check pending)".to_string(),
        VersionCheckStatus::Checking => "checking for updates...".to_string(),
        VersionCheckStatus::UpToDate { current } => {
            format!("up to date ({})", current)
        }
        VersionCheckStatus::UpdateAvailable { latest, url, .. } => {
            format!("new release {} available ({})", latest, url)
        }
        VersionCheckStatus::Error(err) => format!("error checking updates: {}", err),
    }
}

fn copy_text_to_clipboard_osc52(text: &str) -> anyhow::Result<()> {
    let payload = compact_token(text);
    if payload.is_empty() {
        anyhow::bail!("backup token is empty")
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
    let sequence = format!("\u{1b}]52;c;{encoded}\u{7}");

    let mut stdout = io::stdout();
    stdout
        .write_all(sequence.as_bytes())
        .map_err(|err| anyhow::anyhow!("failed to write OSC52 sequence: {err}"))?;
    stdout
        .flush()
        .map_err(|err| anyhow::anyhow!("failed to flush OSC52 sequence: {err}"))?;

    Ok(())
}

fn compact_token(value: &str) -> String {
    value.chars().filter(|c| !c.is_whitespace()).collect()
}

fn wrap_token_for_display(value: &str, width: usize) -> String {
    if width == 0 {
        return value.to_string();
    }

    let mut output = String::with_capacity(value.len() + value.len() / width + 1);
    let mut count = 0usize;

    for ch in value.chars() {
        output.push(ch);
        count += 1;
        if count >= width {
            output.push('\n');
            count = 0;
        }
    }

    if output.ends_with('\n') {
        output.pop();
    }

    output
}
