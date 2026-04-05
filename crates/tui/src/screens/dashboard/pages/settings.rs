use backend::{AppState, VersionCheckStatus};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::Paragraph,
};

use crate::ui::text;

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    frame.render_widget(
        Paragraph::new(format!(
            "SETTINGS\n\nTelemetry enabled: {:?}\nDatabase encryption: {:?}\nDebug panel enabled: {}\nSSH idle timeout (seconds): {}\n\nApp version: {}\nUpdate status: {}\n\nUse config files or future settings editor to modify these values.",
            app.config.enable_telemetry,
            app.config.db_encryption,
            app.config.show_debug_panel,
            app.config.ssh_idle_timeout_seconds,
            env!("CARGO_PKG_VERSION"),
            describe_update_status(&app.version_status),
        ))
        .alignment(Alignment::Left)
        .style(text()),
        area,
    );
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
