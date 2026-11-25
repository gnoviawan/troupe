//! `switchAgent` (`slices/switch.ts`): the chat's conversation moves to
//! another agent. The target agent and a new session come first, then the
//! picks made while the switch was armed, then the durable marker on the old
//! session, the old transcript spliced in front of the new one, the tab
//! remapped, and the handoff prompt (the summary and the draft) sent.
//!
//! Each wait is an effect whose answer moves the run on: the agent
//! (`ensureLiveAgent`), `session/new`, the picks, the marker write
//! (`recordAgentSwitch`), the prompt's dispatch, and on a rollback the kill
//! of an agent spawned for the switch. The action answers `actionResolved`
//! with the new session id, or null when the switch did not happen.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::create::{CreateCaller, CreateOptions, CreateRequest};
use super::ensure::{Caller, EnsureOptions};
use super::handoff::{build_handoff_summary, sanitize_wire_blocks};
use super::host_error::HostError;
use super::import::RequestId;
use super::launcher_options::LauncherCaller;
use super::reuse_key::{detached_reuse_key, parse_reuse_key};
use super::splice::{Band, splice_switch_transcript, with_switch_marker};
use super::types::{Object, SessionStatus};
use super::{Effect, Env, State, js};

/// Names one `switchAgent` call.
pub type SwitchId = u64;

const START: &str = "acp.switchAgent.start";
const FAILURE: &str = "acp.switchAgent.failure";
const SUCCESS: &str = "acp.switchAgent.success";
const SPLICE: &str = "acp.switchAgent.splice";
const ROLLBACK: &str = "acp.switchAgent.rollback";

/// What the composer hands the switch: the draft, and the wire and display
/// blocks its submit seam built from it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPending {
    #[serde(default)]
    pub pending_text: Option<String>,
    #[serde(default)]
    pub wire_blocks: Option<Vec<Value>>,
    #[serde(default)]
    pub display_blocks: Option<Vec<Value>>,
    /// The chat whose Claude, Codex or Antigravity account the spawn uses.
    #[serde(default)]
    pub account_chat_id: Option<String>,
}

