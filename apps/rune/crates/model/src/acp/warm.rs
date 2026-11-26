//! The warm pool (`slices/launch.ts`): `prepareChat` opens a session ahead
//! of the user's first prompt, `claimPreparedChat` hands it to a new chat
//! and `cancelPreparedChat` tears it down. A prepared session is ephemeral
//! (never persisted) until a chat claims it; one that outlived its launch
//! (an edited env, another account, a changed MCP list) is dropped and
//! disposed (`reapOrphanPreparedSession`).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::create::{CreateCaller, CreateOptions, CreateRequest};
use super::ensure::{Caller, EnsureOptions, Holder};
use super::host_error::HostError;
use super::import::RequestId;
use super::setup_error::classify_setup_error;
use super::types::SessionStatus;
use super::{Effect, Env, Killer, State, js};

/// Names one `prepareChat` task.
pub type PrepareId = u64;

const PREPARE_SOURCE: &str = "acp-store.prepareChat";
const REAP_SOURCE: &str = "acp.reapOrphanPreparedSession";

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareOptions {
    /// A background seed: no toast, and a definite rejection is not retried.
    #[serde(default)]
    pub silent: bool,
}

/// The arguments of `prepareChat`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PrepareRequest(
    pub String,
    pub String,
    #[serde(default)] pub Option<Vec<Value>>,
    #[serde(default)] pub Option<String>,
    #[serde(default)] pub Option<PrepareOptions>,
);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prepare {
    key: String,
    request: PrepareRequest,
    cwd: String,
    held_agent: Option<String>,
    agent_id: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WarmBook {
    seq: PrepareId,
    tasks: BTreeMap<PrepareId, Prepare>,
    /// The prepare that owns each key (`inFlightPrepared`).
    in_flight: BTreeMap<String, PrepareId>,
}

/// `mcpServersFingerprint`: the list in any order.
pub(crate) fn mcp_fingerprint(servers: &[Value]) -> String {
    let mut parts: Vec<String> = servers
        .iter()
        .map(|server| serde_json::to_string(server).unwrap_or_default())
        .collect();
    parts.sort();
    parts.join("|")
}

/// `prepareChatKey`.
pub(crate) fn prepare_chat_key(config_id: &str, cwd: &str, servers: Option<&[Value]>) -> String {
    format!(
        "{config_id}\0{}\0{}",
        crate::js_trim(cwd),
        mcp_fingerprint(servers.unwrap_or_default())
    )
}

/// `agentPolicyForConfigId(..).auth.isolateNewSessions`: a new session
/// resets the model of every session in the agent's process.
fn isolates_new_sessions(config_id: &str) -> bool {
    config_id.strip_prefix("acp-registry:").unwrap_or(config_id) == "factory-droid"
}

impl State {
    /// `retargetWarmPool`: the warm sessions for this folder of other
    /// configs go, and the config's own pool is seeded.
    pub(crate) fn retarget_warm_pool(
        &mut self,
        config_id: &str,
        cwd: &str,
        project_id: String,
        fx: &mut Vec<Effect>,
    ) {
        let cwd = crate::js_trim(cwd);
        if config_id.is_empty() || cwd.is_empty() {
            return;
        }
        let target = super::discovery::normalize_cwd(cwd);
        let mut keys: Vec<String> = self.prepared_sessions.keys().cloned().collect();
        for key in self.preparing_chat_keys.keys() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        for key in keys {
            let parsed = super::reuse_key::parse_reuse_key(&key);
            if parsed.config_id != config_id
                && super::discovery::normalize_cwd(parsed.cwd) == target
            {
                self.cancel_prepared_chat(&key, fx);
            }
        }
        let request = PrepareRequest(
            config_id.to_owned(),
            cwd.to_owned(),
            None,
            Some(project_id),
            Some(PrepareOptions { silent: true }),
        );
        self.prepare_chat(request, fx);
    }

    /// `prepareChat`.
    pub(crate) fn prepare_chat(&mut self, request: PrepareRequest, fx: &mut Vec<Effect>) {
        let PrepareRequest(config_id, cwd, servers, project_id, options) = &request;
        let cwd = crate::js_trim(cwd).to_owned();
        if config_id.is_empty() || cwd.is_empty() {
            return;
        }
        let key = prepare_chat_key(config_id, &cwd, servers.as_deref());
        if self.reconcile_prepared_slot(&key, fx).is_some()
            || self.book.warm.in_flight.contains_key(&key)
        {
            return;
        }
        let silent = options.as_ref().is_some_and(|options| options.silent);
        let definite = self
            .prepare_chat_errors
            .get(&key)
            .and_then(|error| error.get("definite"))
            .is_some_and(js::truthy);
        if silent && definite {
            return;
        }
        self.prepare_chat_errors.remove(&key);
        self.preparing_chat_keys.insert(key.clone(), json!(true));
        let book = &mut self.book.warm;
        book.seq += 1;
        let id = book.seq;
        book.in_flight.insert(key.clone(), id);
        let options = EnsureOptions {
            project_id: project_id.clone(),
            holder: Some(Holder::Warm),
            ..EnsureOptions::default()
        };
        let config_id = config_id.clone();
        book.tasks.insert(
            id,
            Prepare {
                key,
                request,
                cwd: cwd.clone(),
                held_agent: None,
                agent_id: String::new(),
            },
        );
        self.ensure_live_agent(&config_id, &cwd, options, Caller::Prepare(id), fx);
    }

    fn prepare_current(&self, id: PrepareId) -> bool {
        self.book
            .warm
            .tasks
            .get(&id)
            .is_some_and(|task| self.book.warm.in_flight.get(&task.key) == Some(&id))
    }

    /// The prepare's agent is there, or not.
    pub(crate) fn on_prepare_agent(
        &mut self,
        id: PrepareId,
        outcome: Result<Option<String>, HostError>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(task) = self.book.warm.tasks.get_mut(&id) else {
            return;
        };
        if let Ok(Some(agent_id)) = &outcome {
            task.held_agent = Some(agent_id.clone());
        }
        let current = self.prepare_current(id);
        let agent_id = match outcome {
            Err(error) => {
                self.prepare_failed(id, &error, fx);
                return self.finish_prepare(id, false, fx);
            }
            Ok(_) if !current => return self.finish_prepare(id, false, fx),
            Ok(None) => {
                let config_id = self.book.warm.tasks[&id].request.0.clone();
                let error = HostError::new(
                    format!("failed to spawn agent for config {config_id}"),
                    None,
                );
                self.prepare_failed(id, &error, fx);
                return self.finish_prepare(id, false, fx);
            }
            Ok(Some(agent_id)) => agent_id,
        };
        let task = self.book.warm.tasks.get_mut(&id).expect("found above");
        task.agent_id = agent_id.clone();
        let PrepareRequest(_, _, servers, project_id, _) = task.request.clone();
        let options = CreateOptions {
            ephemeral: true,
            backend_ephemeral: true,
            promotable: true,
            ..CreateOptions::default()
        };
        let request = CreateRequest(
            agent_id,
            task.cwd.clone(),
            servers,
            project_id,
            Some(options),
            0,
        );
        self.create_session_for(request, CreateCaller::Prepare(id), fx);
    }

    /// The prepare's `session/new` answered.
    pub(crate) fn on_prepare_session(
        &mut self,
        id: PrepareId,
        session_id: String,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(task) = self.book.warm.tasks.get(&id).cloned() else {
            return;
        };
        let agent_id = task.agent_id.clone();
        if self.agent_status.get(&agent_id) != Some(&json!("connected")) {
            // The agent died meanwhile: the session is dropped, never kept.
            self.book.registry_mcp.remove(&session_id);
            if self.book.ephemeral_sessions.remove(&session_id) {
                self.sessions.remove(&session_id);
            }
            return self.finish_prepare(id, false, fx);
        }
        if !self.prepare_current(id) {
            self.reap_orphan_prepared_session(&session_id, None, fx);
            return self.finish_prepare(id, false, fx);
        }
        let PrepareRequest(config_id, _, _, project_id, _) = &task.request;
        if !self.runs_current_launch(&agent_id, config_id, &task.cwd, project_id.as_deref()) {
            fx.push(Effect::info(
                PREPARE_SOURCE,
                format!("Warm session {session_id} on agent {agent_id} started with an outdated launch; preparing it again"),
            ));
            self.reap_orphan_prepared_session(&session_id, Some(agent_id), fx);
            return self.finish_prepare(id, true, fx);
        }
        self.prepared_sessions
            .insert(task.key.clone(), session_id.clone());
        self.cache_options_from_session(&session_id, env, fx);
        self.finish_prepare(id, false, fx);
    }

    /// The prepare's `session/new` failed.
    pub(crate) fn on_prepare_session_failed(
        &mut self,
        id: PrepareId,
        error: &HostError,
        fx: &mut Vec<Effect>,
    ) {
        self.prepare_failed(id, error, fx);
        self.finish_prepare(id, false, fx);
    }

    /// The launcher shows the classified failure; a spawn failure the user
    /// asked for also toasts.
    fn prepare_failed(&mut self, id: PrepareId, error: &HostError, fx: &mut Vec<Effect>) {
        if !self.prepare_current(id) {
            return;
        }
        let task = &self.book.warm.tasks[&id];
        let PrepareRequest(config_id, _, _, _, options) = &task.request;
        let command = self
            .agent_configs
            .iter()
            .find(|config| config.get("id").and_then(Value::as_str) == Some(config_id))
            .and_then(|config| config.get("command"))
            .and_then(Value::as_str);
        let classified = classify_setup_error(error, command);
        let silent = options.as_ref().is_some_and(|options| options.silent);
        if classified["category"] == "spawn" && !silent {
            fx.push(Effect::Toast {
                level: "error".into(),
                message: js::string_of(classified.get("detail")),
            });
        }
        self.prepare_chat_errors
            .insert(task.key.clone(), classified);
    }

    /// The prepare's `finally`: the warm hold ends and, for the task that
    /// still owns its key, the key stops preparing. An outdated launch
    /// prepares again.
    fn finish_prepare(&mut self, id: PrepareId, outdated: bool, fx: &mut Vec<Effect>) {
        let current = self.prepare_current(id);
        let Some(task) = self.book.warm.tasks.remove(&id) else {
            return;
        };
        if let Some(agent_id) = &task.held_agent {
            self.release_warm_agent(agent_id);
        }
        if !current {
            return;
        }
        self.book.warm.in_flight.remove(&task.key);
        self.preparing_chat_keys.remove(&task.key);
        if outdated {
            self.prepare_chat(task.request, fx);
        }
    }

    /// `cancelPreparedChat`.
    pub(crate) fn cancel_prepared_chat(&mut self, key: &str, fx: &mut Vec<Effect>) {
        let session_id = self.prepared_sessions.get(key).cloned();
        self.cancel_prepared_entry(key);
        let Some(session_id) = session_id else {
            return;
        };
        if self.active_session_id.as_deref() == Some(&session_id) {
            return;
        }
        self.reap_orphan_prepared_session(&session_id, None, fx);
    }

    /// `cancelPreparedChatEntry`: the key forgets its task and its slot.
    fn cancel_prepared_entry(&mut self, key: &str) {
        self.book.warm.in_flight.remove(key);
        self.prepared_sessions.remove(key);
        self.preparing_chat_keys.remove(key);
        self.prepare_chat_errors.remove(key);
    }

    /// `claimPreparedChat`: the slot's session becomes the project's chat.
    pub(crate) fn claim_prepared_chat(
        &mut self,
        key: &str,
        project_id: &str,
        request: RequestId,
        fx: &mut Vec<Effect>,
    ) {
        let claimed = self.reconcile_prepared_slot(key, fx).filter(|session_id| {
            self.sessions
                .get(session_id)
                .and_then(|session| session.rest.get("projectId"))
                == Some(&json!(project_id))
        });
        if let Some(session_id) = &claimed {
            self.promote_prepared_session(key, session_id, project_id, fx);
        }
        fx.push(Effect::ActionResolved {
            request,
            value: claimed.map_or(Value::Null, Value::String),
        });
    }

    /// `promotePreparedSession`: the session becomes durable, the chat is
    /// saved, and the pool seeds its next warm session.
    fn promote_prepared_session(
        &mut self,
        key: &str,
        session_id: &str,
        project_id: &str,
        fx: &mut Vec<Effect>,
    ) {
        self.book.ephemeral_sessions.remove(session_id);
        if let Some(session) = self.sessions.get_mut(session_id) {
            if !session.agent_id.is_empty() {
                fx.push(Effect::PromoteSession {
                    agent_id: session.agent_id.clone(),
                    session_id: session_id.to_owned(),
                });
            }
            session.rest.insert("projectId".into(), json!(project_id));
        }
        fx.push(Effect::PersistSession {
            session_id: session_id.to_owned(),
        });
        self.cancel_prepared_entry(key);
        let mut parts = key.split('\0');
        let (config_id, cwd, mcp) = (
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        if self.selected_agent_config_id.as_deref() == Some(config_id)
            && !isolates_new_sessions(config_id)
            && mcp.is_empty()
        {
            let request = PrepareRequest(
                config_id.to_owned(),
                cwd.to_owned(),
                None,
                Some(project_id.to_owned()),
                Some(PrepareOptions { silent: true }),
            );
            self.prepare_chat(request, fx);
        }
    }

    /// A promotion failed: the chat works but stays non-durable.
    pub(crate) fn on_promotion_failed(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        fx.push(Effect::Toast {
            level: "error".into(),
            message: "Chat history will not be saved for this session".into(),
        });
        self.book.ephemeral_sessions.insert(session_id.to_owned());
    }

    /// `reconcilePreparedSlot`: the slot's session when it is live and on
    /// the current launch; otherwise the slot goes and its session is
    /// reaped.
    fn reconcile_prepared_slot(&mut self, key: &str, fx: &mut Vec<Effect>) -> Option<String> {
        let session_id = self.prepared_sessions.get(key)?.clone();
        let session = self.sessions.get(&session_id);
        let live = session.is_some_and(|session| {
            session.status == SessionStatus::Active
                && self.agent_status.get(&session.agent_id) == Some(&json!("connected"))
        });
        if live && self.prepared_launch_is_current(key, &session_id) {
            return Some(session_id);
        }
        let message = match session {
            Some(session) if live => format!(
                "Dropped warm session {session_id}: its agent {} started with an outdated launch (env, Claude account or MCP servers)",
                session.agent_id
            ),
            Some(session) => format!(
                "Dropped stale prepared slot {session_id} (session {})",
                serde_json::to_value(session.status)
                    .ok()
                    .and_then(|status| status.as_str().map(str::to_owned))
                    .unwrap_or_default()
            ),
            None => format!("Dropped stale prepared slot {session_id} (session missing)"),
        };
        fx.push(Effect::info("acp.preparedSlot.stale", message));
        self.prepared_sessions.retain(|_, held| *held != session_id);
        if self.sessions.contains_key(&session_id)
            && self.active_session_id.as_deref() != Some(&session_id)
        {
            self.reap_orphan_prepared_session(&session_id, None, fx);
        }
        None
    }

    /// `preparedLaunchIsCurrent`: the agent runs the launch a new chat would
    /// get, with the MCP servers the list would give it now.
    fn prepared_launch_is_current(&self, key: &str, session_id: &str) -> bool {
        let Some(session) = self.sessions.get(session_id) else {
            return false;
        };
        let mut parts = key.split('\0');
        let (config_id, cwd) = (
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        let project_id = session.rest.get("projectId").and_then(Value::as_str);
        if !self.runs_current_launch(&session.agent_id, config_id, cwd, project_id) {
            return false;
        }
        let Some(recorded) = self.book.registry_mcp.get(session_id) else {
            return true;
        };
        let capabilities = self
            .agents
            .get(&session.agent_id)
            .and_then(|agent| agent.get("capabilities"))
            .filter(|caps| !caps.is_null());
        let (servers, _) = super::create::select_mcp_servers(&self.mcp_servers, capabilities);
        mcp_fingerprint(&servers) == *recorded
    }

    /// `reapOrphanPreparedSession`: dispose a warm session nobody claimed,
    /// then delete it. An indexed chat is never torn down. `then_stop` is an
    /// agent on an outdated launch, stopped once the session is gone.
    pub(crate) fn reap_orphan_prepared_session(
        &mut self,
        session_id: &str,
        then_stop: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let indexed = self
            .session_index
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id));
        if indexed {
            self.book.ephemeral_sessions.remove(session_id);
            return fx.push(Effect::warn(
                REAP_SOURCE,
                format!("Skipped teardown of indexed session {session_id}; orphan reap must not close or delete a persisted chat"),
            ));
        }
        if self.active_session_id.as_deref() == Some(session_id) {
            self.active_session_id = None;
        }
        let agent_id = self
            .sessions
            .get(session_id)
            .map(|session| session.agent_id.clone())
            .filter(|agent| !agent.is_empty());
        let reap = super::oneshot::Disposer::Reap {
            session_id: session_id.to_owned(),
            then_stop,
        };
        match agent_id {
            Some(agent_id) => fx.push(Effect::DisposeEphemeralSession {
                agent_id,
                session_id: session_id.to_owned(),
                by: reap,
            }),
            None => self.on_reaped(reap, None, fx),
        }
    }

    /// The reap's dispose is over: the session is deleted and dropped.
    pub(crate) fn on_reaped(
        &mut self,
        reap: super::oneshot::Disposer,
        failure: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let super::oneshot::Disposer::Reap {
            session_id,
            then_stop,
        } = reap
        else {
            return;
        };
        if let Some(message) = failure {
            fx.push(Effect::warn(
                REAP_SOURCE,
                format!("Failed to dispose warm session {session_id}: {message}"),
            ));
        }
        self.delete_history_session(session_id.clone(), fx);
        self.book.registry_mcp.remove(&session_id);
        self.drop_ephemeral_session(&session_id);
        if let Some(agent_id) = then_stop {
            self.stop_idle_outdated_agent(&agent_id, fx);
        }
    }

    /// `stopIdleOutdatedAgent`: an agent on an outdated launch that serves
    /// no session stops.
    fn stop_idle_outdated_agent(&mut self, agent_id: &str, fx: &mut Vec<Effect>) {
        if !self.agent_status.contains_key(agent_id) {
            return;
        }
        let serves = self
            .sessions
            .values()
            .any(|session| session.agent_id == agent_id && session.status != SessionStatus::Closed);
        if serves {
            return;
        }
        fx.push(Effect::info(
            PREPARE_SOURCE,
            format!("Stopping agent {agent_id}: it started with an outdated launch and serves no session"),
        ));
        self.kill_agent(agent_id.to_owned(), Killer::Quiet, fx);
    }
}
