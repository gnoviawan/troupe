//! Troupe's Rune app root, shared by the web entry (`troupe-web`) and the desktop window
//! (`troupe-dev`). It connects to a Troupe server over `/ws`, authenticates, watches the
//! project registry, reads this device's shell layout and active project from the host store,
//! and draws the workspace shell: the rail with the server's projects, the main area's top row,
//! the side panel frame and the tool strip.
//!
//! The model crate owns the state rules (`projects`, `shell_layout`, `shortcuts`, `chats`); this
//! crate carries their effects to the host and draws. The rail's overlays live in `rail_menus`
//! and `rail_forms`, the theme picker in `theme_picker`, the shortcut menu and the key dispatch
//! in `keys`, and the phone shell in `phone`. Tabs, panes and the side panel's views are
//! placeholders here.

#![forbid(unsafe_code)]

mod answers;
#[cfg(test)]
mod answers_tests;
#[cfg(test)]
mod cards_tests;
mod chat;
mod chat_images;
mod chat_view;
mod command_history;
mod composer;
mod composer_agents;
mod composer_sheet;
mod composer_view;
mod controls;
#[cfg(test)]
mod controls_tests;
mod host;
mod ids;
mod keys;
mod launch;
mod phone;
mod rail;
mod rail_forms;
mod rail_menus;
mod rail_view;
mod sync;
mod term_host;
mod term_input;
mod term_menu;
mod term_select;
mod terminal;
mod terminal_view;
mod theme_picker;
mod view;

use client::Config;
use client::link::Status;
use model::chats::Chats;
use model::{projects, shell_layout, shortcuts};
use rune::{AppSpec, FontSmoothing, KeyEvent, Tx, Waker};
use std::sync::OnceLock;
use theme::Tokens;
use ui::overlay::{Overlays, ToastKind, Toasts};
use ui::workspace::{Uuids, Workspace};

pub use host::Host;
pub use view::view;

/// What the window shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Route {
    #[default]
    Workspace,
    /// The URL named no screen (`*` in the web client's router); holds the path it named.
    NotFound(String),
}

impl Route {
    /// The screen a URL hash names, as the web client's hash router reads it.
    pub fn from_hash(hash: &str) -> Route {
        match hash.trim_start_matches('#') {
            "" | "/" => Route::Workspace,
            path => Route::NotFound(path.to_owned()),
        }
    }
}

/// How an entry starts the app: the server to reach (none for a headless snapshot) and the
/// route the URL asked for.
#[derive(Clone, Debug, Default)]
pub struct Launch {
    pub config: Option<Config>,
    pub route: Route,
}

static LAUNCH: OnceLock<Launch> = OnceLock::new();

/// The overlay key of the phone shell's rail drawer.
pub const DRAWER: &str = "rail-drawer";

pub struct State {
    pub(crate) host: Option<Host>,
    pub(crate) device_id: String,
    pub(crate) server: String,
    pub status: Status,
    pub projects: projects::Model,
    pub layout: shell_layout::State,
    pub overlays: Overlays,
    pub route: Route,
    /// The colour theme family (`settings/app` `colorTheme`) and its appearance.
    pub theme: &'static str,
    pub dark: bool,
    /// The theme id the picker previews while it is open.
    pub preview: Option<&'static str>,
    /// The rail or side panel width while its edge is dragged; committed on release.
    pub drag_width: Option<(Edge, f64)>,
    pub chats: Chats,
    pub shortcuts: shortcuts::State,
    pub toasts: Toasts,
    /// The frame clock (seconds) `poll` last saw: toasts expire on it.
    pub now: f64,
    pub rail: rail::RailUi,
    pub picker: theme_picker::Picker,
    pub keys: keys::KeysUi,
    pub phone: phone::PhoneUi,
    /// `settings/app` `uiZoomLevel`.
    pub zoom: ui::Zoom,
    /// The visible project's panes and tabs.
    pub workspace: Workspace,
    /// The chats the workspace shows: their transcripts and the transcript's own UI state.
    pub chat: chat::ChatUi,
    /// The composers' drafts and what their menus list.
    pub composer: composer::ComposerUi,
    /// The terminals: their link, sessions and cells.
    pub terms: terminal::TerminalUi,
    pub(crate) sync: sync::Sync,
}

/// A resizable edge of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Rail,
    SidePanel,
}

