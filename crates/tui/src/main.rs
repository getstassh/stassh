use std::io;
use std::time::Duration;

use anyhow::Result;

use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::App;
use crate::screens::get_handler_for_screen;

mod app;
mod inputs;
mod navigation;
mod screens;
mod ssh_client;
mod telemetry;
mod ui;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture,
        EnableFocusChange
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let app_result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        DisableFocusChange,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    app_result?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(50);
    let key_rate = Duration::from_millis(16);

    let mut last_tick_time = std::time::Instant::now();

    loop {
        let handler = get_handler_for_screen(&app.screen);

        terminal.draw(|frame| handler.render(frame, app))?;

        let time_since_last_tick = last_tick_time.elapsed();
        if time_since_last_tick >= tick_rate {
            handler.handle_tick(app);
            app.poll_version_check();
            app.maybe_report_telemetry();
            last_tick_time = std::time::Instant::now();
        }

        if event::poll(key_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                        if key.code == KeyCode::Esc && !app.is_ssh_screen() && !app.has_modal_open()
                        {
                            return Ok(());
                        }

                        handler.handle_key(app, key);
                    }
                }
                Event::Paste(text) => {
                    handler.handle_paste(app, &text);
                }
                Event::Resize(cols, rows) => {
                    if cols > 0 && rows > 0 {
                        handler.handle_resize(app, cols, rows);
                    }
                }
                Event::Mouse(_) => {}
                Event::FocusGained => {
                    if let Ok((cols, rows)) = crossterm::terminal::size() {
                        if cols > 0 && rows > 0 {
                            handler.handle_resize(app, cols, rows);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
