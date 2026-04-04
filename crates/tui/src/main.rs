use std::io;
use std::time::Duration;

use anyhow::Result;

use backend::AppState;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::{Color, Frame, Line, Style},
    style::Modifier,
    text::{Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::ui::{button, centered_rect, dual_vertical_rect, full_rect, line_with_caret};

mod ui;

const ASCII_ART: &str = include_str!("../ascii-art.txt");

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let config = backend::Config::load_config();
    let mut app = AppState::new(config);
    let app_result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
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

        if app.time_since_start() > Duration::from_secs(1)
            && app.screen == backend::Screen::LoadingLogo
        {
            match app.config.db_encryption {
                Some(backend::DbEncryption::Passphrase) => {
                    app.set_screen(backend::Screen::AskingPassphrase {
                        state: backend::StringState::invisible(),
                    });
                }
                Some(backend::DbEncryption::None) => {
                    app.db = backend::load_db(backend::DbEncryption::None, None)
                        .unwrap_or_else(|_| backend::Database::default());
                    app.set_screen(backend::Screen::Dashboard);
                }
                None => {
                    app.set_screen(backend::Screen::OnboardingWantsEncryption {
                        state: backend::YesNoState::new(),
                    });
                }
            }
        }

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

                    let mut selected_encryption: Option<backend::DbEncryption> = None;
                    let mut next_screen: Option<backend::Screen> = None;
                    let mut entered_passphrase: Option<String> = None;
                    let mut unlock_passphrase: Option<String> = None;

                    match &mut app.screen {
                        backend::Screen::OnboardingWantsEncryption { state } => {
                            if key_code == KeyCode::Left
                                || key_code == KeyCode::Right
                                || key_code == KeyCode::Tab
                            {
                                state.toggle();
                            }
                            if key_code == KeyCode::Char('y')
                                || (key_code == KeyCode::Enter && state.is_yes())
                            {
                                selected_encryption = Some(backend::DbEncryption::Passphrase);
                                next_screen = Some(backend::Screen::OnboardingWantsPassphrase {
                                    state: backend::StringState::invisible(),
                                });
                            }
                            if key_code == KeyCode::Char('n')
                                || (key_code == KeyCode::Enter && state.is_no())
                            {
                                selected_encryption = Some(backend::DbEncryption::None);
                                next_screen = Some(backend::Screen::Dashboard);
                            }
                        }
                        backend::Screen::OnboardingWantsPassphrase { state } => match key_code {
                            KeyCode::Char(c) => {
                                let mut text = state.text.clone();
                                text.insert(state.caret_position, c);
                                state.set_text(text);
                                state.caret_position += 1;
                            }
                            KeyCode::Backspace => {
                                let mut text = state.text.clone();
                                if state.caret_position > 0 {
                                    text.remove(state.caret_position - 1);
                                    state.set_text(text);
                                    state.caret_position -= 1;
                                }
                            }
                            KeyCode::Enter => {
                                selected_encryption = Some(backend::DbEncryption::Passphrase);
                                entered_passphrase = Some(state.text.clone());
                                next_screen = Some(backend::Screen::Dashboard);
                            }
                            KeyCode::Left => {
                                if state.caret_position > 0 {
                                    state.caret_position -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if state.caret_position < state.text.len() {
                                    state.caret_position += 1;
                                }
                            }
                            _ => {}
                        },
                        backend::Screen::AskingPassphrase { state } => match key_code {
                            KeyCode::Char(c) => {
                                let mut text = state.text.clone();
                                text.insert(state.caret_position, c);
                                state.set_text(text);
                                state.caret_position += 1;
                            }
                            KeyCode::Backspace => {
                                let mut text = state.text.clone();
                                if state.caret_position > 0 {
                                    text.remove(state.caret_position - 1);
                                    state.set_text(text);
                                    state.caret_position -= 1;
                                }
                            }
                            KeyCode::Enter => {
                                unlock_passphrase = Some(state.text.clone());
                            }
                            KeyCode::Left => {
                                if state.caret_position > 0 {
                                    state.caret_position -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if state.caret_position < state.text.len() {
                                    state.caret_position += 1;
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }

                    if let Some(encryption) = selected_encryption {
                        app.config.db_encryption = Some(encryption);
                        app.config.save_config().ok();
                    }

                    if let Some(passphrase) = entered_passphrase {
                        app.password = Some(passphrase);
                    }

                    if let Some(screen) = next_screen {
                        if screen == backend::Screen::Dashboard {
                            match app.config.db_encryption {
                                Some(backend::DbEncryption::None) => {
                                    app.db = backend::load_db(backend::DbEncryption::None, None)
                                        .unwrap_or_else(|_| backend::Database::default());
                                    let _ = backend::save_db(
                                        &app.db,
                                        backend::DbEncryption::None,
                                        None,
                                    );
                                }
                                Some(backend::DbEncryption::Passphrase) => {
                                    app.db = backend::load_db(
                                        backend::DbEncryption::Passphrase,
                                        app.password.as_deref(),
                                    )
                                    .unwrap_or_else(|_| backend::Database::default());
                                    let _ = backend::save_db(
                                        &app.db,
                                        backend::DbEncryption::Passphrase,
                                        app.password.as_deref(),
                                    );
                                }
                                None => {}
                            }
                        }
                        app.set_screen(screen);
                    }

                    if let Some(passphrase) = unlock_passphrase
                        && let Ok(db) =
                            backend::load_db(backend::DbEncryption::Passphrase, Some(&passphrase))
                    {
                        app.password = Some(passphrase);
                        app.db = db;
                        app.set_screen(backend::Screen::Dashboard);
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &AppState) {
    match &app.screen {
        backend::Screen::LoadingLogo => ui_loading_logo(frame, app),
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

fn ui_loading_logo(frame: &mut Frame, _app: &AppState) {
    const BG_HEX: u32 = 0x001521;
    const WHITE_HEX: u32 = 0xFFFFFF;
    const ORANGE_HEX: u32 = 0xE77500;
    const SPLIT_COL: usize = 50;

    let bg = hex_color(BG_HEX);
    let white = hex_color(WHITE_HEX);
    let orange = hex_color(ORANGE_HEX);

    let size = frame.area();
    let block = Block::default().style(Style::default().bg(bg));

    let mut lines = Vec::new();
    for raw_line in ASCII_ART.lines() {
        let split_idx = raw_line
            .char_indices()
            .nth(SPLIT_COL)
            .map(|(idx, _)| idx)
            .unwrap_or(raw_line.len());
        let (left, right) = raw_line.split_at(split_idx);

        lines.push(Line::from(vec![
            Span::styled(left.to_string(), Style::default().fg(white).bg(bg)),
            Span::styled(right.to_string(), Style::default().fg(orange).bg(bg)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Created by Lazar (bylazar.com)",
        Style::default().fg(white).bg(bg),
    )));

    let art = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    frame.render_widget(block, size);
    frame.render_widget(art, size);
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
