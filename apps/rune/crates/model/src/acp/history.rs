//! `openHistorySession` (`slices/session.ts`): reopen a chat from its
//! durable history. The transcript shows at once from the payload; then the
//! chat gets a live agent and its session back through `session/load` or
//! `session/resume`, or attaches to a turn the host still runs.
//!
//! Each wait is an effect: the payload read (`readSessionPayload`), the
//! agent (`ensureLiveAgent`), the reopen call (`loadSession`,
//! `resumeSession`) and the end of the replay window (`endReplay`). The
//! action answers `actionResolved` with null, or `actionRejected`.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::create::select_mcp_servers;
use super::effect::error_text;
use super::ensure::{Caller, EnsureOptions, Holder};
use super::history_install::{Installed, resume_misses_session};
use super::host_error::{HostError, SIGNED_OUT};
use super::import::RequestId;
use super::session::RESUME_FAILED_PREFIX;
use super::types::{Replaying, SessionStatus};
use super::window::PayloadRead;
use super::{Effect, State, js};

/// Names one history open.
pub type OpenId = u64;

const SOURCE: &str = "acp.openHistorySession";

/// `OpenHistoryOptions`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenHistoryOptions {
    /// Reload: never reuse the agent the chat ran on.
    #[serde(default)]
    pub force_new_agent: bool,
    /// The MCP servers to reopen with; `None` picks the registry's.
    #[serde(default)]
    pub mcp_servers: Option<Vec<Value>>,
}

/// The arguments of `openHistorySession`, then the app's request id.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OpenHistoryRequest(
    pub String,
    #[serde(default)] pub Option<OpenHistoryOptions>,
    #[serde(default)] pub RequestId,
);

/// `decideResume`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strategy {
    Local,
    Load,
    Resume,
}

