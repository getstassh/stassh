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
    widgets::{Block, Borders, Paragraph},
};

use crate::ui::{button, centered_rect, dual_vertical_rect, full_rect};

mod ui;

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
            app.set_screen(app.target_screen.clone());
        }

        if app.should_quit() {
            return Ok(());
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
                                app.config.save_config().ok();
                                next_screen = Some(backend::Screen::OnboardingWantsPassphrase {
                                    state: backend::StringState::invisible(),
                                });
                            }
                            if key_code == KeyCode::Char('n')
                                || (key_code == KeyCode::Enter && state.is_no())
                            {
                                selected_encryption = Some(backend::DbEncryption::None);
                                app.config.save_config().ok();
                                next_screen = Some(backend::Screen::Dashboard);
                            }
                        }
                        backend::Screen::OnboardingWantsPassphrase { state }
                        | backend::Screen::AskingPassphrase { state } => match key_code {
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
                                app.config.save_config().ok();
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
                        _ => {}
                    }

                    if let Some(encryption) = selected_encryption {
                        app.config.db_encryption = Some(encryption);
                    }

                    if let Some(screen) = next_screen {
                        app.set_screen(screen);
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &mut AppState) {
    match &app.screen {
        backend::Screen::LoadingLogo => ui_loading_logo(frame, app),
        backend::Screen::OnboardingWantsEncryption { state } => {
            ui_onboarding_wants_encryption(frame, app, state)
        }
        backend::Screen::OnboardingWantsPassphrase { state } => {
            let mut s = state.clone();
            ui_onboarding_wants_passphrase(frame, app, &mut s);
            app.set_screen(backend::Screen::OnboardingWantsPassphrase { state: s });
        }
        backend::Screen::AskingPassphrase { .. } => ui_asking_passphrase(frame, app),
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
    state: &mut backend::StringState,
) {
    let a = frame.area();

    let (inner, area) = full_rect(
        a,
        "Welcome to stassh!",
        "Use ←/→ or Tab to switch, Enter to confirm, type your passphrase",
    );

    frame.render_widget(inner, a);

    let question = Paragraph::new("Enter your passphrase:").alignment(Alignment::Center);
    let passphrase = Paragraph::new(state.visible_text()).alignment(Alignment::Center);

    let (top, bottom) = dual_vertical_rect(area);
    frame.render_widget(question, top);
    frame.render_widget(passphrase, bottom);
}

fn ui_asking_passphrase(frame: &mut Frame, app: &AppState) {
    let size = frame.area();
    let block = Block::default()
        .style(Style::default().bg(Color::Yellow))
        .title("Enter your passphrase to unlock your database:");
    frame.render_widget(block, size);
}

fn ui_loading_logo(frame: &mut Frame, app: &AppState) {
    let size = frame.area();
    let block = Block::default()
        .style(Style::default().bg(Color::Blue))
        .title("Loading stassh...");
    frame.render_widget(block, size);
}

fn ui_dashboard(frame: &mut Frame, app: &AppState) {
    let size = frame.area();
    let block = Block::default()
        .style(Style::default().bg(Color::Green))
        .title("Dashboard");
    frame.render_widget(block, size);
}
