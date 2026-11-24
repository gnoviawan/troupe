//! Troupe's Rune app root, shared by the web entry (`troupe-web`) and the desktop window
//! (`troupe-dev`). It connects to a Troupe server over `/ws`, authenticates, watches the
//! project registry, reads this device's shell layout and active project from the host store,
//! and draws the workspace shell: the rail with the server's projects, the main area's top row,
//! the side panel frame and the tool strip.
//!
//! The model crate owns the state rules (`projects`, `shell_layout`); this crate carries their
//! effects to the host and draws. Tabs, panes and the side panel's views are placeholders here.

#![forbid(unsafe_code)]

mod host;
mod sync;
mod view;

use client::Config;
use client::link::Status;
use model::{projects, shell_layout};
use rune::{AppSpec, FontSmoothing, Key, KeyEvent, Tx, Waker};
use std::sync::OnceLock;
use ui::overlay::Overlays;

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
    pub theme: &'static str,
    /// The rail or side panel width while its edge is dragged; committed on release.
    pub drag_width: Option<(Edge, f64)>,
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
            drag_width: None,
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

fn setup(state: &mut State, wake: Waker) {
    ui::overlay::history::install(wake.clone());
    if let Some(config) = LAUNCH.get().and_then(|l| l.config.clone()) {
        state.connect(Host::connect(config, wake));
    }
}

fn poll(state: &mut State, now: f64, _tx: &mut Tx) -> bool {
    let mut changed = false;
    for _ in 0..ui::overlay::history::take_back_presses() {
        changed |= state.overlays.pop().is_some();
    }
    changed | state.pump(now)
}

fn on_key(state: &mut State, key: &KeyEvent, _tx: &mut Tx) -> bool {
    matches!(key.key, Key::Escape) && state.overlays.pop().is_some()
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
