use std::io;
use std::time::Duration;

use anyhow::Result;

use backend::AppState;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::{Color, Frame, Line, Style},
    text::{Span, Text},
    widgets::Paragraph,
};

use crate::inputs::{handle_text_input, handle_yes_no_input};
use crate::ui::{button, centered_rect, dual_vertical_rect, full_rect, line_with_caret};

mod inputs;
mod ui;

const ASCII_ART: &str = include_str!("../ascii-art.txt");

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let config = backend::Config::load_config();
    let mut app = AppState::new(config);
    let app_result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    app_result?;
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|frame| ui(frame, app))?;

        if app.should_quit() {
            return Ok(());
        }

        if app.screen == backend::Screen::Dashboard {
            app.db.index = app.db.index.wrapping_add(1);
            if app.config.db_encryption.is_some() {
                let _ = backend::save_db(
                    &app.db,
                    app.config.db_encryption.clone().unwrap(),
                    app.password.as_deref(),
                );
            }
        }

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                let key_code = key.code;
                if key.kind == KeyEventKind::Press {
                    if key_code == KeyCode::Char('q') {
                        app.request_quit();
                    }

                    enum Action {
                        None,
                        GoToOnboardingPassphrase,
                        EnableNoEncryption,
                        FinishOnboardingWithPassphrase(String),
                        UnlockWithPassphrase(String),
                    }

                    let action = match &mut app.screen {
                        backend::Screen::OnboardingWantsEncryption { state } => {
                            match handle_yes_no_input(state, key_code) {
                                Some(true) => Action::GoToOnboardingPassphrase,
                                Some(false) => Action::EnableNoEncryption,
                                None => Action::None,
                            }
                        }
                        backend::Screen::OnboardingWantsPassphrase { state } => {
                            match handle_text_input(state, key_code) {
                                Some(passphrase) => {
                                    Action::FinishOnboardingWithPassphrase(passphrase)
                                }
                                None => Action::None,
                            }
                        }
                        backend::Screen::AskingPassphrase { state } => {
                            match handle_text_input(state, key_code) {
                                Some(passphrase) => Action::UnlockWithPassphrase(passphrase),
                                None => Action::None,
                            }
                        }
                        backend::Screen::Dashboard => Action::None,
                    };

                    match action {
                        Action::None => {}
                        Action::GoToOnboardingPassphrase => {
                            app.set_screen(backend::Screen::OnboardingWantsPassphrase {
                                state: backend::StringState::invisible(),
                            });
                        }
                        Action::EnableNoEncryption => {
                            app.config.db_encryption = Some(backend::DbEncryption::None);
                            app.config.save_config().ok();
                            app.db = backend::load_db(backend::DbEncryption::None, None)
                                .unwrap_or_else(|_| backend::Database::default());
                            let _ = backend::save_db(&app.db, backend::DbEncryption::None, None);
                            app.set_screen(backend::Screen::Dashboard);
                        }
                        Action::FinishOnboardingWithPassphrase(passphrase) => {
                            app.config.db_encryption = Some(backend::DbEncryption::Passphrase);
                            app.config.save_config().ok();
                            app.password = Some(passphrase.clone());
                            let _ = backend::save_db(
                                &app.db,
                                backend::DbEncryption::Passphrase,
                                Some(passphrase.as_str()),
                            );
                            app.set_screen(backend::Screen::Dashboard);
                        }
                        Action::UnlockWithPassphrase(passphrase) => {
                            app.password = Some(passphrase.clone());
                            app.db = backend::load_db(
                                backend::DbEncryption::Passphrase,
                                Some(passphrase.as_str()),
                            )
                            .unwrap_or_else(|_| backend::Database::default());
                            app.set_screen(backend::Screen::Dashboard);
                        }
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &AppState) {
    match &app.screen {
        backend::Screen::OnboardingWantsEncryption { state } => {
            ui_onboarding_wants_encryption(frame, app, state)
        }
        backend::Screen::OnboardingWantsPassphrase { state } => {
            ui_onboarding_wants_passphrase(frame, app, &state);
        }
        backend::Screen::AskingPassphrase { state } => {
            ui_asking_passphrase(frame, app, &state);
        }
        backend::Screen::Dashboard => ui_dashboard(frame, app),
    }
}

fn ui_onboarding_wants_encryption(frame: &mut Frame, _app: &AppState, state: &backend::YesNoState) {
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Welcome to stassh!",
        "Use ←/→ or Tab to switch, Enter to confirm, Y/N for quick answers",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Do you want to enable encryption?").alignment(Alignment::Center);

    let buttons = Paragraph::new(format!(
        "{} {}",
        button("Yes", state.is_yes()),
        button("No", state.is_no()),
    ))
    .alignment(Alignment::Center);

    let (top, bottom) = dual_vertical_rect(area);
    frame.render_widget(question, top);
    frame.render_widget(buttons, bottom);
}

fn ui_onboarding_wants_passphrase(
    frame: &mut Frame,
    _app: &AppState,
    state: &backend::StringState,
) {
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Welcome to stassh!",
        "Use ←/→ or Tab to switch, Enter to confirm, type your passphrase",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Enter your passphrase:").alignment(Alignment::Center);
    let (top, bottom) = dual_vertical_rect(area);
    frame.render_widget(question, top);
    let (text_box, text_box_area, text_area) = centered_rect(50, 3, bottom);
    frame.render_widget(text_box, text_box_area);
    let passphrase = Paragraph::new(line_with_caret(state)).alignment(Alignment::Left);
    frame.render_widget(passphrase, text_area);
}

fn ui_asking_passphrase(frame: &mut Frame, _app: &AppState, state: &backend::StringState) {
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
    const SPLIT_COL: usize = 50;

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

fn ui_dashboard(frame: &mut Frame, app: &AppState) {
    let a = frame.area();

    let (inner, area) = full_rect(a, "Stassh", "Use ←/→ or Tab to switch");

    frame.render_widget(inner, a);

    let welcome = Paragraph::new(format!(
        "Welcome to stassh! You've been using the app for {} seconds.",
        app.time_since_start().as_secs()
    ))
    .alignment(Alignment::Center);

    let config = Paragraph::new(format!("Config: {:?}", app.config)).alignment(Alignment::Left);
    let database = Paragraph::new(format!("Database: {:?}", app.db)).alignment(Alignment::Left);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(welcome, layout[0]);
    frame.render_widget(config, layout[1]);
    frame.render_widget(database, layout[2]);
}
