//! Between the server and the model: what the app asks the host for once connected, how
//! `/ws` outputs become model events, and how model effects become host requests.
//!
//! Stored values travel in the React client's envelope (`{ _version, data }`), so both clients
//! read what the other wrote. Shared keys (`shared/lww/...`) carry the revision this client
//! last saw as `baseRevision`; a shared key is never written before it was read.

use std::collections::{HashMap, HashSet};

use client::CallId;
use client::link::Status;
use client::ws::{Command, Output};
use model::chats::Chat;
use model::prefs::Record;
use model::projects::{Event as ProjectEvent, ProjectList};
use model::shell_layout::{Event as LayoutEvent, ReadOutcome};
use model::persistence;
use model::{Effect, Ids, Level, shell_layout};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType, WsWatchName};

use crate::{Host, State};

const ACTIVE_PROJECT_KEY: &str = "active-project";
const PREFS_KEY: &str = "project-prefs";

/// A key every device shares, written last-writer-wins on the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shared {
    /// `settings/app`: the colour theme and appearance, among fields other screens own.
    AppSettings,
    /// `settings/keyboard-shortcuts`.
    Shortcuts,
}

impl Shared {
    fn host_key(self) -> &'static str {
        match self {
            Shared::AppSettings => "shared/lww/settings/app",
            Shared::Shortcuts => "shared/lww/settings/keyboard-shortcuts",
        }
    }
}

/// What a pending call's reply is for.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    ReadLayout,
    ReadActiveProject,
    ReadPrefs,
    ReadShared(Shared),
    WriteShared(Shared),
    ListChats,
    Write(String),
    WritePrefs(String),
    /// A host command; a refusal shows as a toast with this text.
    Host(&'static str),
    /// `add_project`: select the new project once the host has it.
    AddProject(String),
    /// A project's workspace record, read when the project becomes visible.
    ReadWorkspace(String),
    /// Then its terminal layout (`loadPersistedTerminals`), with the record read before it.
    ReadTerminals {
        project_id: String,
        record: Option<Value>,
    },
    WriteWorkspace(String),
}

/// A device read: not answered yet, or answered (`None` when the key is empty).
#[derive(Clone, Debug, Default, PartialEq)]
enum Read<T> {
    #[default]
    Unread,
    Read(T),
}

#[derive(Default)]
pub struct Sync {
    pending: HashMap<CallId, Pending>,
    next_call: CallId,
    saved_active: Read<Option<String>>,
    prefs: Read<()>,
    /// A registry list that came before the device's selection and prefs.
    held: Option<ProjectList>,
    /// The revision of each shared key this client last read or wrote.
    revisions: HashMap<Shared, u64>,
    /// `settings/app` as read, so a theme change rewrites only its own fields.
    pub(crate) app_settings: serde_json::Map<String, Value>,
    /// A project `add_project` created, selected when its registry row lands.
    select_when_listed: Option<String>,
    /// Every command sent, for the tests (a test state has no host).
    #[cfg(test)]
    pub(crate) sent: Vec<Command>,
    /// The project whose workspace record is being read.
    reading: Option<String>,
    /// The last record written per project, so an unchanged one is not written again.
    written: HashMap<String, Value>,
    /// The workspace revision the last persist saw.
    persisted_revision: u64,
    /// Projects whose stored record failed to read or decode: this session never writes over it.
    unwritable: HashSet<String>,
}

/// Fresh ids for groups and projects: UUID v4 text, as `randomUUID` gives.
pub struct Uuid;

impl Ids for Uuid {
    fn next_id(&mut self) -> String {
        crate::ids::uuid()
    }
}

impl State {
    /// Takes the link and asks for everything the shell shows.
    pub(crate) fn connect(&mut self, host: Host) {
        self.host = Some(host);
        self.begin();
    }

    /// The requests a fresh link sends: the projects and chat watches, then the device and
    /// shared reads (calls 1 to 5) and the chat list (call 6).
    pub(crate) fn begin(&mut self) {
        self.status = Status::Connecting;
        for watch in [WsWatchName::Projects, WsWatchName::ChatHistory] {
            self.send(Command::Watch {
                watch,
                scope: String::new(),
            });
        }
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
        self.call(
            WsRequestType::StoreRead,
            json!({ "key": self.device_key(PREFS_KEY) }),
            Pending::ReadPrefs,
        );
        for shared in [Shared::AppSettings, Shared::Shortcuts] {
            self.call(
                WsRequestType::StoreRead,
                json!({ "key": shared.host_key() }),
                Pending::ReadShared(shared),
            );
        }
        self.list_chats();
        self.layout_step(LayoutEvent::StartHydrate);
    }

    fn device_key(&self, key: &str) -> String {
        format!("device/{}/{key}", self.device_id)
    }

