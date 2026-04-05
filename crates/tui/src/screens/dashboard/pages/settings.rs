use backend::AppState;
use ratatui::{
    layout::{Alignment, Rect},
    widgets::Paragraph,
    Frame,
};

pub(crate) fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    frame.render_widget(
        Paragraph::new(format!(
            "Settings\n\n- Telemetry enabled: {:?}\n- Database encryption: {:?}\n- Sidebar visible: {:?}\n- SSH idle timeout (seconds): {}",
            app.config.enable_telemetry,
            app.config.db_encryption,
            app.config.show_sidebar,
            app.config.ssh_idle_timeout_seconds,
        ))
        .alignment(Alignment::Left),
        area,
    );
}
