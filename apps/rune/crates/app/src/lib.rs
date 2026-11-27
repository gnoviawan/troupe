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

mod agent_host;
mod agent_switch;
#[cfg(test)]
mod agent_switch_tests;
mod answers;
mod bug_report;
#[cfg(test)]
mod bug_report_tests;
mod automation_canvas;
mod automation_editor;
mod automation_editor_view;
mod automation_fields;
mod automation_inspector;
mod automation_policy;
mod automation_run_view;
mod automations;
#[cfg(test)]
mod automations_tests;
mod automations_view;
#[cfg(test)]
mod answers_tests;
#[cfg(test)]
mod cards_tests;
mod chat;
mod chat_images;
mod chat_view;
mod command_editor;
mod command_editor_view;
mod command_history;
mod composer;
mod composer_agents;
mod composer_sheet;
mod composer_view;
mod controls;
mod editor_host;
mod viewer_host;
mod explorer_host;
mod first_run;
mod first_run_view;
#[cfg(test)]
mod first_run_tests;
mod phone_files;
#[cfg(test)]
mod explorer_host_tests;
#[cfg(test)]
mod editor_host_tests;
mod git;
mod git_diff;
mod git_history;
mod git_history_view;
#[cfg(test)]
mod git_history_tests;
#[cfg(test)]
mod git_tests;
mod git_view;
mod github;
mod github_item_view;
mod github_logic;
#[cfg(test)]
mod github_tests;
mod github_view;
#[cfg(test)]
mod controls_tests;
mod history_host;
mod host;
mod ids;
mod keys;
mod launch;
mod model_sheet;
#[cfg(test)]
mod model_sheet_tests;
mod palette;
mod phone;
mod rail;
mod rail_forms;
mod rail_menus;
mod mcp_catalog;
mod mcp_install;
mod mcp_install_view;
mod mcp_host;
mod project_settings;
mod project_settings_view;
mod rail_view;
mod settings;
mod settings_accounts;
mod settings_accounts_dialogs;
mod settings_mcp;
mod settings_mcp_dialog;
mod settings_mcp_json;
mod settings_pages;
mod settings_profiles;
mod settings_shortcuts;
mod settings_subagents;
#[cfg(test)]
mod settings_tests;
mod settings_view;
mod snapshots;
mod snapshots_view;
#[cfg(test)]
mod snapshots_tests;
mod sync;
mod term_host;
mod term_input;
mod term_keys;
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
    /// The workspace snapshots page (`/snapshots`).
    Snapshots,
    /// The URL named no screen (`*` in the web client's router); holds the path it named.
    NotFound(String),
}

