use backend::AppState;
use crossterm::event::KeyCode;
use ratatui::Frame;

use crate::{app::App, navigation::Screen};

mod asking_passphrase;
mod dashboard;
mod onboarding_wants_encryption;
mod onboarding_wants_passphrase;

type AppEffect = Box<dyn FnOnce(&mut App)>;

struct ScreenHandler<S> {
    matches: fn(&Screen) -> bool,
    get: fn(&Screen) -> Option<&S>,
    get_mut: fn(&mut Screen) -> Option<&mut S>,
    render: fn(&mut Frame, &AppState, &S),
    handle_key: fn(&AppState, KeyCode, &mut S) -> Option<AppEffect>,
    handle_tick: fn(&AppState, &mut S) -> Option<AppEffect>,
}

pub(crate) trait AnyScreenHandler {
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

pub(crate) fn get_handlers() -> Vec<Box<dyn AnyScreenHandler>> {
    vec![
        Box::new(dashboard::dashboard_handler()),
        Box::new(onboarding_wants_encryption::onboarding_wants_encryption_handler()),
        Box::new(onboarding_wants_passphrase::onboarding_wants_passphrase_handler()),
        Box::new(asking_passphrase::asking_passphrase_handler()),
    ]
}

static EMPTY_HANDLER: ScreenHandler<()> = ScreenHandler {
    matches: |_| true,
    get: |_| None,
    get_mut: |_| None,
    render: |_, _, _| {},
    handle_key: |_, _, _| None,
    handle_tick: |_, _| None,
};

pub(crate) fn get_handler_for_screen<'a>(
    handlers: &'a [Box<dyn AnyScreenHandler>],
    screen: &Screen,
) -> &'a dyn AnyScreenHandler {
    handlers
        .iter()
        .find(|h| h.matches(screen))
        .map(|h| h.as_ref())
        .unwrap_or(&EMPTY_HANDLER)
}