/// Who waits on an open.
#[derive(Clone, Copy, Debug, PartialEq)]
enum OpenWaiter {
    /// The user's action, by the app's request id.
    Action(RequestId),
    /// A `createSession` the host answered from its records.
    Create(super::create::CreateId),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Open {
    session_id: String,
    generation: u64,
    options: OpenHistoryOptions,
    waiters: Vec<OpenWaiter>,
    /// The index held the chat when the open began, so its row going away
    /// meanwhile is a delete.
    was_indexed: bool,
    payload: Value,
    turn_live: bool,
    keep_error: bool,
    owner_connected: bool,
    live_agent: String,
    /// `hostReportSeq()` when the agent was looked for.
    report_seq: u64,
    capabilities: Value,
    /// The session's controls before the reopen call.
    baseline: Option<[Value; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HistoryBook {
    seq: OpenId,
    opens: BTreeMap<OpenId, Open>,
    /// The open in flight per session.
    by_session: BTreeMap<String, OpenId>,
    /// Per session, the reopen generation (`sessionReopenGenerations`): a
    /// newer open, a delete or a new session makes an older open's late
    /// answers moot.
    generations: BTreeMap<String, u64>,
    /// Sessions whose queued prompt goes out once the step ends.
    pub(crate) flush: Vec<String>,
    /// Creates whose looked-up session opened, answered once the step ends.
    pub(crate) created: Vec<(super::create::CreateId, String)>,
}

impl HistoryBook {
    /// `openingHistoryIds`: an open of the session is in flight.
    pub(crate) fn opening(&self, session_id: &str) -> bool {
        self.by_session.contains_key(session_id)
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|value| !value.is_null())
}

fn text(value: &Value, key: &str) -> Option<String> {
    field(value, key)
        .filter(|value| js::truthy(value))
        .map(|value| js::string_of(Some(value)))
}

/// `isReopenTurnActiveError`.
fn turn_active(error: &HostError) -> bool {
    error_text(error).contains("ACP_REOPEN_TURN_ACTIVE")
}

impl State {
    fn reopen_generation(&self, session_id: &str) -> u64 {
        self.book
            .history
            .generations
            .get(session_id)
            .copied()
            .unwrap_or(0)
    }

    /// `invalidateSessionReopen`: an open in flight stops at its next step.
    pub(crate) fn invalidate_session_reopen(&mut self, session_id: &str) {
        *self
            .book
            .history
            .generations
            .entry(session_id.to_owned())
            .or_default() += 1;
    }

    /// The open still owns its session: not superseded and not deleted.
    fn open_current(&self, open: &Open) -> bool {
        let indexed = || {
            self.session_index
                .iter()
                .any(|entry| entry.get("id").and_then(Value::as_str) == Some(&open.session_id))
        };
        self.reopen_generation(&open.session_id) == open.generation
            && (!open.was_indexed || indexed())
    }

    fn open_of(&self, id: OpenId) -> Option<&Open> {
        self.book.history.opens.get(&id)
    }

    /// `openHistorySession`.
    pub(crate) fn open_history_session(
        &mut self,
        request: OpenHistoryRequest,
        fx: &mut Vec<Effect>,
    ) {
        let OpenHistoryRequest(session_id, options, request) = request;
        // A live chat is already usable.
        if self.sessions.get(&session_id).is_some_and(|session| {
            !matches!(session.status, SessionStatus::Closed | SessionStatus::Error)
        }) {
            return fx.push(Effect::ActionResolved {
                request,
                value: Value::Null,
            });
        }
        self.open_history_for(session_id, options, OpenWaiter::Action(request), fx);
    }

    /// `createSession` was answered from the host's records: open the
    /// session it names from history, then answer the create.
    pub(crate) fn open_looked_up_session(
        &mut self,
        session_id: String,
        create: super::create::CreateId,
        fx: &mut Vec<Effect>,
    ) {
        if self.sessions.get(&session_id).is_some_and(|session| {
            !matches!(session.status, SessionStatus::Closed | SessionStatus::Error)
        }) {
            return self.book.history.created.push((create, session_id));
        }
        self.open_history_for(session_id, None, OpenWaiter::Create(create), fx);
    }

    fn open_history_for(
        &mut self,
        session_id: String,
        options: Option<OpenHistoryOptions>,
        waiter: OpenWaiter,
        fx: &mut Vec<Effect>,
    ) {
        let current = self.reopen_generation(&session_id);
        let book = &mut self.book.history;
        if let Some(open) = book
            .by_session
            .get(&session_id)
            .and_then(|id| book.opens.get_mut(id))
            .filter(|open| open.generation == current)
        {
            return open.waiters.push(waiter);
        }
        self.invalidate_session_reopen(&session_id);
        let was_indexed = self
            .session_index
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(&session_id));
        let book = &mut self.book.history;
        book.seq += 1;
        let id = book.seq;
        book.by_session.insert(session_id.clone(), id);
        book.opens.insert(
            id,
            Open {
                session_id: session_id.clone(),
                generation: current + 1,
                options: options.unwrap_or_default(),
                waiters: vec![waiter],
                was_indexed,
                payload: Value::Null,
                turn_live: false,
                keep_error: false,
                owner_connected: false,
                live_agent: String::new(),
                report_seq: 0,
                capabilities: Value::Null,
                baseline: None,
            },
        );
        fx.push(Effect::ReadSessionPayload {
            session_id,
            read: PayloadRead::History(id),
        });
    }

    /// The open is over: its callers get the outcome, and a prompt queued
    /// meanwhile goes out.
    fn finish_open(&mut self, id: OpenId, error: Option<HostError>, fx: &mut Vec<Effect>) {
        let book = &mut self.book.history;
        let Some(open) = book.opens.remove(&id) else {
            return;
        };
        if book.by_session.get(&open.session_id) == Some(&id) {
            book.by_session.remove(&open.session_id);
            book.flush.push(open.session_id.clone());
        }
        for waiter in open.waiters {
            match (waiter, &error) {
                (OpenWaiter::Action(request), None) => fx.push(Effect::ActionResolved {
                    request,
                    value: Value::Null,
                }),
                (OpenWaiter::Action(_), Some(error)) => fx.push(Effect::rejected_by(error)),
                (OpenWaiter::Create(create), None) => {
                    let opened = (create, open.session_id.clone());
                    self.book.history.created.push(opened);
                }
                (OpenWaiter::Create(create), Some(error)) => {
                    self.on_create_failed(create, error.clone(), fx);
                }
            }
        }
    }

    /// `withSessionResumeError`, then the open fails with the same error.
    fn fail_open(&mut self, id: OpenId, error: &HostError, fx: &mut Vec<Effect>) {
        let Some(session_id) = self.open_of(id).map(|open| open.session_id.clone()) else {
            return;
        };
        let text = error_text(error);
        if let Some(session) = self.sessions.get_mut(&session_id) {
            let shown = if error.is(&SIGNED_OUT) {
                SIGNED_OUT
                    .iter()
                    .find_map(|(_, tag)| error.message.strip_prefix(&format!("{tag}: ")))
                    .unwrap_or(&error.message)
                    .to_owned()
            } else {
                text.clone()
            };
            session.replaying = Some(None);
            session.last_error = Some(Some(format!("{RESUME_FAILED_PREFIX}{shown}")));
            session.last_error_code = Some(None);
            session.last_error_data = None;
        }
        self.finish_open(id, Some(error.clone()), fx);
    }

    /// The payload read is over: show the transcript, then look for the
    /// chat's agent.
    pub(crate) fn on_history_payload(
        &mut self,
        id: OpenId,
        outcome: Result<Option<Value>, String>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(open) = self.open_of(id) else {
            return;
        };
        let session_id = open.session_id.clone();
        if self.reopen_generation(&session_id) != open.generation {
            return self.finish_open(id, None, fx);
        }
        let payload = match outcome {
            Err(message) => return self.finish_open(id, Some(HostError::new(message, None)), fx),
            Ok(None) => {
                let message = format!("no persisted history for {session_id}");
                return self.finish_open(id, Some(HostError::new(message, None)), fx);
            }
            Ok(Some(payload)) => payload,
        };
        let Some(installed) = self.install_history(&session_id, &payload, fx) else {
            let message = format!("unreadable history for {session_id}");
            return self.finish_open(id, Some(HostError::new(message, None)), fx);
        };
        let Installed {
            turn_live,
            keep_error,
            ..
        } = installed;
        if let Some(open) = self.book.history.opens.get_mut(&id) {
            open.payload = payload;
            open.turn_live = turn_live;
            open.keep_error = keep_error;
        }
        self.find_history_agent(id, fx);
    }

    /// Resolve the agent the chat reopens on: the persisted one when it
    /// still runs the turn, else whatever `ensureLiveAgent` gives.
    fn find_history_agent(&mut self, id: OpenId, fx: &mut Vec<Effect>) {
        let Some(open) = self.open_of(id).cloned() else {
            return;
        };
        if !self.open_current(&open) {
            return self.finish_open(id, None, fx);
        }
        let meta = open.payload.get("metadata").cloned().unwrap_or(Value::Null);
        let agent_id = js::string_of(meta.get("agentId"));
        let force = open.options.force_new_agent;
        let connected =
            |state: &Self, agent: &str| state.agent_status.get(agent) == Some(&json!("connected"));
        // The host's owner listing is not ported, so it never vouches; the
        // persisted agent, still connected, may still run the turn.
        let owner_connected = !force && open.turn_live && connected(self, &agent_id);
        if open.turn_live && !owner_connected {
            fx.push(Effect::warn(
                SOURCE,
                format!(
                    "Session {} persisted a live turn but no owning agent is connected; clearing the dead turn and reopening normally",
                    open.session_id
                ),
            ));
            self.clear_dead_turn(&open.session_id);
        }
        if force
            && self
                .sessions
                .get(&open.session_id)
                .is_some_and(|session| session.active_turn || session.open_turn_id.is_some())
        {
            self.clear_dead_turn(&open.session_id);
        }
        let report_seq = self.book.report_seq;
        if let Some(open) = self.book.history.opens.get_mut(&id) {
            open.owner_connected = owner_connected;
            open.live_agent = agent_id;
            open.report_seq = report_seq;
        }
        let config_id = text(&meta, "agentConfigId");
        let cwd = text(&meta, "cwd");
        match (owner_connected, config_id, cwd) {
            (false, Some(config_id), Some(cwd)) => {
                let options = EnsureOptions {
                    silent: true,
                    surface_definite: true,
                    project_id: text(&meta, "projectId"),
                    holder: Some(Holder::Chat),
                    ..EnsureOptions::default()
                };
                self.ensure_live_agent(&config_id, &cwd, options, Caller::HistoryOpen(id), fx);
            }
            _ => self.reopen_history(id, fx),
        }
    }

    /// `ensureLiveAgent` answered the open.
    pub(crate) fn on_history_agent(
        &mut self,
        id: OpenId,
        outcome: Result<Option<String>, HostError>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(open) = self.open_of(id) else {
            return;
        };
        if !self.open_current(open) {
            return self.finish_open(id, None, fx);
        }
        match outcome {
            Err(error) => self.fail_open(id, &error, fx),
            Ok(Some(agent_id)) => {
                if let Some(open) = self.book.history.opens.get_mut(&id) {
                    open.live_agent = agent_id;
                }
                self.reopen_history(id, fx);
            }
            Ok(None) if open.options.force_new_agent => {
                let error =
                    HostError::new("Could not start a new agent for this chat.".into(), None);
                self.fail_open(id, &error, fx);
            }
            Ok(None) => self.reopen_history(id, fx),
        }
    }

    /// Attach to the running turn, or bring the session back on the agent.
    fn reopen_history(&mut self, id: OpenId, fx: &mut Vec<Effect>) {
        let Some(open) = self.open_of(id).cloned() else {
            return;
        };
        if !self.open_current(&open) {
            return self.finish_open(id, None, fx);
        }
        let session_id = open.session_id.clone();
        let live = open.live_agent.clone();
        let connected = self.agent_status.get(&live) == Some(&json!("connected"));
        if !open.options.force_new_agent && open.turn_live && open.owner_connected {
            if connected {
                return self.attach_live_turn(id, fx);
            }
            fx.push(Effect::warn(
                SOURCE,
                format!(
                    "Session {session_id} turn owner {live} disconnected during reopen; clearing the dead turn and reopening normally"
                ),
            ));
            self.clear_dead_turn(&session_id);
        }
        let capabilities = self
            .agents
            .get(&live)
            .and_then(|agent| field(agent, "capabilities"))
            .cloned()
            .unwrap_or(Value::Null);
        let strategy = match (&capabilities, connected) {
            (Value::Null, _) | (_, false) => Strategy::Local,
            (caps, true)
                if caps
                    .pointer("/sessionCapabilities/resume")
                    .is_some_and(|r| !r.is_null()) =>
            {
                Strategy::Resume
            }
            (caps, true) if caps.get("loadSession") == Some(&Value::Bool(true)) => Strategy::Load,
            _ => Strategy::Local,
        };
        let mcp_servers = match &open.options.mcp_servers {
            Some(servers) => servers.clone(),
            None => {
                let caps = Some(&capabilities).filter(|caps| !caps.is_null());
                let servers = select_mcp_servers(&self.mcp_servers, caps).0;
                if strategy != Strategy::Local {
                    fx.push(Effect::info(
                        "acp.openHistorySession.mcpAttach",
                        format!(
                            "Reopen of session {session_id} attaches {} mcp server(s)",
                            servers.len()
                        ),
                    ));
                }
                servers
            }
        };
        let reports = self
            .book
            .host_reports
            .get(&(session_id.clone(), live.clone()));
        let late_working = reports
            .and_then(|reports| reports.working)
            .filter(|(_, seq)| *seq > open.report_seq)
            .map(|(working, _)| working);
        let late_commands = reports
            .and_then(|reports| reports.background_commands.clone())
            .filter(|(_, seq)| *seq > open.report_seq)
            .map(|(mut list, _)| {
                list["agentId"] = json!(live);
                list
            });
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return self.finish_open(id, None, fx);
        };
        session.agent_id = live.clone();
        if late_working.is_some() {
            session.agent_working = late_working;
        }
        session.background_commands = late_commands;
        session.replaying = Some(match strategy {
            Strategy::Load => Some(Replaying::Pending),
            Strategy::Resume => Some(Replaying::Streaming),
            Strategy::Local => None,
        });
        let control = |key: &str| session.rest.get(key).cloned().unwrap_or(Value::Null);
        let baseline = [
            control("modes"),
            control("models"),
            control("configOptions"),
        ];
        let meta = open.payload.get("metadata").cloned().unwrap_or(Value::Null);
        let cwd = js::string_of(meta.get("cwd"));
        let project_id = field(&meta, "projectId").and_then(Value::as_str);
        let additional_directories = self.additional_workspace_roots(&live, &cwd, project_id);
        if let Some(open) = self.book.history.opens.get_mut(&id) {
            open.capabilities = capabilities;
            open.baseline = Some(baseline);
        }
        let call = |load: bool| {
            let (agent_id, session_id, open) = (live.clone(), session_id.clone(), id);
            let (cwd, additional_directories, mcp_servers) = (
                cwd.clone(),
                additional_directories.clone(),
                mcp_servers.clone(),
            );
            if load {
                Effect::LoadSession {
                    agent_id,
                    session_id,
                    cwd,
                    additional_directories,
                    mcp_servers,
                    open,
                    retried: false,
                }
            } else {
                Effect::ResumeSession {
                    agent_id,
                    session_id,
                    cwd,
                    additional_directories,
                    mcp_servers,
                    open,
                    retried: false,
                }
            }
        };
        match strategy {
            Strategy::Local => self.finish_open(id, None, fx),
            Strategy::Load => fx.push(call(true)),
            Strategy::Resume => fx.push(call(false)),
        }
    }

    /// `attachLiveTurn`: the host still runs the turn, so the chat takes its
    /// events as they come instead of reopening.
    fn attach_live_turn(&mut self, id: OpenId, fx: &mut Vec<Effect>) {
        let Some(open) = self.open_of(id).cloned() else {
            return;
        };
        let session_id = open.session_id.clone();
        let payload = &open.payload;
        let messages: Vec<super::types::ChatMessage> = payload
            .get("messages")
            .cloned()
            .and_then(|messages| serde_json::from_value(messages).ok())
            .unwrap_or_default();
        let max_seq = super::history_install::max_payload_seq(payload);
        let last_seq = payload
            .pointer("/metadata/lastSeq")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let watermark = last_seq.max(max_seq);
        let prompt_seq = super::history_install::open_turn_prompt_seq(&messages);
        let cursor = prompt_seq.map_or(watermark, |seq| seq - 1.0);
        if prompt_seq.is_some() {
            self.book
                .live_turn_replay_floors
                .insert(session_id.clone(), cursor);
        }
        let Some(session) = self.sessions.get_mut(&session_id) else {
            let message = format!("no session record to attach for {session_id}");
            return self.finish_open(id, Some(HostError::new(message, None)), fx);
        };
        session.agent_id = open.live_agent.clone();
        session.status = SessionStatus::Active;
        session.last_error = Some(None);
        session.last_error_code = Some(None);
        session.last_error_data = None;
        session.replaying = Some(Some(Replaying::Streaming));
        session.active_turn = true;
        if session.open_turn_id.is_none() {
            session.open_turn_id = Some("turn:live".into());
        }
        fx.push(Effect::info(
            "acp.attachLiveTurn",
            format!(
                "Attaching to live turn for session {session_id} via subscribe/replay instead of resume/load (cursor {cursor}, history watermark {watermark})"
            ),
        ));
        self.end_reopen(id, false, fx);
    }

    /// `withSessionActive`, then the replay window ends after the events
    /// already queued.
    fn end_reopen(&mut self, id: OpenId, keep_error: bool, fx: &mut Vec<Effect>) {
        let Some(open) = self.open_of(id) else {
            return;
        };
        let (session_id, generation) = (open.session_id.clone(), open.generation);
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.status = SessionStatus::Active;
            if !keep_error {
                session.last_error = Some(None);
                session.last_error_code = Some(None);
                session.last_error_data = None;
            }
        }
        fx.push(Effect::EndReplay {
            session_id,
            generation,
        });
        self.finish_open(id, None, fx);
    }

