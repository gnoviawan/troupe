//! Between the server and the model: what the app asks the host for once connected, how
//! `/ws` outputs become model events, and how model effects become host requests.

use std::collections::HashMap;

use client::CallId;
use client::link::Status;
use client::ws::{Command, Output};
use model::projects::{Event as ProjectEvent, ProjectList};
use model::shell_layout::{Event as LayoutEvent, ReadOutcome};
use model::persistence;
use model::{Effect, Ids, Level, shell_layout};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsRequestType, WsWatchName};

use crate::{Host, State};

const ACTIVE_PROJECT_KEY: &str = "active-project";

/// What a pending call's reply is for.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    ReadLayout,
    ReadActiveProject,
    /// A project's workspace record, read when the project becomes visible.
    ReadWorkspace(String),
    Write(String),
}

/// The device's saved selection: unread, or read (`None` when it has none).
#[derive(Clone, Debug, Default, PartialEq)]
enum Saved {
    #[default]
    Unread,
    Read(Option<String>),
}

#[derive(Default)]
pub struct Sync {
    pending: HashMap<CallId, Pending>,
    next_call: CallId,
    saved_active: Saved,
    /// A registry list that came before the saved selection.
    held: Option<ProjectList>,
    /// The project whose workspace record is being read.
    reading: Option<String>,
    /// The last record written per project, so an unchanged one is not written again.
    written: HashMap<String, Value>,
    /// The workspace revision the last persist saw.
    persisted_revision: u64,
}

/// Ids for a store that mints them (groups); the shell creates none yet.
struct Counter(u64);

impl Ids for Counter {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("rune-{}", self.0)
    }
}

impl State {
    /// Takes the link and asks for everything the shell shows.
    pub(crate) fn connect(&mut self, host: Host) {
        self.host = Some(host);
        self.begin();
    }

    /// The requests a fresh link sends: the projects watch and the two device reads (calls 1
    /// and 2).
    pub(crate) fn begin(&mut self) {
        self.status = Status::Connecting;
        self.send(Command::Watch {
            watch: WsWatchName::Projects,
            scope: String::new(),
        });
        self.call(
            WsRequestType::StoreRead,
            json!({ "key": self.device_key(shell_layout::KEY) }),
            Pending::ReadLayout,
        );
        self.call(
            WsRequestType::StoreRead,
            json!({ "key": self.device_key(ACTIVE_PROJECT_KEY) }),
            Pending::ReadActiveProject,
        );
        self.layout_step(LayoutEvent::StartHydrate);
    }

    fn device_key(&self, key: &str) -> String {
        format!("device/{}/{key}", self.device_id)
    }

    fn send(&mut self, command: Command) {
        if let Some(host) = &mut self.host {
            host.send(command);
        }
    }

    fn call(&mut self, kind: WsRequestType, payload: Value, pending: Pending) {
        self.sync.next_call += 1;
        let call = self.sync.next_call;
        self.sync.pending.insert(call, pending);
        self.send(Command::Call {
            call,
            kind,
            payload,
            ledgered: false,
        });
    }

    /// Runs the outputs the link queued; true when the state changed.
    pub(crate) fn pump(&mut self, now: f64) -> bool {
        let Some(host) = &mut self.host else {
            return false;
        };
        let outputs = host.drain(now);
        let changed = !outputs.is_empty();
        for output in outputs {
            self.apply(output);
        }
        changed
    }

    /// One `/ws` output.
    pub fn apply(&mut self, output: Output) {
        match output {
            Output::Status(status) => {
                if let Status::Halted { code, message } = &status {
                    log::warn!("[app] the server refused this client: {code}: {message}");
                }
                self.status = status;
            }
            Output::WatchState {
                watch: WsWatchName::Projects,
                state,
                ..
            }
            | Output::WatchDelta {
                watch: WsWatchName::Projects,
                delta: state,
                ..
            } => match serde_json::from_value::<ProjectList>(state) {
                Ok(list) => self.registry_list(list),
                Err(e) => log::warn!("[app] unreadable projects frame: {e}"),
            },
            Output::WatchRefused { watch, error, .. } => {
                log::warn!(
                    "[app] watch {watch:?} refused: {}: {}",
                    error.code,
                    error.message
                );
            }
            Output::Reply { call, result } => self.reply(call, result),
            _ => {}
        }
    }

