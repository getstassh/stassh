use backend::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::Paragraph,
};

use crate::ui::text;

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    frame.render_widget(
        Paragraph::new(format!(
            "SETTINGS\n\nTelemetry enabled: {:?}\nDatabase encryption: {:?}\nDebug panel enabled: {}\nSSH idle timeout (seconds): {}\n\nUse config files or future settings editor to modify these values.",
            app.config.enable_telemetry,
            app.config.db_encryption,
            app.config.show_debug_panel,
            app.config.ssh_idle_timeout_seconds,
        ))
        .alignment(Alignment::Left)
        .style(text()),
        area,
    );
}
