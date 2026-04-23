use backend::AppState;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::{
    app::App,
    navigation::Screen,
    screens::components::{LogoType, render_logo},
    ui::{accent_text, full_rect, text},
};

mod asking_passphrase;
mod components;
mod dashboard;
mod onboarding_wants_encryption;
mod onboarding_wants_passphrase;
mod onboarding_wants_telemetry;
mod startup_update;

type AppEffect = Box<dyn FnOnce(&mut App)>;

struct ScreenHandler<S> {
    matches: fn(&Screen) -> bool,
    get: fn(&Screen) -> Option<&S>,
    get_mut: fn(&mut Screen) -> Option<&mut S>,
    render: fn(&mut Frame, &AppState, &S),
    handle_key: fn(&AppState, KeyEvent, &mut S) -> Option<AppEffect>,
    handle_mouse: fn(&AppState, MouseEvent, &mut S) -> Option<AppEffect>,
    handle_paste: fn(&AppState, &str, &mut S) -> Option<AppEffect>,
    handle_resize: fn(&AppState, u16, u16, &mut S) -> Option<AppEffect>,
    handle_tick: fn(&AppState, &mut S) -> Option<AppEffect>,
}

pub(crate) trait AnyScreenHandler: Sync {
    fn matches(&self, screen: &Screen) -> bool;
    fn render(&self, frame: &mut Frame, app: &App);
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn handle_mouse(&self, app: &mut App, mouse: MouseEvent);
    fn handle_paste(&self, app: &mut App, text: &str);
    fn handle_resize(&self, app: &mut App, cols: u16, rows: u16);
    fn handle_tick(&self, app: &mut App);
}

impl<S: 'static> AnyScreenHandler for ScreenHandler<S> {
    fn matches(&self, screen: &Screen) -> bool {
        (self.matches)(screen)
    }

    fn render(&self, frame: &mut Frame, app: &App) {
        if let Some(state) = (self.get)(&app.screen) {
            (self.render)(frame, app.state(), state);
        }
    }

    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        let effect = {
            let (app_state, screen) = app.state_and_screen_mut();

            if let Some(state) = (self.get_mut)(screen) {
                (self.handle_key)(app_state, key, state)
            } else {
                None
            }
        };

        if let Some(effect) = effect {
            effect(app);
        }
    }

    fn handle_mouse(&self, app: &mut App, mouse: MouseEvent) {
        let effect = {
            let (app_state, screen) = app.state_and_screen_mut();

            if let Some(state) = (self.get_mut)(screen) {
                (self.handle_mouse)(app_state, mouse, state)
            } else {
                None
            }
        };

        if let Some(effect) = effect {
            effect(app);
        }
    }

    fn handle_tick(&self, app: &mut App) {
        let effect = {
            let (app_state, screen) = app.state_and_screen_mut();

            if let Some(state) = (self.get_mut)(screen) {
                (self.handle_tick)(app_state, state)
            } else {
                None
            }
        };

        if let Some(effect) = effect {
            effect(app);
        }
    }

    fn handle_paste(&self, app: &mut App, text: &str) {
        let effect = {
            let (app_state, screen) = app.state_and_screen_mut();

            if let Some(state) = (self.get_mut)(screen) {
                (self.handle_paste)(app_state, text, state)
            } else {
                None
            }
        };

        if let Some(effect) = effect {
            effect(app);
        }
    }

    fn handle_resize(&self, app: &mut App, cols: u16, rows: u16) {
        let effect = {
            let (app_state, screen) = app.state_and_screen_mut();

            if let Some(state) = (self.get_mut)(screen) {
                (self.handle_resize)(app_state, cols, rows, state)
            } else {
                None
            }
        };

        if let Some(effect) = effect {
            effect(app);
        }
    }
}

static HANDLERS: &[&dyn AnyScreenHandler] = &[
    &dashboard::HANDLER,
    &startup_update::HANDLER,
    &onboarding_wants_encryption::HANDLER,
    &onboarding_wants_passphrase::HANDLER,
    &onboarding_wants_telemetry::HANDLER,
    &asking_passphrase::HANDLER,
];

static EMPTY_HANDLER: ScreenHandler<()> = ScreenHandler {
    matches: |_| true,
    get: |_| Some(&()),
    get_mut: |_| None,
    render: |frame, _, _| {
        let a = frame.area();

        let (inner, area) = full_rect(a, "404 not found", "Esc exit");

        frame.render_widget(inner, a);

        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(7)])
            .split(area);

        render_logo(frame, split[0], LogoType::Simple);
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(split[1]);
        frame.render_widget(
            Paragraph::new("This view is still under construction")
                .alignment(Alignment::Center)
                .style(text()),
            split[0],
        );
        frame.render_widget(
            Paragraph::new("Return with Esc")
                .alignment(Alignment::Center)
                .style(accent_text()),
            split[1],
        );
    },
    handle_key: |_, _, _| None,
    handle_mouse: |_, _, _| None,
    handle_paste: |_, _, _| None,
    handle_resize: |_, _, _, _| None,
    handle_tick: |_, _| None,
};

pub(crate) fn get_handler_for_screen<'a>(screen: &Screen) -> &'a dyn AnyScreenHandler {
    HANDLERS
        .iter()
        .copied()
        .find(|h| h.matches(screen))
        .unwrap_or(&EMPTY_HANDLER)
}