    fn reply(&mut self, call: CallId, result: Result<Option<Value>, troupe_protocol::ws::WsError>) {
        let Some(pending) = self.sync.pending.remove(&call) else {
            return;
        };
        match pending {
            Pending::ReadLayout => {
                let outcome = match result {
                    Ok(reply) => stored_value(reply).map(|data| ReadOutcome::Found { data }),
                    Err(e) => Some(ReadOutcome::Failed { code: e.code }),
                };
                self.layout_step(LayoutEvent::Hydrate(outcome));
            }
            Pending::ReadActiveProject => {
                let saved = match result {
                    Ok(reply) => stored_value(reply)
                        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)),
                    Err(e) => {
                        log::warn!("[app] reading the active project failed: {}", e.code);
                        None
                    }
                };
                self.sync.saved_active = Saved::Read(saved);
                if let Some(list) = self.sync.held.take() {
                    self.registry_list(list);
                }
            }
            Pending::ReadWorkspace(project_id) => {
                let raw = match result {
                    Ok(reply) => stored_value(reply),
                    Err(e) => {
                        log::warn!("[app] reading the workspace of {project_id} failed: {}", e.code);
                        None
                    }
                };
                self.workspace_read(&project_id, raw);
            }
            Pending::Write(key) => {
                if let Err(e) = result {
                    log::warn!("[app] writing {key} failed: {}: {}", e.code, e.message);
                }
            }
        }
    }

    fn registry_list(&mut self, list: ProjectList) {
        let saved = match &self.sync.saved_active {
            Saved::Unread if !self.projects.store.is_loaded => {
                self.sync.held = Some(list);
                return;
            }
            Saved::Read(saved) => saved.clone(),
            Saved::Unread => None,
        };
        self.projects_step(ProjectEvent::ApplyRegistryList(list, saved));
    }

    pub fn projects_step(&mut self, event: ProjectEvent) {
        let out = projects::step(&mut self.projects, event, &mut Counter(0));
        self.run(out.effects);
        self.show_active_project();
    }

    /// Bring the selected project's tree on screen: its parked tree at once, else after its
    /// record is read. The project being left is saved first.
    fn show_active_project(&mut self) {
        let Some(project_id) = self.active_project().map(|p| p.id.clone()) else {
            return;
        };
        if self.workspace.project() == Some(project_id.as_str())
            || self.sync.reading.as_deref() == Some(project_id.as_str())
        {
            return;
        }
        self.persist_workspace();
        if self.workspace.has_tree(&project_id) {
            self.workspace.activate(&project_id, None);
            return;
        }
        self.sync.reading = Some(project_id.clone());
        let key = self.device_key(&persistence::key(&project_id));
        self.call(
            WsRequestType::StoreRead,
            json!({ "key": key }),
            Pending::ReadWorkspace(project_id),
        );
    }

    fn workspace_read(&mut self, project_id: &str, raw: Option<Value>) {
        if self.sync.reading.as_deref() != Some(project_id) {
            return;
        }
        self.sync.reading = None;
        let root = self
            .projects
            .store
            .projects
            .iter()
            .find(|p| p.id == project_id)
            .and_then(|p| p.path.clone());
        let mut effects = Vec::new();
        let hydrated = persistence::hydrate(
            raw,
            root.as_deref(),
            &[],
            None,
            self.workspace.ids(),
            &mut effects,
        );
        self.run(effects);
        self.workspace.activate(project_id, Some(hydrated));
        if self.active_project().map(|p| p.id.as_str()) != Some(project_id) {
            self.show_active_project();
        }
    }

    /// Write the visible project's record when it changed (`persistState`), and carry out what
    /// the workspace asked for.
    pub(crate) fn persist_workspace(&mut self) {
        let logs = std::mem::take(&mut self.workspace.logs);
        self.run(logs);
        for request in std::mem::take(&mut self.workspace.requests) {
            log::info!("[app] workspace request not wired in the Rune shell yet: {request:?}");
        }
        if self.workspace.revision == self.sync.persisted_revision {
            return;
        }
        self.sync.persisted_revision = self.workspace.revision;
        let (Some(project_id), Some(record)) = (
            self.workspace.project().map(str::to_owned),
            self.workspace.record(),
        ) else {
            return;
        };
        let value = persistence::js_numbers(serde_json::to_value(record).unwrap_or(Value::Null));
        if self.sync.written.get(&project_id) == Some(&value) {
            return;
        }
        self.sync.written.insert(project_id.clone(), value.clone());
        self.write(&persistence::key(&project_id), value);
    }

    pub fn layout_step(&mut self, event: LayoutEvent) {
        let effects = shell_layout::step(&mut self.layout, event);
        self.run(effects);
    }

    /// Write a device key in the web client's envelope (`{ _version, data }`).
    fn write(&mut self, key: &str, value: Value) {
        let key = self.device_key(key);
        self.call(
            WsRequestType::StoreWrite,
            json!({ "key": key, "value": persistence::stored(value) }),
            Pending::Write(key.clone()),
        );
    }

    fn run(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Log {
                    level,
                    source,
                    message,
                } => match level {
                    Level::Info => log::info!("[{source}] {message}"),
                    Level::Warn | Level::Error => log::warn!("[{source}] {message}"),
                },
                Effect::WriteShellLayout { record, .. } => {
                    let value = serde_json::to_value(record).unwrap_or(Value::Null);
                    self.write(shell_layout::KEY, value);
                }
                Effect::SaveActiveProject { project_id } => {
                    self.write(ACTIVE_PROJECT_KEY, Value::from(project_id))
                }
                other => log::info!("[app] effect not wired in the shell yet: {other:?}"),
            }
        }
    }
}

use model::projects;

/// A `store_read` reply's value without the web client's envelope, or `None` when the key
/// is absent (revision 0).
fn stored_value(reply: Option<Value>) -> Option<Value> {
    let reply = reply?;
    if reply.get("revision").and_then(Value::as_u64).unwrap_or(0) == 0 {
        return None;
    }
    reply
        .get("value")
        .cloned()
        .map(persistence::unwrap_stored)
        .filter(|v| !v.is_null())
}