impl Default for State {
    fn default() -> Self {
        let launch = LAUNCH.get().cloned().unwrap_or_default();
        State {
            host: None,
            device_id: launch
                .config
                .as_ref()
                .map(|c| c.device_id.clone())
                .unwrap_or_default(),
            server: launch
                .config
                .as_ref()
                .map(|c| c.base_url.clone())
                .unwrap_or_default(),
            status: Status::Stopped,
            projects: projects::Model::default(),
            layout: shell_layout::State::default(),
            overlays: Overlays::default(),
            route: launch.route,
            theme: theme::DEFAULT_FAMILY,
            dark: true,
            preview: None,
            drag_width: None,
            chats: Chats::default(),
            shortcuts: shortcuts::State::new(keys::platform()),
            toasts: Toasts::default(),
            now: 0.0,
            rail: rail::RailUi::default(),
            picker: theme_picker::Picker::default(),
            keys: keys::KeysUi::default(),
            phone: phone::PhoneUi::default(),
            zoom: ui::Zoom::DEFAULT,
            workspace: Workspace::new(Box::new(Uuids::new(seed()))),
            chat: chat::ChatUi::default(),
            composer: composer::ComposerUi::default(),
            terms: terminal::TerminalUi::default(),
            sync: sync::Sync::default(),
        }
    }
}

impl State {
    /// The project the workspace shows.
    pub fn active_project(&self) -> Option<&projects::Project> {
        let store = &self.projects.store;
        store
            .projects
            .iter()
            .find(|p| p.id == store.active_project_id)
    }

    /// The tokens the window paints with: the picker's preview, else the saved theme.
    pub fn tokens(&self) -> &'static Tokens {
        self.preview
            .and_then(theme::by_id)
            .unwrap_or_else(|| theme::tokens(self.theme, self.dark))
    }

    /// Switches to `id`, closes the phone drawer and returns to the workspace.
    pub fn select_project(&mut self, id: &str) {
        self.projects_step(projects::Event::SelectProject(id.to_owned()));
        self.overlays.close(DRAWER);
        self.route = Route::Workspace;
    }

    /// A toast that names a placeholder: the action ports in a later unit.
    pub fn toast_later(&mut self, what: &str) {
        self.toasts.push(self.now, ToastKind::Info, what, Some("Not in the Rune client yet"));
    }

    pub fn toast_error(&mut self, title: &str, detail: &str) {
        let detail = (!detail.is_empty()).then_some(detail);
        self.toasts.push(self.now, ToastKind::Error, title, detail);
    }

    /// A width the drag is moving, else the saved one.
    pub fn width(&self, edge: Edge) -> f64 {
        match self.drag_width {
            Some((dragged, width)) if dragged == edge => width,
            _ => match edge {
                Edge::Rail => self.layout.record.rail_width,
                Edge::SidePanel => self.layout.record.side_panel_width,
            },
        }
    }
}

/// A different seed each run, so pane ids minted now never repeat ids a saved tree holds.
#[cfg(not(target_arch = "wasm32"))]
fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos ^ u64::from(std::process::id()).rotate_left(32)
}

#[cfg(target_arch = "wasm32")]
fn seed() -> u64 {
    (js_sys::Math::random() * 9_007_199_254_740_992.0) as u64 ^ js_sys::Date::now() as u64
}

fn setup(state: &mut State, wake: Waker) {
    ui::overlay::history::install(wake.clone());
    state.chat.images = Some(chat_images::ChatImages::new(wake.clone()));
    if let Some(config) = LAUNCH.get().and_then(|l| l.config.clone()) {
        state.terms.host = Some(term_host::TermHost::connect(config.clone(), wake.clone()));
        state.connect(Host::connect(config, wake));
    }
}

fn poll(state: &mut State, now: f64, tx: &mut Tx) -> bool {
    state.now = now;
    let mut changed = state.toasts.expire(now);
    for _ in 0..ui::overlay::history::take_back_presses() {
        changed |= state.close_top();
    }
    changed |= state.pump(now);
    changed |= state.pump_terminals(now);
    state.sync_chats();
    state.read_agents();
    changed |= state.flush_chat();
    let (retried, next_retry) = state.fire_retries(now);
    let (waited, next_wait) = state.fire_readvertise_waits(now);
    changed |= retried | waited;
    for due in [state.toasts.next_expiry(), next_retry, next_wait, state.next_blink()].into_iter().flatten() {
        tx.wake_at(due);
    }
    state.persist_workspace();
    changed
}

fn on_key(state: &mut State, key: &KeyEvent, tx: &mut Tx) -> bool {
    keys::on_key(state, key, tx)
}

/// The app for `launch`. The entries call `run` on it, the desktop one after linking the live
/// driver. Call it once per process.
pub fn app(launch: Launch) -> rune::App<State> {
    if LAUNCH.set(launch).is_err() {
        log::warn!("[app] launched twice; keeping the first launch");
    }
    let spec = AppSpec {
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        min_size: Some((360.0, 400.0)),
        ..AppSpec::new("Troupe", (1280.0, 800.0), State::default, view)
    };
    // Chromium's text on Linux: the autohinted outline on whole-pixel pens.
    rune::App::new(spec).font_smoothing(FontSmoothing::HintedGpu)
}

#[cfg(test)]
mod tests;
