//! The app settings: the `settings/app` record every device shares, with the boot read and its
//! migrations, a change synced from another device, edits, the immediate write, the reset, and
//! the agent idle-window pushes. Port of `src/renderer/stores/app-settings-store.ts` and the
//! web path of `src/renderer/hooks/use-app-settings.ts`.
//!
//! The record stays a JSON object, as the React store spreads it: keys this client does not
//! know survive a round trip. Every Rune client reaches the host over `/ws` and HTTP, so the
//! web transport's rules hold for all of them: the four ACP timeout pushes are no-ops on the
//! server (the app logs and drops them) and the idle window is read from the host after boot.

mod normalize;
pub mod routes;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Level;

pub use normalize::{defaults, normalize};

/// The host store key (`PersistenceKeys.appSettings`).
pub const KEY: &str = "settings/app";
/// The window the host uses when nothing is set (`AGENT_IDLE_WINDOW_OPTIONS`' null row).
pub const DEFAULT_IDLE_WINDOW_SECS: u64 = 1800;

/// What the boot read of `settings/app` found (`AppSettingsHostRead`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostRead {
    #[default]
    Pending,
    Found,
    Missing,
    /// The store holds defaults that are not the user's; the host refuses writes until a read works.
    Failed,
}

/// The store as the recorder snapshots it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub settings: Map<String, Value>,
    pub is_loaded: bool,
    pub host_read: HostRead,
    #[serde(skip)]
    idle: IdleBook,
}

/// `idleWindowEpoch` and what the boot read saw: a newer idle-window choice voids an older read
/// or push still in flight.
#[derive(Clone, Debug, Default, PartialEq)]
struct IdleBook {
    epoch: u64,
    /// The epoch and the saved value when the boot read asked the host for its window.
    reading: Option<(u64, Value)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            settings: defaults(),
            is_loaded: false,
            host_read: HostRead::Pending,
            idle: IdleBook::default(),
        }
    }
}

/// How the boot read of the key answered.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Read {
    /// The stored record; a JavaScript-falsy one reads as missing, as `result.data` does.
    Found(Value),
    Missing,
    /// The read failed with this code.
    Failed(String),
}

/// What the host says its idle window is (`AgentIdleWindowStatus`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct IdleWindow {
    pub secs: f64,
    /// `default`, `setting`, `env` or `operator`.
    pub source: String,
}

/// An ACP timeout the settings push to the host (`acpApi.set*Timeout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Timeout {
    TurnTimeout,
    TurnIdleTimeout,
    SessionNewTimeout,
    SessionReopenTimeout,
}

impl Timeout {
    pub const ALL: [Timeout; 4] = [
        Timeout::TurnTimeout,
        Timeout::TurnIdleTimeout,
        Timeout::SessionNewTimeout,
        Timeout::SessionReopenTimeout,
    ];

    /// The `settings/app` field that holds it.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Timeout::TurnTimeout => "acpTurnTimeoutSecs",
            Timeout::TurnIdleTimeout => "acpTurnIdleTimeoutSecs",
            Timeout::SessionNewTimeout => "acpSessionNewTimeoutSecs",
            Timeout::SessionReopenTimeout => "acpSessionReopenTimeoutSecs",
        }
    }

    /// The `acpApi` method, as the recorder names the call.
    #[must_use]
    pub fn op(self) -> &'static str {
        match self {
            Timeout::TurnTimeout => "setTurnTimeout",
            Timeout::TurnIdleTimeout => "setTurnIdleTimeout",
            Timeout::SessionNewTimeout => "setSessionNewTimeout",
            Timeout::SessionReopenTimeout => "setSessionReopenTimeout",
        }
    }
}

/// A shell-layout panel the reset returns to its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Rail,
    SidePanel,
    SshSection,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Write the whole record: now when `immediate`, else through the debounced writer.
    Write {
        record: Map<String, Value>,
        immediate: bool,
    },
    /// Push an ACP timeout (a no-op on the server).
    PushTimeout { timeout: Timeout, secs: Value },
    /// `GET /host/agent-idle-window`; answer with [`Event::IdleWindowRead`].
    ReadIdleWindow,
    /// `POST /host/agent-idle-window` unless [`State::idle_push_is_current`] says a newer
    /// choice replaced it; a failure answers [`Event::IdleWindowPushFailed`].
    PushIdleWindow { secs: Value, epoch: u64 },
    /// Show or hide a shell panel through the shell-layout store.
    ShowPanel { panel: Panel, visible: bool },
    Log {
        level: Level,
        source: &'static str,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The boot read answered (`useAppSettingsLoader`). `system_dark` is the OS appearance a
    /// stored `system` resolves to.
    Loaded { read: Read, system_dark: bool },
    /// The host's idle window, or why the read failed.
    IdleWindowRead(Result<IdleWindow, String>),
    /// Another device or tab changed the key (`useSharedKeySync`): what it holds now.
    SharedChange {
        data: Option<Value>,
        system_dark: bool,
    },
    /// `useUpdateAppSetting`.
    UpdateSetting(String, Value),
    /// `useUpdateAppSettings`.
    UpdateSettings(Map<String, Value>),
    /// `writeAppSettingsNow`.
    WriteNow(Map<String, Value>),
    /// `useResetAppSettings`, after the user confirmed.
    Reset,
    /// An ACP timeout choice (`handleAcp*Change`): write it, then push it.
    SetTimeout(Timeout, Value),
    /// The idle-window choice (`handleAgentIdleWindowChange`).
    SetAgentIdleWindow(Value),
    /// The idle-window push failed with this message.
    IdleWindowPushFailed(String),
}