/// The arguments of `switchAgent`, then the app's request id.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SwitchRequest(
    pub String,
    pub String,
    #[serde(default)] pub Option<SwitchPending>,
    #[serde(default)] pub RequestId,
);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SwitchRun {
    /// The app's request, when the switch is its own action; a send the
    /// switch took over gets no answer.
    request: Option<RequestId>,
    session_id: String,
    to_config_id: String,
    old_config_id: Option<String>,
    old_agent_id: String,
    cwd: String,
    project_id: Option<String>,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
    summary_text: String,
    wire_blocks: Vec<Value>,
    display_blocks: Vec<Value>,
    new_agent_id: Option<String>,
    new_session_id: Option<String>,
    marker_warned: bool,
    /// Why the switch rolls back, while its kill runs.
    failure: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SwitchBook {
    seq: SwitchId,
    runs: BTreeMap<SwitchId, SwitchRun>,
    /// Sessions whose switch runs (`inFlightAgentSwitches`).
    pub(crate) in_flight: BTreeSet<String>,
    /// Per target, the source a live switch spliced into it and its marker
    /// (`liveSwitchSources`), which a later reinstall splices again.
    pub(crate) live_sources: BTreeMap<String, (String, Value)>,
}

fn optional_text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

impl State {
    fn switch_run(&self, id: SwitchId) -> Option<&SwitchRun> {
        self.book.switch.runs.get(&id)
    }

    /// The old chat's config: the agent's reuse key, else the index row.
    pub(crate) fn chat_config_id(&self, session_id: &str, agent_id: &str) -> Option<String> {
        self.config_id_for_agent(agent_id).or_else(|| {
            self.session_index
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id))
                .and_then(|entry| entry.get("agentConfigId"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    }

    fn clear_switching(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.get_mut(session_id)
            && session.rest.get("switching").is_some_and(js::truthy)
        {
            session.rest.insert("switching".into(), Value::Null);
        }
    }

    /// The staged send: a send on an armed chat runs the switch with the
    /// composer's draft instead of a turn on the old agent. False when no
    /// switch is armed.
    pub(crate) fn send_into_armed_switch(
        &mut self,
        session_id: &str,
        pending: SwitchPending,
        fx: &mut Vec<Effect>,
    ) -> bool {
        let Some(to_config_id) = self
            .sessions
            .get(session_id)
            .and_then(|session| session.rest.get("switching"))
            .filter(|switching| js::truthy(switching))
            .map(|switching| js::string_of(switching.get("toConfigId")))
        else {
            return false;
        };
        let request = SwitchRequest(session_id.to_owned(), to_config_id, Some(pending), 0);
        self.switch_agent(request, false, fx);
        true
    }

    /// `switchAgent`; `answers` when the app waits for its outcome.
    pub(crate) fn switch_agent(
        &mut self,
        request: SwitchRequest,
        answers: bool,
        fx: &mut Vec<Effect>,
    ) {
        let SwitchRequest(session_id, to_config_id, pending, request) = request;
        let request = answers.then_some(request);
        let Some(old) = self.sessions.get(&session_id) else {
            return fx.push(Effect::rejected(format!("unknown session {session_id}")));
        };
        let resolve_null = |fx: &mut Vec<Effect>| {
            if let Some(request) = request {
                fx.push(Effect::ActionResolved {
                    request,
                    value: Value::Null,
                });
            }
        };
        if self.book.switch.in_flight.contains(&session_id) {
            return resolve_null(fx);
        }
        let old_agent_id = old.agent_id.clone();
        let cwd = js::string_of(old.rest.get("cwd"));
        let project_id = optional_text(old.rest.get("projectId"));
        let worktree_path = optional_text(old.rest.get("worktreePath"));
        let worktree_branch = optional_text(old.rest.get("worktreeBranch"));
        let closed = old.status == SessionStatus::Closed;
        let config_name = |id: Option<&str>| {
            self.agent_configs
                .iter()
                .find(|config| config.get("id").and_then(Value::as_str) == id)
                .map(|config| {
                    config
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
        };
        let known = config_name(Some(&to_config_id)).is_some();
        let old_config_id = self.chat_config_id(&session_id, &old_agent_id);
        let from_name = config_name(old_config_id.as_deref()).flatten();
        fx.push(Effect::info(
            START,
            format!(
                "Switching session {session_id} from config {} to config {to_config_id}",
                old_config_id.as_deref().unwrap_or("?")
            ),
        ));
        if let Some(blocked) = self.switch_blocked_reason(&session_id) {
            self.reject_switch(&session_id, blocked);
            fx.push(Effect::warn(
                FAILURE,
                format!(
                    "Switch blocked for session {session_id} (busy gate) → config {to_config_id}: {blocked}"
                ),
            ));
            return resolve_null(fx);
        }
        if !known {
            self.reject_switch(&session_id, &format!("unknown agent config {to_config_id}"));
            fx.push(Effect::warn(
                FAILURE,
                format!("Switch failed for session {session_id}: unknown config {to_config_id}"),
            ));
            return resolve_null(fx);
        }
        if old_config_id.as_deref() == Some(to_config_id.as_str()) && !closed {
            self.reject_switch(&session_id, "that agent already owns this chat");
            return resolve_null(fx);
        }
        let pending = pending.unwrap_or_default();
        let pending_text = pending.pending_text.unwrap_or_default();
        let draft = |blocks: Option<Vec<Value>>| match blocks.filter(|b| !b.is_empty()) {
            Some(blocks) => blocks,
            None if !crate::js_trim(&pending_text).is_empty() => {
                vec![json!({ "type": "text", "text": pending_text })]
            }
            None => Vec::new(),
        };
        let pending_wire = draft(pending.wire_blocks);
        let display_blocks = draft(pending.display_blocks);
        let handoff = build_handoff_summary(
            self.messages.get(&session_id).map_or(&[], Vec::as_slice),
            self.tool_calls.get(&session_id).map_or(&[], Vec::as_slice),
            from_name.as_deref(),
            &pending_text,
        );
        let wire_blocks = if pending_wire.is_empty() {
            handoff.wire_blocks
        } else if handoff.summary_text.is_empty() {
            sanitize_wire_blocks(&pending_wire)
        } else {
            let summary = json!({ "type": "text", "text": handoff.summary_text });
            [vec![summary], sanitize_wire_blocks(&pending_wire)].concat()
        };
        fx.push(Effect::DeletePersisted {
            key: format!(
                "chat-draft/{}/{session_id}",
                project_id.as_deref().unwrap_or("undefined")
            ),
        });
        self.book.switch.in_flight.insert(session_id.clone());
        let book = &mut self.book.switch;
        book.seq += 1;
        let id = book.seq;
        book.runs.insert(
            id,
            SwitchRun {
                request,
                session_id,
                to_config_id: to_config_id.clone(),
                old_config_id,
                old_agent_id,
                cwd: cwd.clone(),
                project_id: project_id.clone(),
                worktree_path,
                worktree_branch,
                summary_text: handoff.summary_text,
                wire_blocks,
                display_blocks,
                new_agent_id: None,
                new_session_id: None,
                marker_warned: false,
                failure: None,
            },
        );
        let options = EnsureOptions {
            project_id,
            session_id: pending.account_chat_id,
            ..EnsureOptions::default()
        };
        self.ensure_live_agent(&to_config_id, &cwd, options, Caller::Switch(id), fx);
    }

    /// `ensureLiveAgent` answered the switch.
    pub(crate) fn on_switch_agent(
        &mut self,
        id: SwitchId,
        outcome: Result<Option<String>, HostError>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        let agent_id = match outcome {
            Err(error) => return self.roll_back_switch(id, error.message, fx),
            Ok(None) => {
                let message = format!("failed to spawn agent for config {}", run.to_config_id);
                return self.roll_back_switch(id, message, fx);
            }
            Ok(Some(agent_id)) => agent_id,
        };
        if let Some(run) = self.book.switch.runs.get_mut(&id) {
            run.new_agent_id = Some(agent_id.clone());
        }
        let request = CreateRequest(
            agent_id,
            run.cwd,
            None,
            run.project_id,
            Some(CreateOptions {
                worktree_path: run.worktree_path,
                worktree_branch: run.worktree_branch,
                ..CreateOptions::default()
            }),
            0,
        );
        self.create_session_for(request, CreateCaller::Switch(id), fx);
    }

    /// `createSession` answered the switch: the armed picks go to the new
    /// session before anything else.
    pub(crate) fn on_switch_session(
        &mut self,
        id: SwitchId,
        outcome: Result<String, HostError>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        let new_session_id = match outcome {
            Err(error) => return self.roll_back_switch(id, error.message, fx),
            Ok(session_id) => session_id,
        };
        if let Some(run) = self.book.switch.runs.get_mut(&id) {
            run.new_session_id = Some(new_session_id.clone());
        }
        let armed = self
            .sessions
            .get(&run.session_id)
            .and_then(|session| session.rest.get("switching"))
            .and_then(|switching| switching.get("pendingOptions"))
            .filter(|picks| !picks.is_null())
            .and_then(|picks| serde_json::from_value(picks.clone()).ok());
        match armed {
            Some(picks) => self.apply_pending_launcher_options(
                new_session_id,
                Some(picks),
                LauncherCaller::Switch(id),
                fx,
            ),
            None => self.record_switch_marker(id, fx),
        }
    }

    /// The durable marker goes on the old session's log.
    pub(crate) fn record_switch_marker(&mut self, id: SwitchId, fx: &mut Vec<Effect>) {
        let Some(run) = self.switch_run(id) else {
            return;
        };
        fx.push(Effect::RecordAgentSwitch {
            session_id: run.session_id.clone(),
            marker: json!({
                "fromConfigId": run.old_config_id.clone().unwrap_or_default(),
                "toConfigId": run.to_config_id,
                "newSessionId": run.new_session_id,
                "summaryText": run.summary_text,
            }),
            switch: id,
        });
    }

    /// The marker write is over. A failure does not stop the switch: the
    /// conversation goes on, and a reopen comes back to the original agent.
    pub(crate) fn on_switch_marker(
        &mut self,
        id: SwitchId,
        failure: Option<String>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        let target = run.new_session_id.clone().unwrap_or_default();
        if let Some(message) = failure {
            if let Some(run) = self.book.switch.runs.get_mut(&id) {
                run.marker_warned = true;
            }
            fx.push(Effect::Toast {
                level: "warning".into(),
                message: "Could not save the switch marker".into(),
            });
            fx.push(Effect::warn(
                FAILURE,
                format!(
                    "Marker write failed for session {} → config {}, new session {target} (continuing without it): {message}",
                    run.session_id, run.to_config_id
                ),
            ));
        }
        self.splice_live_switch(&run, &target, env, fx);
        if self.book.open_chat_tabs.remove(&run.session_id) {
            self.book.open_chat_tabs.insert(target.clone());
            fx.push(Effect::RemapChatTab {
                from: run.session_id.clone(),
                to: target.clone(),
            });
        }
        if run.wire_blocks.is_empty() {
            return self.finish_switch(id, env.now, fx);
        }
        self.send_handoff_prompt(&target, id, run.wire_blocks, run.display_blocks, env, fx);
    }

    /// `spliceLiveSwitchTranscript`: the old chat's transcript, with one
    /// separator, in front of the new session's, so the remapped tab shows
    /// the whole conversation. The old chat's own records stay as they are.
    fn splice_live_switch(
        &mut self,
        run: &SwitchRun,
        target: &str,
        env: &Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let source = run.session_id.as_str();
        let target_switches = self.agent_switches.get(target).cloned().unwrap_or_default();
        if target_switches
            .iter()
            .any(|sw| sw.get("newSessionId").and_then(Value::as_str) == Some(target))
        {
            return fx.push(Effect::info(
                SPLICE,
                format!(
                    "Live switch splice for session {target} already applied; skipping re-entry"
                ),
            ));
        }
        let skip = if target == source {
            Some("equals source session")
        } else if !self.sessions.contains_key(target) {
            Some("no longer exists")
        } else if !self.messages.contains_key(target) {
            Some("transcript was torn down")
        } else {
            None
        };
        if let Some(reason) = skip {
            return fx.push(Effect::warn(
                SPLICE,
                format!("Live switch splice skipped: target {target} {reason}"),
            ));
        }
        // A chat closed or deleted mid-switch has no live transcript left.
        let messages: Vec<_> = self
            .messages
            .get(source)
            .into_iter()
            .flatten()
            .map(|message| {
                let mut copy = message.clone();
                copy.streaming = false;
                copy
            })
            .collect();
        let tool_calls: Vec<Object> = self
            .tool_calls
            .get(source)
            .into_iter()
            .flatten()
            .map(|call| {
                let mut copy = call.clone();
                let status = call.get("status").and_then(Value::as_str);
                if !matches!(status, Some("completed" | "failed")) {
                    copy.insert("status".into(), json!("failed"));
                }
                copy
            })
            .collect();
        let marker = json!({
            "fromConfigId": run.old_config_id.clone().unwrap_or_default(),
            "toConfigId": run.to_config_id,
            "newSessionId": target,
            "summaryText": run.summary_text,
            "timestamp": env.now,
        });
        let band = Band {
            messages,
            tool_calls,
            switches: self.agent_switches.get(source).cloned().unwrap_or_default(),
        };
        let (message_count, call_count) = (band.messages.len(), band.tool_calls.len());
        let (band, fabricated) = with_switch_marker(band, &marker);
        let target_band = Band {
            messages: self.messages.remove(target).unwrap_or_default(),
            tool_calls: self.tool_calls.remove(target).unwrap_or_default(),
            switches: target_switches,
        };
        let spliced = splice_switch_transcript(source, &band, target_band);
        self.messages.insert(target.to_owned(), spliced.messages);
        let mut calls = spliced.tool_calls;
        super::window::trim_live_tool_calls(&mut calls);
        self.tool_calls.insert(target.to_owned(), calls);
        self.agent_switches
            .insert(target.to_owned(), spliced.switches);
        if let Some(plan) = self.plans.get(source).cloned() {
            self.plans.insert(target.to_owned(), plan);
        }
        self.trim_live_window(target, fx);
        self.book
            .switch
            .live_sources
            .insert(target.to_owned(), (source.to_owned(), marker));
        let kind = if fabricated {
            "fabricated switch marker"
        } else {
            "real switch marker"
        };
        fx.push(Effect::info(
            SPLICE,
            format!(
                "Spliced pre-switch transcript of session {source} into {target}: {message_count} message(s), {call_count} tool call(s), {kind}"
            ),
        ));
    }

    /// The handoff prompt was dispatched and answered (`None`), or failed.
    pub(crate) fn on_switch_dispatched(
        &mut self,
        id: SwitchId,
        failure: Option<String>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(message) = failure else {
            return self.finish_switch(id, env.now, fx);
        };
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        // The switch is durable once the session exists: the new session
        // shows the failure, and the rest of the switch still runs.
        let target = run.new_session_id.clone().unwrap_or_default();
        self.splice_live_switch(&run, &target, env, fx);
        if let Some(session) = self.sessions.get_mut(&target) {
            session.active_turn = false;
            session.open_turn_id = None;
            session.last_error = Some(Some(format!(
                "Could not deliver the handoff prompt: {message}"
            )));
            session.last_error_code = Some(None);
            session.last_error_data = None;
        }
        self.settle_switch(&run, env.now);
        fx.push(Effect::warn(
            FAILURE,
            format!(
                "Handoff dispatch failed for session {} → config {}, new session {target} (switch already durable): {message}",
                run.session_id, run.to_config_id
            ),
        ));
        self.end_switch(id, Some(target), fx);
    }

    /// The old agent's reuse keys detach, the index records the hop, and the
    /// armed switch clears.
    fn settle_switch(&mut self, run: &SwitchRun, now: f64) {
        let target = run.new_session_id.clone().unwrap_or_default();
        if let Some(old_config) = &run.old_config_id
            && run.new_agent_id.as_deref() != Some(run.old_agent_id.as_str())
        {
            self.detach_old_agent_for_switch(old_config, &run.cwd, &run.old_agent_id);
        }
        self.append_ordered_agents(
            &run.session_id,
            run.old_config_id.as_deref(),
            &run.to_config_id,
            now,
        );
        self.stamp_switch_continuation(&target, &run.session_id);
        self.clear_switching(&run.session_id);
    }

    pub(crate) fn finish_switch(&mut self, id: SwitchId, now: f64, fx: &mut Vec<Effect>) {
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        self.settle_switch(&run, now);
        let target = run.new_session_id.clone().unwrap_or_default();
        let warned = if run.marker_warned {
            " (marker write failed — non-blocking)"
        } else {
            ""
        };
        fx.push(Effect::info(
            SUCCESS,
            format!(
                "Switched session {} (config {}) to config {}, new session {target}{warned}",
                run.session_id,
                run.old_config_id.as_deref().unwrap_or("?"),
                run.to_config_id
            ),
        ));
        self.end_switch(id, Some(target), fx);
    }

    /// No new session: the old chat stays as it was, with the reason on its
    /// banner. An agent spawned for the switch alone is stopped first.
    fn roll_back_switch(&mut self, id: SwitchId, message: String, fx: &mut Vec<Effect>) {
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        self.reject_switch(&run.session_id, &message);
        if let Some(agent_id) = run.new_agent_id.clone() {
            let used = self.sessions.values().any(|session| {
                session.agent_id == agent_id
                    && (session.status != SessionStatus::Closed
                        || self.book.history.opening(&session.id))
            });
            if !used {
                if let Some(run) = self.book.switch.runs.get_mut(&id) {
                    run.failure = Some(message);
                }
                return self.kill_agent(agent_id, super::Killer::Switch(id), fx);
            }
        }
        self.end_rollback(id, &message, fx);
    }

    /// The rollback's kill is over; `failure` is its error.
    pub(crate) fn on_switch_kill(
        &mut self,
        id: SwitchId,
        failure: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(run) = self.switch_run(id).cloned() else {
            return;
        };
        if let Some(error) = failure {
            fx.push(Effect::warn(
                ROLLBACK,
                format!(
                    "Could not stop the just-spawned agent {} after a failed session/new: {error}",
                    run.new_agent_id.unwrap_or_default()
                ),
            ));
        }
        let message = run.failure.unwrap_or_default();
        self.end_rollback(id, &message, fx);
    }

    fn end_rollback(&mut self, id: SwitchId, message: &str, fx: &mut Vec<Effect>) {
        let Some(run) = self.switch_run(id) else {
            return;
        };
        fx.push(Effect::error(
            FAILURE,
            format!(
                "Switch failed for session {} → config {}: {message}",
                run.session_id, run.to_config_id
            ),
        ));
        self.end_switch(id, None, fx);
    }

    fn end_switch(&mut self, id: SwitchId, new_session_id: Option<String>, fx: &mut Vec<Effect>) {
        let Some(run) = self.book.switch.runs.remove(&id) else {
            return;
        };
        self.book.switch.in_flight.remove(&run.session_id);
        if let Some(request) = run.request {
            fx.push(Effect::ActionResolved {
                request,
                value: json!(new_session_id),
            });
        }
    }

    /// `detachOldAgentForSwitch`: every canonical key of the old process for
    /// the old config and folder moves to a detached key, so no new chat
    /// reuses it; its open sessions still resolve it.
    fn detach_old_agent_for_switch(&mut self, config_id: &str, cwd: &str, agent_id: &str) {
        let cwd = crate::js_trim(cwd);
        let keys: Vec<String> = self
            .config_to_live_agent
            .iter()
            .filter(|(key, live)| {
                let parsed = parse_reuse_key(key);
                parsed.detached_agent_id.is_none()
                    && live.as_str() == Some(agent_id)
                    && parsed.config_id == config_id
                    && parsed.cwd == cwd
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            self.config_to_live_agent.shift_remove(&key);
            self.config_to_live_agent
                .insert(detached_reuse_key(&key, agent_id), json!(agent_id));
        }
    }

    /// `appendOrderedAgents`: the chat's index row lists the configs it ran
    /// on, oldest first, and moves to the front.
    fn append_ordered_agents(
        &mut self,
        session_id: &str,
        original: Option<&str>,
        to_config_id: &str,
        now: f64,
    ) {
        let Some(at) = self
            .session_index
            .iter()
            .position(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id))
        else {
            return;
        };
        let row = &self.session_index[at];
        let mut list: Vec<Value> = match row.get("agents").and_then(Value::as_array) {
            Some(agents) => agents.clone(),
            None => original.map(|id| vec![json!(id)]).unwrap_or_default(),
        };
        if list.is_empty()
            && let Some(config) = row.get("agentConfigId").filter(|c| js::truthy(c))
        {
            list.push(config.clone());
        }
        if list.last().and_then(Value::as_str) == Some(to_config_id) {
            return;
        }
        list.push(json!(to_config_id));
        let mut row = self.session_index.remove(at);
        row.insert("lastActivityAt".into(), json!(now));
        row.insert("agents".into(), Value::Array(list));
        self.session_index.insert(0, row);
    }
}
