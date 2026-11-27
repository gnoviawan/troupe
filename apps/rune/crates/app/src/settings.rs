//! The app settings driver: `model::settings` fed by the shared `settings/app` read, its
//! effects carried to the host (the shared write, the idle-window routes, the shell panels),
//! and the window's look (theme, appearance, zoom) read back from the record after each step.
//! The Settings window itself is `settings_view`.

use client::http::{HttpError, HttpRequest, Method};
use model::settings::routes::Request;
use model::settings::{self, Effect, Event, IdleWindow, Panel, Read};
use model::shell_layout::Event as Layout;
use serde_json::{Value, json};

use crate::State;
use crate::host::Fetch;
use crate::sync::Shared;

/// An idle-window request in flight; a push carries its epoch.
#[derive(Clone, Debug, PartialEq)]
pub enum IdleFetch {
    Read,
    Push(u64),
}

impl State {
    pub(crate) fn settings_step(&mut self, event: Event) {
        let effects = settings::step(&mut self.settings, event);
        self.settings_run(effects);
        self.read_settings_look();
    }

    /// One setting, written as `useUpdateAppSetting` does. Only the key reaches the log: a value
    /// can hold secrets, such as a terminal profile's environment.
    pub(crate) fn set_setting(&mut self, key: &str, value: Value) {
        log::info!("[settings] {key} changed");
        self.settings_step(Event::UpdateSetting(key.to_owned(), value));
    }

    /// The boot read of `settings/app` answered: `None` is a failed read with its code.
    pub(crate) fn settings_loaded(&mut self, read: Result<Option<Value>, String>) {
        let read = match read {
            Ok(Some(value)) => Read::Found(value),
            Ok(None) => Read::Missing,
            Err(code) => Read::Failed(code),
        };
        self.settings_step(Event::Loaded {
            read,
            system_dark: system_dark(),
        });
    }

    fn settings_run(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                // The web client's debounce coalesces a burst of edits; a write per edit is
                // the same last-writer-wins result.
                Effect::Write { record, .. } => self.write_shared(Shared::AppSettings, Value::Object(record)),
                Effect::PushTimeout { timeout, secs } => {
                    log::debug!("[settings] {} {secs} is a no-op on the server", timeout.op());
                }
                Effect::ReadIdleWindow => self.idle_request(Request::ReadIdleWindow, IdleFetch::Read),
                Effect::PushIdleWindow { secs, epoch } => {
                    if self.settings.idle_push_is_current(epoch) {
                        self.idle_request(Request::PushIdleWindow { secs }, IdleFetch::Push(epoch));
                    }
                }
                Effect::ShowPanel { panel, visible } => {
                    let event = match panel {
                        Panel::Rail => Layout::SetRailExpanded(json!(visible)),
                        Panel::SidePanel => Layout::SetSidePanelVisible(json!(visible)),
                        Panel::SshSection => Layout::SetSshSectionVisible(json!(visible)),
                    };
                    self.layout_step(event);
                }
                Effect::Log { level, source, message } => match level {
                    model::Level::Error => log::error!("[{source}] {message}"),
                    model::Level::Warn => log::warn!("[{source}] {message}"),
                    model::Level::Info => log::info!("[{source}] {message}"),
                },
            }
        }
    }

    fn idle_request(&mut self, request: Request, tag: IdleFetch) {
        let route = request.route();
        let http = HttpRequest {
            method: if route.post { Method::Post } else { Method::Get },
            path: route.path.to_owned(),
            body: route.body,
            command_id: None,
        };
        match &mut self.host {
            Some(host) => host.request(http, Fetch::Settings(tag)),
            None => log::debug!("[settings] no host for {}", route.path),
        }
    }

    pub(crate) fn idle_replied(&mut self, tag: IdleFetch, result: Result<Value, HttpError>) {
        match (tag, result) {
            (IdleFetch::Read, Ok(reply)) => {
                let live = serde_json::from_value::<IdleWindow>(reply).map_err(|e| format!("agent idle window response had no data: {e}"));
                self.settings_step(Event::IdleWindowRead(live));
            }
            (IdleFetch::Read, Err(e)) => self.settings_step(Event::IdleWindowRead(Err(message(&e)))),
            (IdleFetch::Push(_), Ok(_)) => {}
            (IdleFetch::Push(_), Err(e)) => self.settings_step(Event::IdleWindowPushFailed(message(&e))),
        }
    }

    /// The theme, appearance and zoom the record holds now.
    fn read_settings_look(&mut self) {
        let family = self.settings.str("colorTheme").to_owned();
        if !family.is_empty() {
            self.theme = theme::tokens(&family, true).family.as_str();
        }
        self.dark = self.settings.str("appearanceMode") != "light";
        if let Some(k) = self.settings.get("uiZoomLevel").as_f64() {
            self.zoom = ui::Zoom::new(k as f32);
        }
    }
}

/// The text an `AcpTransportError` carries for a failed request.
fn message(error: &HttpError) -> String {
    match error {
        HttpError::Refused { message, .. } => message.clone(),
        HttpError::Network(detail) => detail.clone(),
        HttpError::InProgress => error.to_string(),
    }
}

/// The OS appearance a stored `system` resolves to (`getSystemAppearance`).
#[cfg(target_arch = "wasm32")]
pub(crate) fn system_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .is_none_or(|m| m.matches())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn system_dark() -> bool {
    true
}