impl State {
    /// A typed read of one field, with the default when the record holds something else.
    #[must_use]
    pub fn get(&self, key: &str) -> &Value {
        self.settings.get(key).unwrap_or(&Value::Null)
    }

    #[must_use]
    pub fn str(&self, key: &str) -> &str {
        self.get(key).as_str().unwrap_or_default()
    }

    #[must_use]
    pub fn bool(&self, key: &str) -> bool {
        self.get(key) == &Value::Bool(true)
    }

    /// Whether a push minted at `epoch` is still the newest idle-window choice.
    #[must_use]
    pub fn idle_push_is_current(&self, epoch: u64) -> bool {
        epoch == self.idle.epoch
    }

    fn write(&self, immediate: bool) -> Effect {
        Effect::Write {
            record: self.settings.clone(),
            immediate,
        }
    }

    fn set(&mut self, key: &str, value: Value) {
        self.settings.insert(key.to_owned(), value);
    }

    fn push_idle(&mut self, secs: Value) -> Effect {
        self.idle.epoch += 1;
        Effect::PushIdleWindow {
            secs,
            epoch: self.idle.epoch,
        }
    }
}

fn log(level: Level, source: &'static str, message: String) -> Effect {
    Effect::Log {
        level,
        source,
        message,
    }
}

/// JavaScript truthiness of a stored record (`result.data`).
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

pub fn step(state: &mut State, event: Event) -> Vec<Effect> {
    let mut effects = Vec::new();
    match event {
        Event::Loaded { read, system_dark } => {
            let mut found = false;
            let failed = match read {
                Read::Found(data) if truthy(&data) => {
                    found = true;
                    let out = normalize(&data, system_dark, &mut effects);
                    state.settings = out.settings;
                    state.is_loaded = true;
                    state.host_read = HostRead::Found;
                    if out.changed {
                        effects.push(state.write(false));
                    }
                    None
                }
                Read::Failed(code) if code != "KEY_NOT_FOUND" => Some(code),
                Read::Found(_) | Read::Missing | Read::Failed(_) => None,
            };
            if !found {
                state.settings = defaults();
                state.is_loaded = true;
                state.host_read = if failed.is_some() {
                    HostRead::Failed
                } else {
                    HostRead::Missing
                };
            }
            if let Some(code) = failed {
                effects.push(log(
                    Level::Warn,
                    "useAppSettingsLoader",
                    format!("App settings read failed ({code}); using defaults"),
                ));
            }
            for timeout in Timeout::ALL {
                effects.push(Effect::PushTimeout {
                    timeout,
                    secs: state.get(timeout.key()).clone(),
                });
            }
            state.idle.reading = Some((state.idle.epoch, state.get("agentIdleWindowSecs").clone()));
            effects.push(Effect::ReadIdleWindow);
        }
        Event::IdleWindowRead(reply) => {
            let Some((epoch, saved)) = state.idle.reading.take() else {
                return effects;
            };
            match reply {
                Err(message) => effects.push(log(
                    Level::Error,
                    "useAppSettingsLoader.agentIdleWindow",
                    format!("Failed to apply the agent idle window: {message}"),
                )),
                Ok(_) if epoch != state.idle.epoch => {}
                Ok(live) if live.source == "setting" || live.source == "default" => {
                    let default =
                        live.source == "default" || live.secs == DEFAULT_IDLE_WINDOW_SECS as f64;
                    let next = if default {
                        Value::Null
                    } else {
                        json!(live.secs)
                    };
                    if !same_number(&next, &saved) {
                        state.set("agentIdleWindowSecs", next);
                        effects.push(state.write(false));
                    }
                }
                Ok(_) => {}
            }
        }
        Event::SharedChange {
            data: Some(data),
            system_dark,
        } if truthy(&data) => {
            state.settings = normalize(&data, system_dark, &mut effects).settings;
        }
        Event::SharedChange { .. } => {}
        Event::UpdateSetting(key, value) => {
            state.set(&key, value);
            effects.push(state.write(false));
        }
        Event::UpdateSettings(updates) => {
            state.settings.extend(updates);
            effects.push(state.write(false));
        }
        Event::WriteNow(updates) => {
            state.settings.extend(updates);
            effects.push(state.write(true));
        }
        Event::Reset => {
            let keep = ["terminalProfiles", "onboardingCompletedAt"]
                .map(|key| (key, state.get(key).clone()));
            state.settings = defaults();
            for (key, value) in keep {
                state.set(key, value);
            }
            for (panel, visible) in [
                (Panel::Rail, true),
                (Panel::SidePanel, false),
                (Panel::SshSection, true),
            ] {
                effects.push(Effect::ShowPanel { panel, visible });
            }
            effects.push(state.write(true));
            for timeout in Timeout::ALL {
                effects.push(Effect::PushTimeout {
                    timeout,
                    secs: Value::Null,
                });
            }
            effects.push(state.push_idle(Value::Null));
        }
        Event::SetTimeout(timeout, secs) => {
            state.set(timeout.key(), secs.clone());
            effects.push(state.write(false));
            effects.push(Effect::PushTimeout { timeout, secs });
        }
        Event::SetAgentIdleWindow(secs) => {
            // The push is queued before the write but runs after it, as the React promise chain does.
            let push = state.push_idle(secs.clone());
            state.set("agentIdleWindowSecs", secs);
            effects.push(state.write(false));
            effects.push(push);
        }
        Event::IdleWindowPushFailed(message) => effects.push(log(
            Level::Error,
            "queueAgentIdleWindowPush",
            format!("Failed to apply the agent idle window: {message}"),
        )),
    }
    effects
}

/// `!==` on two JSON numbers or nulls: `3600` equals `3600.0`.
fn same_number(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests;
