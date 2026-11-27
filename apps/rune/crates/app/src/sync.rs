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
/// The code the link fails an unledgered call with when its socket drops.
const CLOSED: &str = "closed";

/// A key every device shares, written last-writer-wins on the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shared {
    /// `settings/app`: the colour theme and appearance, among fields other screens own.
    AppSettings,
    /// `settings/keyboard-shortcuts`.
    Shortcuts,
    /// `settings/pinned-commands`: the palette's pinned rows.
    PinnedCommands,
}

impl Shared {
    fn host_key(self) -> &'static str {
        match self {
            Shared::AppSettings => "shared/lww/settings/app",
            Shared::Shortcuts => "shared/lww/settings/keyboard-shortcuts",
            Shared::PinnedCommands => "shared/lww/settings/pinned-commands",
        }
    }
}

/// What a pending call's reply is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pending {
    ReadLayout,
    ReadActiveProject,
    ReadPrefs,
    ReadShared(Shared),
    /// This device's recent palette commands.
    ReadRecentCommands,
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
    /// A session payload the model's history read asked for (`get_session_payload`).
    Payload(Box<model::acp::Effect>),
    /// The agent configs (`acp/agents`).
    ReadAgents,
    /// The launcher's last agent (`agents/last-selected`).
    ReadLastAgent,
    /// A prompt the model dispatched; its reply settles the turn.
    Prompt { session_id: String, turn_id: String },
    /// The user's answer to a permission request, question or elicitation.
    Answer(Box<model::acp::answer::Answer>),
    /// A set command a model pick sent; its reply goes back to the model.
    Control(Box<model::acp::Effect>),
    /// The saved composer picks a model pick reads first.
    ComposerOptions(Box<model::acp::Effect>),
    /// The saved composer picks of a config, read before the first write merges into them.
    SavedPicks(String),
    /// A subagent's `stop_subagent`, by the record's id.
    StopSubagent(String),
    /// A browser consent answer, with the ticket to restore if the host refuses it.
    BrowserConsent(Box<Value>),
    /// An agent, session or switch command the model sent; its reply goes back to the model.
    Agent(Box<model::acp::Effect>),
    /// The launcher's `spawn_agent`.
    LaunchSpawn,
    /// Then its `create_session`, on that agent.
    LaunchSession(String),
    /// A project's saved commands, for the command history modal.
    ReadHistory(String),
    /// The acp model's MCP registry read.
    Mcp(Box<model::acp::Effect>),
    /// Its registry write, with what the Settings page does once it lands.
    McpSave(Box<model::acp::Effect>, Option<crate::settings_mcp::Action>),
    /// The empty list the history's Clear wrote for a project.
    ClearHistory(String),
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
    /// Shared keys with a write on the wire: one at a time per key, as the web client
    /// serializes them, so each write goes over the revision the one before it made.
    writing: HashSet<Shared>,
    /// The newest value written while its key had a write on the wire; it goes next.
    queued: HashMap<Shared, Value>,
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

impl Sync {
    /// The id the last call went out under.
    #[cfg(test)]
    pub(crate) fn last_call(&self) -> CallId {
        self.next_call
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
        self.composer.agents_asked = false;
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
        self.palette_link_reset();
    }

    /// A watched store key changed (`StoreWatchState`: `exists` and the stored `value`). While
    /// this client's own write to the key is out, its reply settles the value instead.
    fn store_watched(&mut self, scope: &str, state: &Value) {
        if scope == model::settings::mcp::KEY {
            return self.mcp_watched(state);
        }
        let shared = Shared::PinnedCommands;
        if scope != shared.host_key() || self.sync.writing.contains(&shared) {
            return;
        }
        if state.get("exists").and_then(Value::as_bool) != Some(true) {
            return;
        }
        if let Some(value) = state.get("value").cloned().map(persistence::unwrap_stored) {
            self.pinned_commands_changed(value);
        }
    }

    /// The palette's pinned and recent lists, read the first time it opens on this link.
    pub(crate) fn read_palette_lists(&mut self) {
        if self.palette.read {
            return;
        }
        self.palette.read = true;
        // `useSharedKeySync`: a pin another device saves shows without a reload.
        self.send(Command::Watch {
            watch: WsWatchName::Store,
            scope: Shared::PinnedCommands.host_key().to_owned(),
        });
        self.call(
            WsRequestType::StoreRead,
            json!({ "key": Shared::PinnedCommands.host_key() }),
            Pending::ReadShared(Shared::PinnedCommands),
        );
        self.call(
            WsRequestType::StoreRead,
            json!({ "key": self.device_key(model::settings::palette::RECENT_KEY) }),
            Pending::ReadRecentCommands,
        );
    }

