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

enum Screen {
    LoadingLogo,
    OnboardingWantsEncryption,
    OnboardingWantsPassphrase,
    AskingPassphrase,
    Dashboard,
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // TODO: load config and db, pass to app state
    let config = backend::Config::load_config();
    match config.db_encryption {
        None => {
            // ask user if they want to encrypt the db, if yes, ask for passphrase and create new db with encryption
            // if no, create new db without encryption
        }
        Some(backend::DbEncryption::None) => {
            // load db without encryption
        }
        Some(backend::DbEncryption::Passphrase) => {
            // ask for passphrase, load db with encryption
        }
    }

    let mut app = AppState::new(config, backend::Database::default());
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

        if app.should_quit() {
            return Ok(());
        }

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => app.request_quit(),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &AppState) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    let title = Paragraph::new(Line::from(app.app_name()).style(Style::default().fg(Color::Green)))
        .block(Block::bordered().title("App"));

    let content = Paragraph::new(vec![Line::from("Press any key to tick, press q to quit.")])
        .block(Block::bordered().title("Dashboard"))
        .style(Style::default().fg(Color::Cyan));

    let status = Paragraph::new(
        Line::from("crossterm + ratatui workspace scaffold")
            .style(Style::default().fg(Color::Yellow)),
    )
    .block(Block::bordered().title("Status"));

    frame.render_widget(title, header);
    frame.render_widget(content, body);
    frame.render_widget(status, footer);
}