    /// `session/load` or `session/resume` answered.
    pub(crate) fn on_reopen_done(&mut self, command: &Effect, reply: &Value, fx: &mut Vec<Effect>) {
        let (Effect::LoadSession { open: id, .. } | Effect::ResumeSession { open: id, .. }) =
            command
        else {
            return;
        };
        let Some(open) = self.open_of(*id).cloned() else {
            return;
        };
        if !self.open_current(&open) {
            self.clear_replay_if_current(&open);
            return self.finish_open(*id, None, fx);
        }
        let session_id = open.session_id.clone();
        if let (Some(baseline), Some(session)) =
            (&open.baseline, self.sessions.get_mut(&session_id))
        {
            // `mergeReopenOutcomeIfUnchanged`: a control an event changed
            // meanwhile keeps its newer value.
            for (key, held) in ["modes", "models", "configOptions"].iter().zip(baseline) {
                let unchanged = session.rest.get(*key).unwrap_or(&Value::Null) == held;
                if let Some(value) = reply.get(*key)
                    && unchanged
                {
                    session.rest.insert((*key).into(), value.clone());
                }
            }
        }
        if matches!(command, Effect::LoadSession { .. })
            && let Some(session) = self.sessions.get_mut(&session_id)
            && session.replaying() == Some(Replaying::Pending)
        {
            // The agent replayed nothing: close the window now, so a later
            // live chunk cannot replace the transcript.
            session.replaying = Some(None);
            super::turn::finalize_streaming(self.messages.get_mut(&session_id));
        }
        self.end_reopen(*id, open.keep_error, fx);
    }