    /// The composers' agent reads (`acp/agents`, `agents/last-selected`), once the first
    /// workspace shows; asked again after a reconnect.
    pub(crate) fn read_agents(&mut self) {
        let down = matches!(self.status, Status::Reconnecting { .. } | Status::Halted { .. } | Status::Stopped);
        if self.host.is_none() || down || self.composer.agents_asked || self.workspace.record().is_none() {
            return;
        }
        self.composer.agents_asked = true;
        self.call(WsRequestType::StoreRead, json!({ "key": "shared/lww/acp/agents" }), Pending::ReadAgents);
        let key = self.device_key("agents/last-selected");
        self.call(WsRequestType::StoreRead, json!({ "key": key }), Pending::ReadLastAgent);
    }

    /// A read the dropped socket failed (`closed`) is asked again once the link is live.
    fn agents_read_lost(&mut self, e: &WsError) {
        if e.code == CLOSED {
            self.composer.agents_asked = false;
        }
    }

    pub(crate) fn device_key(&self, key: &str) -> String {
        format!("device/{}/{key}", self.device_id)
    }

    pub(crate) fn send(&mut self, command: Command) {
        #[cfg(test)]
        self.sync.sent.push(command.clone());
        if let Some(host) = &mut self.host {
            host.send(command);
        }
    }

    pub(crate) fn call(&mut self, kind: WsRequestType, payload: Value, pending: Pending) {
        self.request(kind, payload, pending, false);
    }

    /// A mutating request the host ledgers: it carries a command id and survives a reconnect.
    pub(crate) fn command(&mut self, kind: WsRequestType, payload: Value, pending: Pending) {
        self.request(kind, payload, pending, true);
    }

