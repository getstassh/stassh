use ratatui::style::{Color, Modifier, Style};

const BG: Color = Color::Rgb(16, 20, 24);
const PANEL: Color = Color::Rgb(22, 27, 33);
const PANEL_ALT: Color = Color::Rgb(28, 34, 41);
const BORDER: Color = Color::Rgb(88, 102, 116);
pub(crate) const TEXT: Color = Color::Rgb(226, 217, 202);
const MUTED: Color = Color::Rgb(150, 145, 136);
const ACCENT: Color = Color::Rgb(227, 144, 62);
const ACCENT_SOFT: Color = Color::Rgb(109, 166, 174);
const SUCCESS: Color = Color::Rgb(114, 179, 134);
const DANGER: Color = Color::Rgb(210, 99, 81);
const WARNING: Color = Color::Rgb(229, 184, 93);

pub(crate) fn app_background() -> Style {
    Style::default().bg(BG)
}

pub(crate) fn panel_background() -> Style {
    Style::default().bg(PANEL)
}

pub(crate) fn panel_alt_background() -> Style {
    Style::default().bg(PANEL_ALT)
}

pub(crate) fn text() -> Style {
    Style::default().fg(TEXT)
}

pub(crate) fn muted_text() -> Style {
    Style::default().fg(MUTED)
}

pub(crate) fn accent_text() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub(crate) fn soft_accent_text() -> Style {
    Style::default().fg(ACCENT_SOFT)
}

pub(crate) fn success_text() -> Style {
    Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD)
}

pub(crate) fn warning_text() -> Style {
    Style::default().fg(WARNING).add_modifier(Modifier::BOLD)
}

pub(crate) fn danger_text() -> Style {
    Style::default().fg(DANGER).add_modifier(Modifier::BOLD)
}

pub(crate) fn border() -> Style {
    Style::default().fg(BORDER)
}

pub(crate) fn selected_border() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
