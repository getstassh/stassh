use std::io;
use std::time::Duration;

use anyhow::Result;

use backend::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::screens::get_handler_for_screen;

mod inputs;
mod screens;
mod ui;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let config = backend::Config::load_config();
    let mut app = App::new(config);
    let app_result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    app_result?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(50);
    let key_rate = Duration::from_millis(250);

    let handlers = screens::get_handlers();

    let mut last_tick_time = std::time::Instant::now();

    loop {
        let handler = get_handler_for_screen(&handlers, &app.screen);

        terminal.draw(|frame| handler.render(frame, app))?;

        let time_since_last_tick = last_tick_time.elapsed();
        if time_since_last_tick >= tick_rate {
            handler.handle_tick(app);
            last_tick_time = std::time::Instant::now();
        }

        if event::poll(key_rate)? {
            if let Event::Key(key) = event::read()? {
                let key_code = key.code;
                if key.kind == KeyEventKind::Press {
                    if key_code == KeyCode::Char('c')
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        return Ok(());
                    }

                    handler.handle_key(app, key_code);
                }
            }
        }
    }

    // loop {
    //     terminal.draw(|frame| ui(frame, app))?;

    //     if app.screen == backend::Screen::Dashboard {
    //         app.db.index = app.db.index.wrapping_add(1);
    //         if app.config.db_encryption.is_some() {
    //             let _ = backend::save_db(
    //                 &app.db,
    //                 app.config.db_encryption.clone().unwrap(),
    //                 app.password.as_deref(),
    //             );
    //         }
    //     }

    //     if event::poll(tick_rate)? {
    //         if let Event::Key(key) = event::read()? {
    //             let key_code = key.code;
    //             if key.kind == KeyEventKind::Press {

    //                 let action = match &mut app.screen {
    //                     backend::Screen::OnboardingWantsEncryption { state } => {
    //                         match handle_yes_no_input(state, key_code) {
    //                             Some(true) => Action::GoToOnboardingPassphrase,
    //                             Some(false) => Action::EnableNoEncryption,
    //                             None => Action::None,
    //                         }
    //                     }
    //                     backend::Screen::OnboardingWantsPassphrase { state } => {
    //                         match handle_text_input(state, key_code) {
    //                             Some(passphrase) => {
    //                                 Action::FinishOnboardingWithPassphrase(passphrase)
    //                             }
    //                             None => Action::None,
    //                         }
    //                     }
    //                     backend::Screen::AskingPassphrase { state } => {
    //                         match handle_text_input(state, key_code) {
    //                             Some(passphrase) => Action::UnlockWithPassphrase(passphrase),
    //                             None => Action::None,
    //                         }
    //                     }
    //                     backend::Screen::Dashboard => Action::None,
    //                 };

    //                 match action {
    //                     Action::None => {}
    //                     Action::GoToOnboardingPassphrase => {
    //                         app.set_screen(backend::Screen::OnboardingWantsPassphrase {
    //                             state: backend::StringState::invisible(),
    //                         });
    //                     }
    //                     Action::EnableNoEncryption => {
    //                         app.config.db_encryption = Some(backend::DbEncryption::None);
    //                         app.config.save_config().ok();
    //                         app.db = backend::load_db(backend::DbEncryption::None, None)
    //                             .unwrap_or_else(|_| backend::Database::default());
    //                         let _ = backend::save_db(&app.db, backend::DbEncryption::None, None);
    //                         app.set_screen(backend::Screen::Dashboard);
    //                     }
    //                     Action::FinishOnboardingWithPassphrase(passphrase) => {
    //                         app.config.db_encryption = Some(backend::DbEncryption::Passphrase);
    //                         app.config.save_config().ok();
    //                         app.password = Some(passphrase.clone());
    //                         let _ = backend::save_db(
    //                             &app.db,
    //                             backend::DbEncryption::Passphrase,
    //                             Some(passphrase.as_str()),
    //                         );
    //                         app.set_screen(backend::Screen::Dashboard);
    //                     }
    //                     Action::UnlockWithPassphrase(passphrase) => {
    //                         app.password = Some(passphrase.clone());
    //                         app.db = backend::load_db(
    //                             backend::DbEncryption::Passphrase,
    //                             Some(passphrase.as_str()),
    //                         )
    //                         .unwrap_or_else(|_| backend::Database::default());
    //                         app.set_screen(backend::Screen::Dashboard);
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }
}
