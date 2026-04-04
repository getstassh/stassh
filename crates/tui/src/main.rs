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
    layout::{Constraint, Layout},
    prelude::{Color, Frame, Line, Style},
    widgets::{Block, Paragraph},
};

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
                    if (key_code == KeyCode::Char('q')) {
                        app.request_quit();
                    }

                    match app.screen {
                        backend::Screen::OnboardingWantsEncryption => {
                            if key_code == KeyCode::Char('y') {
                                app.config.db_encryption = Some(backend::DbEncryption::Passphrase);
                                // save config
                                app.set_screen(backend::Screen::OnboardingWantsPassphrase);
                            }
                            if key_code == KeyCode::Char('n') {
                                app.config.db_encryption = Some(backend::DbEncryption::None);
                                // save config
                                app.set_screen(backend::Screen::Dashboard);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &AppState) {
    match app.screen {
        backend::Screen::LoadingLogo => ui_loading_logo(frame, app),
        backend::Screen::OnboardingWantsEncryption => ui_onboarding_wants_encryption(frame, app),
        backend::Screen::OnboardingWantsPassphrase => ui_onboarding_wants_passphrase(frame, app),
        backend::Screen::AskingPassphrase => ui_asking_passphrase(frame, app),
        backend::Screen::Dashboard => ui_dashboard(frame, app),
    }
}

fn ui_onboarding_wants_encryption(frame: &mut Frame, app: &AppState) {
    let size = frame.area();
    let block = Block::default()
        .style(Style::default().bg(Color::Yellow))
        .title("Do you want to encrypt your database? (y/n)");
    frame.render_widget(block, size);
}

fn ui_onboarding_wants_passphrase(frame: &mut Frame, app: &AppState) {
    let size = frame.area();
    let block = Block::default()
        .style(Style::default().bg(Color::Yellow))
        .title("Enter a passphrase to encrypt your database:");
    frame.render_widget(block, size);
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