impl Route {
    /// The screen a URL hash names, as the web client's hash router reads it.
    pub fn from_hash(hash: &str) -> Route {
        match hash.trim_start_matches('#') {
            "" | "/" => Route::Workspace,
            "/snapshots" => Route::Snapshots,
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

/// A bar the desktop shell draws above the frame (the in-window app menu on Linux and
/// Windows). It may push its open menus onto the overlay layer.
pub type ShellBar = fn(&State, &'static Tokens, &mut Vec<rune::El>) -> rune::El;

static SHELL_BAR: OnceLock<ShellBar> = OnceLock::new();

/// Sets the shell's bar; call before the app runs.
pub fn set_shell_bar(bar: ShellBar) {
    let _ = SHELL_BAR.set(bar);
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
    pub palette: palette::PaletteUi,
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
    /// The open text files and their editor tabs.
    pub editors: ui::editor::Editors,
    pub viewers: ui::viewers::Viewers,
    pub(crate) editor_host: editor_host::EditorHost,
    pub explorer: ui::explorer::Explorer,
    pub(crate) explorer_host: explorer_host::ExplorerHost,
    /// The git cache and the Changes panel's own state.
    pub git: git::GitUi,
    /// The git history reads and the History tab's own state.
    pub history: git_history::HistoryUi,
    /// The GitHub cache and the area's own state.
    pub github: github::GithubUi,
    /// The shared `settings/app` record.
    pub settings: model::settings::State,
    /// The Settings window: which one is open, its page and its own fields.
    pub settings_ui: settings_view::SettingsUi,
    /// The project settings window's draft and its own fields.
    pub project_ui: project_settings::ProjectSettingsUi,
    /// The MCP servers page, its dialogs and the host side of the registry.
    pub mcp_ui: settings_mcp::McpUi,
    pub subagents_ui: settings_subagents::SubagentsUi,
    pub accounts_ui: settings_accounts::AccountsUi,
    pub(crate) sync: sync::Sync,
    /// The Report a Bug dialog.
    pub bug: bug_report::BugReportUi,
    /// The web client's setup wizard, token screen and first-run onboarding.
    pub first_run: first_run::FirstRun,
    /// The snapshots page, its dialogs and `model::snapshots`.
    pub snapshots: snapshots::SnapshotsUi,
    /// The Automations tab's reads, selection and canvases.
    pub automations: automations::AutomationsUi,
    /// The window's waker, kept for `reload`.
    pub(crate) waker: Option<Waker>,
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
            palette: palette::PaletteUi::default(),
            keys: keys::KeysUi::default(),
            phone: phone::PhoneUi::default(),
            zoom: ui::Zoom::DEFAULT,
            workspace: Workspace::new(Box::new(Uuids::new(seed()))),
            chat: chat::ChatUi::default(),
            composer: composer::ComposerUi::default(),
            terms: terminal::TerminalUi::default(),
            editors: ui::editor::Editors::default(),
            viewers: ui::viewers::Viewers::default(),
            editor_host: editor_host::EditorHost::default(),
            explorer: ui::explorer::Explorer::default(),
            explorer_host: explorer_host::ExplorerHost::default(),
            git: git::GitUi::default(),
            history: git_history::HistoryUi::default(),
            github: github::GithubUi::default(),
            settings: model::settings::State::default(),
            settings_ui: settings_view::SettingsUi::default(),
            project_ui: project_settings::ProjectSettingsUi::default(),
            mcp_ui: settings_mcp::McpUi::default(),
            subagents_ui: settings_subagents::SubagentsUi::default(),
            accounts_ui: settings_accounts::AccountsUi::default(),
            sync: sync::Sync::default(),
            bug: bug_report::BugReportUi::default(),
            first_run: first_run::FirstRun::default(),
            snapshots: snapshots::SnapshotsUi::default(),
            automations: automations::AutomationsUi::default(),
            waker: None,
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

/// Drops every connection and all view state, then connects again as at launch: the desktop
/// menu's Reload, as a webview reload rebuilds the React client.
pub fn reload(state: &mut State) {
    let Some(wake) = state.waker.clone() else { return };
    log::info!("[app] reload");
    *state = State::default();
    setup(state, wake);
}

/// The shell's bar above the frame, if the entry has one.
pub(crate) fn shell_bar() -> Option<ShellBar> {
    SHELL_BAR.get().copied()
}

fn setup(state: &mut State, wake: Waker) {
    state.waker = Some(wake.clone());
    ui::overlay::history::install(wake.clone());
    state.chat.images = Some(chat_images::ChatImages::new(wake.clone()));
    state.viewers.images = Some(rune::ImageCache::new(wake.clone()));
    state.viewers.waker = Some(wake.clone());
    if let Some(config) = LAUNCH.get().and_then(|l| l.config.clone()) {
        // The web client asks `/health` first: a server in setup mode or one that refuses its
        // token shows a first-run screen before the link starts.
        if cfg!(target_arch = "wasm32") {
            state.first_run_start(config, wake);
            return;
        }
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
    changed |= state.pump_viewers();
    changed |= state.pump_terminals(now);
    changed |= state.blink_flipped();
    state.sync_chats();
    state.read_agents();
    changed |= state.flush_chat();
    let (retried, next_retry) = state.fire_retries(now);
    let (waited, next_wait) = state.fire_readvertise_waits(now);
    let (gave_up, next_arm) = state.expire_pending_arms(now);
    changed |= retried | waited | gave_up;
    state.editors.now = editor_host::epoch_ms();
    state.sync_drafts(now);
    state.sync_editors();
    state.sync_explorer();
    let next_git = state.git_tick();
    let next_github = state.github_tick();
    let next_mcp = state.mcp_tick(now);
    let next_catalog = state.catalog_tick(now);
    let next_accounts = state.accounts_tick(now);
    let next_completion = state.pump_command_editors(tx);
    state.history_tick();
    let next_first_run = state.first_run_tick(now);
    state.bug_report_tick(tx);
    if state.route == Route::Snapshots {
        state.load_snapshots();
    }
    let next_automation = state.automations_tick(now);
    let next_preview = state.automation_editor_tick(now);
    let waits = [
        state.toasts.next_expiry(),
        next_retry,
        next_wait,
        next_arm,
        state.next_blink(),
        state.next_find_refresh(),
        state.next_draft_sync(),
        next_git,
        next_github,
        next_mcp,
        next_first_run,
        next_catalog,
        next_accounts,
        next_completion,
        next_automation,
        next_preview,
    ];
    for due in waits.into_iter().flatten() {
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
    app_with(spec(launch))
}

/// The app's spec for `launch`, for a shell that changes the window before `app_with` builds
/// it. Call it once per process.
pub fn spec(launch: Launch) -> AppSpec<State> {
    if LAUNCH.set(launch).is_err() {
        log::warn!("[app] launched twice; keeping the first launch");
    }
    AppSpec {
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        min_size: Some((360.0, 400.0)),
        ..AppSpec::new("Troupe", (1280.0, 800.0), State::default, view)
    }
}

pub fn app_with(spec: AppSpec<State>) -> rune::App<State> {
    // Chromium's text on Linux: the autohinted outline on whole-pixel pens.
    rune::App::new(spec).font_smoothing(FontSmoothing::HintedGpu).on_ime(editor_host::on_ime)
}

#[cfg(test)]
mod tests;