    fn request(&mut self, kind: WsRequestType, payload: Value, pending: Pending, ledgered: bool) {
        self.sync.next_call += 1;
        let call = self.sync.next_call;
        self.sync.pending.insert(call, pending);
        self.send(Command::Call {
            call,
            kind,
            payload,
            ledgered,
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
        let fetched = host.fetched();
        let changed = !outputs.is_empty() || !fetched.is_empty();
        for output in outputs {
            self.apply(output);
        }
        for (tag, result) in fetched {
            match tag {
                crate::host::Fetch::Skills(root) => self.skills_listed(root, result),
                crate::host::Fetch::Listing { dir, root } => self.listing_read(dir, &root, result),
                tag @ (crate::host::Fetch::ExplorerList(_) | crate::host::Fetch::ExplorerOp(_)) => self.explorer_fetched(tag, result),
                crate::host::Fetch::Git(task) => self.git_replied(task, result),
                crate::host::Fetch::Github(task) => self.github_replied(task, result),
                crate::host::Fetch::History(tag) => self.history_replied(tag, result),
                crate::host::Fetch::Settings(tag) => self.idle_replied(tag, result),
                crate::host::Fetch::Shells => self.shells_listed(result),
                crate::host::Fetch::ProjectEnv { request, project, root } => self.env_read((request, project, root), result),
                crate::host::Fetch::Gitignore { project, why } => self.gitignore_parsed(&project, why, result),
                crate::host::Fetch::Mcp(effect) => self.mcp_fetched(*effect, result),
                crate::host::Fetch::McpPoll(url) => self.mcp_polled(&url, result),
                crate::host::Fetch::Catalog(tag) => self.catalog_fetched(tag, result),
                crate::host::Fetch::Automation(tag) => self.automation_replied(tag, result),
                tag => self.editor_fetched(tag, result),
            }
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
            Output::WatchState {
                watch: WsWatchName::Store,
                scope,
                state,
            }
            | Output::WatchDelta {
                watch: WsWatchName::Store,
                scope,
                delta: state,
            } => self.store_watched(&scope, &state),
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
            Output::Event(event) => self.chat_event(event),
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
            Pending::ReadRecentCommands => match result {
                Ok(reply) => self.recent_commands_loaded(stored_value(reply)),
                Err(e) => self.recent_commands_unread(&e.code),
            },
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
                Err(e) => {
                    log::warn!("[app] reading {} failed: {}", shared.host_key(), e.code);
                    match shared {
                        Shared::AppSettings => self.settings_loaded(Err(e.code.clone())),
                        Shared::PinnedCommands => self.pinned_commands_unread(&e.code),
                        Shared::Shortcuts => {}
                    }
                }
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
                    self.sync.writing.remove(&shared);
                    if let Some(next) = self.sync.queued.remove(&shared) {
                        self.write_shared(shared, next);
                    } else if let Some(merged) = merged_value(reply.as_ref()) {
                        // The host kept fields another device wrote since our read: show
                        // what it holds now, as the web client's `notifyShared` does.
                        self.shared_changed(shared, merged);
                    }
                }
                Err(e) => {
                    self.sync.writing.remove(&shared);
                    if let Some(next) = self.sync.queued.remove(&shared) {
                        self.write_shared(shared, next);
                    }
                    log::warn!(
                        "[app] writing {} failed: {}: {}",
                        shared.host_key(),
                        e.code,
                        e.message
                    );
                    self.toast_error("Failed to save the setting", &e.message);
                }
            },
            Pending::ReadAgents => match result {
                Ok(reply) => self.agents_loaded(stored_value(reply)),
                Err(e) => {
                    log::warn!("[app] reading the agent configs failed: {}", e.code);
                    self.agents_read_lost(&e);
                }
            },
            Pending::ReadLastAgent => match result {
                Ok(reply) => self.last_agent_loaded(stored_value(reply)),
                Err(e) => {
                    log::warn!("[app] reading the last agent failed: {}", e.code);
                    self.agents_read_lost(&e);
                    if e.code != CLOSED {
                        self.last_agent_loaded(None);
                    }
                }
            },
            Pending::Prompt { session_id, turn_id } => self.prompt_settled(session_id, turn_id, result),
            Pending::Answer(answer) => self.answer_settled(*answer, result),
            Pending::StopSubagent(id) => self.subagent_stop_settled(id, result),
            Pending::BrowserConsent(ticket) => self.browser_consent_settled(*ticket, result),
            Pending::Control(effect) => self.control_settled(*effect, result),
            Pending::ComposerOptions(effect) => self.composer_options_read(*effect, result),
            Pending::SavedPicks(config_id) => self.saved_picks_read(config_id, result),
            Pending::Agent(effect) => self.agent_command_settled(*effect, result),
            Pending::LaunchSpawn => self.launch_spawned(result),
            Pending::LaunchSession(agent_id) => self.launch_created(agent_id, result),
            Pending::ReadHistory(project) => self.history_read(project, result),
            Pending::ClearHistory(project) => self.history_cleared(project, result),
            Pending::Mcp(effect) => self.mcp_loaded(*effect, result),
            Pending::McpSave(effect, action) => self.mcp_saved(*effect, action, result),
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
            Pending::Payload(effect) => self.history_payload_read(*effect, result),
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
        self.project_settings_check_target();
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
        // The leaving project's drafts reach its record before it is saved and parked; the
        // debounce would let the buffers close first.
        self.flush_drafts();
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
        self.flush_drafts();
        self.persist_workspace();
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
            match request {
                ui::workspace::Request::NewTerminal { pane } => self.new_terminal(Some(pane)),
                ui::workspace::Request::SaveAndClose { path, .. } => self.save_and_close(path),
                other => log::info!("[app] workspace request not wired in the Rune shell yet: {other:?}"),
            }
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

    pub(crate) fn write(&mut self, key: &str, value: Value, pending: Pending) {
        let key = self.device_key(key);
        self.call(
            WsRequestType::StoreWrite,
            json!({ "key": key, "value": persistence::stored(value) }),
            pending,
        );
    }

    /// Writes a shared key over the revision this client read; a key never read stays
    /// unwritten, so defaults cannot overwrite what another device saved.
    /// While the key has a write on the wire the value waits, and a newer one replaces it.
    pub(crate) fn write_shared(&mut self, shared: Shared, data: Value) {
        let Some(&base) = self.sync.revisions.get(&shared) else {
            log::warn!("[app] not writing {}: it was never read", shared.host_key());
            return;
        };
        if self.sync.writing.contains(&shared) {
            self.sync.queued.insert(shared, data);
            return;
        }
        self.sync.writing.insert(shared);
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
/// The stored value of a shared write's reply when the host merged it with a newer one
/// (`merged`, sent by current hosts), else None.
fn merged_value(reply: Option<&Value>) -> Option<Value> {
    let reply = reply?;
    if reply.get("merged").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    reply.get("value").cloned().map(persistence::unwrap_stored).filter(|v| !v.is_null())
}

pub(crate) fn stored_value(reply: Option<Value>) -> Option<Value> {
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