    /// `session/load` or `session/resume` failed for good.
    pub(crate) fn on_reopen_failed(
        &mut self,
        command: Effect,
        error: HostError,
        fx: &mut Vec<Effect>,
    ) {
        let (Effect::LoadSession { open: id, .. } | Effect::ResumeSession { open: id, .. }) =
            &command
        else {
            return;
        };
        let id = *id;
        let Some(open) = self.open_of(id).cloned() else {
            return;
        };
        if !self.open_current(&open) {
            if matches!(command, Effect::LoadSession { .. }) {
                self.clear_replay_if_current(&open);
            }
            return self.finish_open(id, None, fx);
        }
        if turn_active(&error) {
            return self.attach_live_turn(id, fx);
        }
        let session_id = open.session_id.clone();
        if let Effect::ResumeSession {
            agent_id,
            session_id: _,
            cwd,
            additional_directories,
            mcp_servers,
            ..
        } = command
        {
            let loads = open.capabilities.get("loadSession") == Some(&Value::Bool(true));
            if !(loads && resume_misses_session(&error)) {
                return self.fail_open(id, &error, fx);
            }
            fx.push(Effect::warn(
                "acp.openHistorySession.resumeFallback",
                format!(
                    "session/resume missed session {session_id}; falling back to session/load once"
                ),
            ));
            if let Some(session) = self.sessions.get_mut(&session_id) {
                session.replaying = Some(Some(Replaying::Pending));
            }
            return fx.push(Effect::LoadSession {
                agent_id,
                session_id,
                cwd,
                additional_directories,
                mcp_servers,
                open: id,
                retried: false,
            });
        }
        // A partial replay may have replaced the transcript: put the
        // payload's back.
        self.restore_history_transcript(&session_id, &open.payload, fx);
        self.fail_open(id, &error, fx);
    }

    fn clear_replay_if_current(&mut self, open: &Open) {
        if self.reopen_generation(&open.session_id) != open.generation {
            return;
        }
        if let Some(session) = self.sessions.get_mut(&open.session_id)
            && session.replaying().is_some()
        {
            session.replaying = Some(None);
        }
    }

    /// `scheduleReplayEnd`'s timer ran: the replay window closes, and a title
    /// the replay brought reaches the index.
    pub(crate) fn on_replay_end(&mut self, end: &Effect, fx: &mut Vec<Effect>) {
        let Effect::EndReplay {
            session_id,
            generation,
        } = end
        else {
            return;
        };
        if self.reopen_generation(session_id) != *generation {
            return;
        }
        let Some(session) = self
            .sessions
            .get_mut(session_id)
            .filter(|session| session.replaying().is_some())
        else {
            return;
        };
        session.replaying = Some(None);
        let titled = session
            .title
            .as_deref()
            .is_some_and(|title| !title.is_empty());
        super::turn::finalize_streaming(self.messages.get_mut(session_id));
        let indexed = self
            .session_index
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id));
        if titled && indexed {
            fx.push(Effect::PersistSession {
                session_id: session_id.clone(),
            });
        }
    }
}
