use std::io;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture,
        EnableBracketedPaste, EnableFocusChange, EnableMouseCapture, Event, KeyCode,
        KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
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
    let restart_exe = std::env::current_exe()?;
    let restart_args: Vec<_> = std::env::args_os().skip(1).collect();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableFocusChange,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let app_result = run_app(&mut terminal, &mut app);
    let should_restart = app.restart_requested();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        DisableFocusChange,
        PopKeyboardEnhancementFlags,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    app_result?;

    if should_restart {
        if let Err(err) = Command::new(&restart_exe).args(&restart_args).status() {
            eprintln!(
                "Update installed, but automatic restart failed: {}. Please restart stassh manually.",
                err
            );
        }
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(50);
    let key_rate = Duration::from_millis(16);
    let mut mouse_capture_enabled = false;

    let mut last_tick_time = std::time::Instant::now();

    loop {
        let handler = get_handler_for_screen(&app.screen);

        terminal.draw(|frame| handler.render(frame, app))?;

        let should_enable_mouse_capture = app.is_ssh_screen();
        if should_enable_mouse_capture != mouse_capture_enabled {
            if should_enable_mouse_capture {
                execute!(terminal.backend_mut(), EnableMouseCapture)?;
            } else {
                execute!(terminal.backend_mut(), DisableMouseCapture)?;
            }
            mouse_capture_enabled = should_enable_mouse_capture;
        }

        let time_since_last_tick = last_tick_time.elapsed();
        if time_since_last_tick >= tick_rate {
            handler.handle_tick(app);
            app.poll_version_check();
            app.maybe_report_telemetry();
            last_tick_time = std::time::Instant::now();

            if app.exit_requested() {
                return Ok(());
            }
        }

        if event::poll(key_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    let is_press_or_repeat =
                        key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat;
                    let is_quick_switch_release =
                        key.kind == KeyEventKind::Release && app.is_quick_switcher_open();

                    if is_press_or_repeat || is_quick_switch_release {
                        if key.code == KeyCode::Esc && !app.is_ssh_screen() && !app.has_modal_open()
                        {
                            return Ok(());
                        }

                        handler.handle_key(app, key);

                        if app.exit_requested() {
                            return Ok(());
                        }
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
                Event::Mouse(mouse) => {
                    handler.handle_mouse(app, mouse);
                }
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
