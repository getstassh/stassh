use backend::AppState;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Color,
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
};

use crate::{
    inputs::handle_text_input,
    navigation::{Screen, StringState},
    screens::AppEffect,
    ui::{centered_rect, full_rect, line_with_caret},
};

use crate::screens::ScreenHandler;

pub fn asking_passphrase_handler() -> ScreenHandler<StringState> {
    ScreenHandler {
        matches: |s| matches!(s, Screen::AskingPassphrase { .. }),
        get: |s| match s {
            Screen::AskingPassphrase { state } => Some(state),
            _ => None,
        },
        get_mut: |s| match s {
            Screen::AskingPassphrase { state } => Some(state),
            _ => None,
        },
        render: ui,
        handle_key: handle_key,
        handle_tick: |_app, _| None,
    }
}

fn handle_key(_: &AppState, key_code: KeyCode, state: &mut StringState) -> Option<AppEffect> {
    let text = handle_text_input(state, key_code);
    if let Some(text) = text {
        let text = text.to_string();
        return Some(Box::new(move |app| {
            app.password = Some(text);
            let result = app.load_db();
            if let Err(e) = result {
                panic!("Failed to load database with provided passphrase: {e}");
            }

            app.screen = Screen::Dashboard;
        }));
    }
    None
}

const ASCII_ART: &str = include_str!("../../ascii-art.txt");

fn ui(frame: &mut Frame, _app: &AppState, state: &StringState) {
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Enter Passphrase",
        "Type your passphrase and press Enter",
    );

    frame.render_widget(inner, a);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(ASCII_ART.lines().count() as u16 + 2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    render_logo_with_credits(frame, layout[0]);

    let question = Paragraph::new("Enter your passphrase:").alignment(Alignment::Center);
    frame.render_widget(question, layout[2]);
    let (text_box, text_box_area, text_area) = centered_rect(50, 3, layout[3]);
    frame.render_widget(text_box, text_box_area);
    let passphrase = Paragraph::new(line_with_caret(state)).alignment(Alignment::Left);
    frame.render_widget(passphrase, text_area);
}

fn render_logo_with_credits(frame: &mut Frame, area: Rect) {
    const WHITE_HEX: u32 = 0xFFFFFF;
    const ORANGE_HEX: u32 = 0xE77500;
    const SPLIT_COL: usize = 44;

    let white = hex_color(WHITE_HEX);
    let orange = hex_color(ORANGE_HEX);

    let mut lines = Vec::new();
    for raw_line in ASCII_ART.lines() {
        let split_idx = raw_line
            .char_indices()
            .nth(SPLIT_COL)
            .map(|(idx, _)| idx)
            .unwrap_or(raw_line.len());
        let (left, right) = raw_line.split_at(split_idx);

        lines.push(Line::from(vec![
            Span::styled(left.to_string(), Style::default().fg(white)),
            Span::styled(right.to_string(), Style::default().fg(orange)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Created by Lazar (bylazar.com)",
        Style::default().fg(white),
    )));

    let art = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    frame.render_widget(art, area);
}

fn hex_color(hex: u32) -> Color {
    let r = ((hex >> 16) & 0xFF) as u8;
    let g = ((hex >> 8) & 0xFF) as u8;
    let b = (hex & 0xFF) as u8;
    Color::Rgb(r, g, b)
}
