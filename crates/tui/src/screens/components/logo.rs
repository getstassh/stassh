use ratatui::{
    Frame,
    layout::Alignment,
    prelude::Color,
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
};

use crate::ui::muted_text;

const LOGO_XS: &str = include_str!("../../../ascii-art-xs.txt");
const LOGO_LG: &str = include_str!("../../../ascii-art-lg.txt");
const LOGO_MD: &str = include_str!("../../../ascii-art-md.txt");
const LOGO_SM: &str = include_str!("../../../ascii-art-sm.txt");

const CREDIT: &str = "Created by Lazar (bylazar.com)";

pub(crate) enum LogoType {
    Simple,
    WithCredits,
}

struct ParsedLogo<'a> {
    split_col: usize,
    lines: Vec<&'a str>,
    width: usize,
}

impl<'a> ParsedLogo<'a> {
    fn height(&self) -> usize {
        self.lines.len()
    }
}

pub(crate) fn render_logo(frame: &mut Frame, area: ratatui::layout::Rect, logo_type: LogoType) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let lg = parse_logo(LOGO_LG);
    let md = parse_logo(LOGO_MD);
    let sm = parse_logo(LOGO_SM);
    let xs = parse_logo(LOGO_XS);

    let logos = [&lg, &md, &sm, &xs];
    let area_width = area.width as usize;
    let area_height = area.height as usize;

    let padding_percentage = 0.1;
    let padded_width = (area_width as f32 * (1.0 - padding_percentage * 2.0)) as usize;
    let padded_height = (area_height as f32 * (1.0 - padding_percentage * 2.0)) as usize;

    if let Some(logo) = logos
        .iter()
        .find(|logo| logo.width <= padded_width && logo.height() <= padded_height)
    {
        render_logo_size(frame, area, logo, padded_width, padded_height, logo_type);
        return;
    }

    render_logo_size(frame, area, &xs, area_width, area_height, logo_type);
}

fn parse_logo(raw: &str) -> ParsedLogo<'_> {
    let mut lines = raw.lines();
    let marker_line = lines.next().unwrap_or_default();
    let split_col = marker_line.chars().position(|c| c == '0').unwrap_or(0);

    let body_lines: Vec<&str> = lines.collect();
    let width = body_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    ParsedLogo {
        split_col,
        lines: body_lines,
        width,
    }
}

fn render_logo_size(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    logo: &ParsedLogo,
    area_width: usize,
    area_height: usize,
    logo_type: LogoType,
) {
    let light = hex_color(0xE2D9CA);
    let amber = hex_color(0xE3903E);

    let mut lines = Vec::new();
    for raw_line in &logo.lines {
        let (left, right) = split_with_width(raw_line, logo.split_col, area_width);
        lines.push(Line::from(vec![
            Span::styled(left, Style::default().fg(light)),
            Span::styled(right, Style::default().fg(amber)),
        ]));
    }

    if area_height >= logo.height() + 2 && matches!(logo_type, LogoType::WithCredits) {
        lines.push(Line::from(Span::styled(CREDIT, muted_text())));
    }

    let art = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    frame.render_widget(art, area);
}

fn split_with_width(line: &str, split_col: usize, max_chars: usize) -> (String, String) {
    let left_full = take_chars(line, split_col);
    let right_full = skip_chars(line, split_col);

    if max_chars <= left_full.chars().count() {
        return (take_chars(&left_full, max_chars), String::new());
    }

    let right_width = max_chars.saturating_sub(left_full.chars().count());
    (left_full, take_chars(&right_full, right_width))
}

fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn skip_chars(s: &str, n: usize) -> String {
    s.chars().skip(n).collect()
}

fn hex_color(hex: u32) -> Color {
    let r = ((hex >> 16) & 0xFF) as u8;
    let g = ((hex >> 8) & 0xFF) as u8;
    let b = (hex & 0xFF) as u8;
    Color::Rgb(r, g, b)
}