    fn send(&mut self, command: Command) {
        #[cfg(test)]
        self.sync.sent.push(command.clone());
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

    fn list_chats(&mut self) {
        self.call(
            WsRequestType::ListPersistedSessions,
            json!({}),
            Pending::ListChats,
        );
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
            Output::WatchDelta {
                watch: WsWatchName::ChatHistory,
                ..
            } => self.list_chats(),
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

    fn reply(&mut self, call: CallId, result: Result<Option<Value>, WsError>) {
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
                self.sync.saved_active = Read::Read(saved);
                self.release_held();
            }
            Pending::ReadPrefs => {
                let record = match result {
                    Ok(reply) => stored_value(reply).and_then(|v| {
                        match serde_json::from_value::<Record>(v) {
                            Ok(record) => Some(record),
                            Err(e) => {
                                log::warn!("[app] unreadable project prefs: {e}");
                                None
                            }
                        }
                    }),
                    Err(e) => {
                        log::warn!("[app] reading the project prefs failed: {}", e.code);
                        None
                    }
                };
                self.projects_step(ProjectEvent::LoadProjectPrefs(record));
                self.sync.prefs = Read::Read(());
                self.release_held();
            }
            Pending::ReadShared(shared) => match result {
                Ok(reply) => {
                    let revision = reply
                        .as_ref()
                        .and_then(|r| r.get("revision"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    self.sync.revisions.insert(shared, revision);
                    self.shared_loaded(shared, stored_value(reply));
                }
                Err(e) => log::warn!("[app] reading {} failed: {}", shared.host_key(), e.code),
            },
            Pending::WriteShared(shared) => match result {
                Ok(reply) => {
                    if let Some(revision) = reply
                        .as_ref()
                        .and_then(|r| r.get("revision"))
                        .and_then(Value::as_u64)
                    {
                        self.sync.revisions.insert(shared, revision);
                    }
                }
                Err(e) => {
                    log::warn!(
                        "[app] writing {} failed: {}: {}",
                        shared.host_key(),
                        e.code,
                        e.message
                    );
                    self.toast_error("Failed to save the setting", &e.message);
                }
            },
            Pending::ListChats => {
                match result.map(|r| r.map(serde_json::from_value::<Vec<Chat>>)) {
                    Ok(Some(Ok(list))) => self.chats.replace(list),
                    Ok(Some(Err(e))) => log::warn!("[app] unreadable chat list: {e}"),
                    Ok(None) => self.chats.replace(Vec::new()),
                    Err(e) => log::warn!("[app] listing chats failed: {}: {}", e.code, e.message),
                }
            }
            Pending::ReadWorkspace(project_id) => match result {
                Ok(reply) => {
                    let key = self.device_key(&format!("terminals/{project_id}"));
                    let record = stored_value(reply);
                    self.call(
                        WsRequestType::StoreRead,
                        json!({ "key": key }),
                        Pending::ReadTerminals { project_id, record },
                    );
                }
                Err(e) => {
                    log::warn!(
                        "[app] reading the workspace of {project_id} failed: {}; its stored record is left unwritten",
                        e.code
                    );
                    self.sync.unwritable.insert(project_id.clone());
                    self.workspace_read(&project_id, None, None);
                }
            },
            Pending::ReadTerminals { project_id, record } => {
                let terminals = match result {
                    Ok(reply) => stored_value(reply)
                        .and_then(|layout| layout.get("terminals").cloned())
                        .and_then(|list| serde_json::from_value(list).ok()),
                    Err(e) => {
                        log::warn!("[app] reading the terminal layout of {project_id} failed: {}", e.code);
                        None
                    }
                };
                self.workspace_read(&project_id, record, terminals);
            }
            Pending::Write(key) => {
                if let Err(e) = result {
                    log::warn!("[app] writing {key} failed: {}: {}", e.code, e.message);
                }
            }
            Pending::WritePrefs(signature) => {
                if let Err(e) = result {
                    log::warn!(
                        "[app] writing the project prefs failed: {}: {}",
                        e.code,
                        e.message
                    );
                    self.projects_step(ProjectEvent::PrefsWriteFailed(signature));
                }
            }
            Pending::Host(what) => {
                if let Err(e) = result {
                    log::warn!("[app] {what} failed: {}: {}", e.code, e.message);
                    self.toast_error(what, &e.message);
                }
            }
            Pending::AddProject(id) => match result {
                Ok(_) => {
                    self.sync.select_when_listed = Some(id.clone());
                    if self.projects.store.projects.iter().any(|p| p.id == id) {
                        self.select_project(&id);
                    }
                }
                Err(e) => {
                    log::warn!("[app] add_project failed: {}: {}", e.code, e.message);
                    self.toast_error("Failed to create the project", &e.message);
                }
            },
            Pending::WriteWorkspace(project_id) => {
                if let Err(e) = result {
                    log::warn!(
                        "[app] writing the workspace of {project_id} failed: {}: {}",
                        e.code,
                        e.message
                    );
                    // The next change writes again, even back to the value that failed.
                    self.sync.written.remove(&project_id);
                }
            }
        }
    }

    fn release_held(&mut self) {
        if let Some(list) = self.sync.held.take() {
            self.registry_list(list);
        }
    }

    fn registry_list(&mut self, list: ProjectList) {
        let ready =
            matches!(self.sync.saved_active, Read::Read(_)) && self.sync.prefs == Read::Read(());
        if !ready && !self.projects.store.is_loaded {
            self.sync.held = Some(list);
            return;
        }
        let saved = match &self.sync.saved_active {
            Read::Read(saved) => saved.clone(),
            Read::Unread => None,
        };
        self.projects_step(ProjectEvent::ApplyRegistryList(list, saved));
        if let Some(id) = self.sync.select_when_listed.take() {
            if self.projects.store.projects.iter().any(|p| p.id == id) {
                self.select_project(&id);
            } else {
                self.sync.select_when_listed = Some(id);
            }
        }
    }

    pub fn projects_step(&mut self, event: ProjectEvent) {
        let out = projects::step(&mut self.projects, event, &mut Uuid);
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

    fn workspace_read(
        &mut self,
        project_id: &str,
        raw: Option<Value>,
        terminals: Option<Vec<persistence::TerminalInfo>>,
    ) {
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
        // No terminal is live in the Rune shell yet, so the layout keeps the saved terminal tabs.
        let hydrated = persistence::hydrate(
            raw,
            root.as_deref(),
            &[],
            terminals.as_deref(),
            self.workspace.ids(),
            &mut effects,
        );
        self.run(effects);
        if hydrated.unreadable {
            self.sync.unwritable.insert(project_id.to_owned());
        }
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
        if self.sync.unwritable.contains(&project_id) {
            return;
        }
        let value = persistence::js_numbers(serde_json::to_value(record).unwrap_or(Value::Null));
        if self.sync.written.get(&project_id) == Some(&value) {
            return;
        }
        self.sync.written.insert(project_id.clone(), value.clone());
        let key = self.device_key(&persistence::key(&project_id));
        self.call(
            WsRequestType::StoreWrite,
            json!({ "key": key, "value": persistence::stored(value) }),
            Pending::WriteWorkspace(project_id),
        );
    }

    /// The projects whose workspace record has a write in flight.
    #[cfg(test)]
    pub(crate) fn workspace_writes(&self) -> Vec<&str> {
        self.sync
            .pending
            .values()
            .filter_map(|pending| match pending {
                Pending::WriteWorkspace(project_id) => Some(project_id.as_str()),
                _ => None,
            })
            .collect()
    }

    /// A group change: apply it, then save the sidebar layout.
    pub fn group_step(&mut self, event: ProjectEvent) {
        self.projects_step(event);
        self.projects_step(ProjectEvent::PersistProjectPrefs);
    }

    pub fn layout_step(&mut self, event: LayoutEvent) {
        let effects = shell_layout::step(&mut self.layout, event);
        self.run(effects);
    }

    fn write(&mut self, key: &str, value: Value, pending: Pending) {
        let key = self.device_key(key);
        self.call(
            WsRequestType::StoreWrite,
            json!({ "key": key, "value": persistence::stored(value) }),
            pending,
        );
    }

    /// Writes a shared key over the revision this client read; a key never read stays
    /// unwritten, so defaults cannot overwrite what another device saved.
    pub(crate) fn write_shared(&mut self, shared: Shared, data: Value) {
        let Some(&base) = self.sync.revisions.get(&shared) else {
            log::warn!("[app] not writing {}: it was never read", shared.host_key());
            return;
        };
        self.call(
            WsRequestType::StoreWrite,
            json!({ "key": shared.host_key(), "value": persistence::stored(data), "baseRevision": base }),
            Pending::WriteShared(shared),
        );
    }

    /// Sends a project registry command; a refusal raises a toast titled `what`.
    pub(crate) fn host_command(&mut self, kind: WsRequestType, payload: Value, what: &'static str) {
        self.call(kind, payload, Pending::Host(what));
    }

    /// `add_project`, then a switch to it once the registry lists it.
    pub(crate) fn add_project(&mut self, name: &str, path: &str, color: &str) {
        let id = crate::ids::uuid();
        self.call(
            WsRequestType::AddProject,
            json!({ "id": id, "name": name, "path": path, "color": color }),
            Pending::AddProject(id),
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
                    let key = self.device_key(shell_layout::KEY);
                    self.write(shell_layout::KEY, value, Pending::Write(key));
                }
                Effect::SaveActiveProject { project_id } => {
                    let key = self.device_key(ACTIVE_PROJECT_KEY);
                    self.write(
                        ACTIVE_PROJECT_KEY,
                        Value::from(project_id),
                        Pending::Write(key),
                    );
                }
                Effect::WritePrefs { record, signature } => {
                    let value = serde_json::to_value(record).unwrap_or(Value::Null);
                    self.write(PREFS_KEY, value, Pending::WritePrefs(signature));
                }
                Effect::UpdateProjectOnHost { project_id, patch } => {
                    let mut payload = serde_json::to_value(patch).unwrap_or_else(|_| json!({}));
                    payload["projectId"] = Value::from(project_id);
                    self.host_command(
                        WsRequestType::UpdateProject,
                        payload,
                        "Failed to update the project",
                    );
                }
                Effect::DeleteProject { project_id } => self.host_command(
                    WsRequestType::RemoveProject,
                    json!({ "projectId": project_id }),
                    "Failed to delete the project",
                ),
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
