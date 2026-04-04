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
                            if key_code == KeyCode::Char('y') {
                                selected_encryption = Some(backend::DbEncryption::Passphrase);
                                // save config
                                next_screen = Some(backend::Screen::OnboardingWantsPassphrase {
                                    passphrase: backend::StringState::new(),
                                });
                            }
                            if key_code == KeyCode::Char('n') {
                                selected_encryption = Some(backend::DbEncryption::None);
                                // save config
                                next_screen = Some(backend::Screen::Dashboard);
                            }
                            if key_code == KeyCode::Enter {
                                if state.is_yes() {
                                    selected_encryption = Some(backend::DbEncryption::Passphrase);
                                    // save config
                                    next_screen =
                                        Some(backend::Screen::OnboardingWantsPassphrase {
                                            passphrase: backend::StringState::new(),
                                        });
                                } else {
                                    selected_encryption = Some(backend::DbEncryption::None);
                                    // save config
                                    next_screen = Some(backend::Screen::Dashboard);
                                }
                            }
                        }
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

fn ui(frame: &mut Frame, app: &AppState) {
    match &app.screen {
        backend::Screen::LoadingLogo => ui_loading_logo(frame, app),
        backend::Screen::OnboardingWantsEncryption { state } => {
            ui_onboarding_wants_encryption(frame, app, state)
        }
        backend::Screen::OnboardingWantsPassphrase { .. } => {
            ui_onboarding_wants_passphrase(frame, app)
        }
        backend::Screen::AskingPassphrase { .. } => ui_asking_passphrase(frame, app),
        backend::Screen::Dashboard => ui_dashboard(frame, app),
    }
}
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width),
            Constraint::Fill(1),
        ])
        .split(vertical[1]);

    horizontal[1]
}

fn ui_onboarding_wants_encryption(frame: &mut Frame, _app: &AppState, state: &backend::YesNoState) {
    let area = frame.area();

    let title = Line::from(" Welcome to stassh! ").centered();
    let controls =
        Line::from(" Use ←/→ or Tab to switch, Enter to confirm, Y/N for quick answer ").centered();

    let dialog_area = centered_rect(area.width, area.height, area);

    let block = Block::default()
        .title(title)
        .title_bottom(controls)
        .borders(Borders::ALL);

    let inner = block.inner(dialog_area);

    frame.render_widget(block, dialog_area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // question
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
        ])
        .split(inner);

    let question = Paragraph::new("Do you want to enable encryption?").alignment(Alignment::Center);

    let yes_style = if state.is_yes() {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let no_style = if state.is_no() {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let buttons = Paragraph::new(format!(
        "   {}      {}   ",
        styled_button("Yes", yes_style),
        styled_button("No", no_style),
    ))
    .alignment(Alignment::Center);

    frame.render_widget(question, layout[0]);
    frame.render_widget(buttons, layout[2]);
}

fn styled_button(label: &str, style: Style) -> String {
    if style == Style::default() {
        format!("[ {} ]", label)
    } else {
        format!("[*{}*]", label)
    }
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
