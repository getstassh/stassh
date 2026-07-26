use std::io;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::navigation::Screen;

use anyhow::Result;

use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
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

        let should_enable_mouse_capture = app.is_ssh_screen() || app.is_dashboard_screen();
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
            let mut pending_drag = None;

            loop {
                let event = event::read()?;

                if let Event::Mouse(mouse) = event
                    && matches!(
                        mouse.kind,
                        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left)
                    )
                {
                    pending_drag = Some(mouse);
                } else {
                    if let Some(mouse) = pending_drag.take() {
                        handler.handle_mouse(app, mouse);
                    }

                    match event {
                        Event::Key(key) => {
                            let is_press_or_repeat =
                                key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat;
                            let is_quick_switch_release =
                                key.kind == KeyEventKind::Release && app.is_quick_switcher_open();

                            if is_press_or_repeat || is_quick_switch_release {
                                let is_exit_context =
                                    key.code == KeyCode::Esc
                                    && !app.is_ssh_screen()
                                    && !app.has_modal_open();

                                if is_exit_context {
                                    let now = Instant::now();
                                    let should_exit = app.exit_pending().is_some_and(|t| now.duration_since(t).as_secs() < 1);
                                    if should_exit {
                                        return Ok(());
                                    }
                                    app.set_exit_pending(Some(now));
                                    if let Screen::Dashboard { state } = &mut app.screen {
                                        state.exit_pending_at = Some(now);
                                        state.last_status = Some("Press Esc again to exit".to_string());
                                    }
                                } else {
                                    if app.exit_pending().is_some() {
                                        app.set_exit_pending(None);
                                        if let Screen::Dashboard { state } = &mut app.screen {
                                            state.exit_pending_at = None;
                                            state.last_status = None;
                                        }
                                    }
                                    handler.handle_key(app, key);
                                    if app.exit_requested() {
                                        return Ok(());
                                    }
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
                            handler.handle_focus(app, true);
                            if let Ok((cols, rows)) = crossterm::terminal::size() {
                                if cols > 0 && rows > 0 {
                                    handler.handle_resize(app, cols, rows);
                                }
                            }
                        }
                        Event::FocusLost => {
                            handler.handle_focus(app, false);
                        }
                    }
                }

                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }

            if let Some(mouse) = pending_drag {
                handler.handle_mouse(app, mouse);
            }
        }
    }
}
