use backend::AppState;
use crossterm::event::KeyCode;
use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::{
    app::App,
    navigation::Screen,
    screens::components::{LogoType, render_logo},
    ui::{dual_vertical_rect, full_rect},
};

mod asking_passphrase;
mod components;
mod dashboard;
mod onboarding_wants_encryption;
mod onboarding_wants_passphrase;
mod onboarding_wants_telemetry;

type AppEffect = Box<dyn FnOnce(&mut App)>;

struct ScreenHandler<S> {
    matches: fn(&Screen) -> bool,
    get: fn(&Screen) -> Option<&S>,
    get_mut: fn(&mut Screen) -> Option<&mut S>,
    render: fn(&mut Frame, &AppState, &S),
    handle_key: fn(&AppState, KeyCode, &mut S) -> Option<AppEffect>,
    handle_tick: fn(&AppState, &mut S) -> Option<AppEffect>,
}

pub(crate) trait AnyScreenHandler: Sync {
    fn matches(&self, screen: &Screen) -> bool;
    fn render(&self, frame: &mut Frame, app: &App);
    fn handle_key(&self, app: &mut App, key: KeyCode);
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

    fn handle_key(&self, app: &mut App, key: KeyCode) {
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
}

static HANDLERS: &[&dyn AnyScreenHandler] = &[
    &dashboard::HANDLER,
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

        let (inner, area) = full_rect(a, "404 not found", "CTRL+C to exit");

        frame.render_widget(inner, a);

        let (top, bottom) = dual_vertical_rect(area);

        render_logo(frame, top, LogoType::Simple);
        let message =
            Paragraph::new("This screen is not implemented yet :(").alignment(Alignment::Center);
        frame.render_widget(message, bottom);
    },
    handle_key: |_, _, _| None,
    handle_tick: |_, _| None,
};

pub(crate) fn get_handler_for_screen<'a>(screen: &Screen) -> &'a dyn AnyScreenHandler {
    HANDLERS
        .iter()
        .copied()
        .find(|h| h.matches(screen))
        .unwrap_or(&EMPTY_HANDLER)
}
