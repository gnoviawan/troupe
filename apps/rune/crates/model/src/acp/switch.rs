//! The in-chat agent switch (`slices/switch.ts`): arming it
//! (`armAgentSwitch`, with its busy gate), the composer picks made while it
//! is armed (`setSwitchPendingOption`) and `cancelAgentSwitch`, the host's
//! durable `agent_switch` marker, and the continuation edge it stamps on the
//! index (`stampSwitchContinuation`). The switch itself (`switchAgent`, with
//! the handoff summary and the transcript splice) is not ported yet.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::import::RequestId;
use super::types::SessionStatus;
use super::warm::{PrepareOptions, PrepareRequest};
use super::{Effect, Env, State, js};

const ARM_SOURCE: &str = "acp.switchAgent.start";
const PICK_SOURCE: &str = "acp.setSwitchPendingOption";

/// `BACKGROUND_SUBAGENTS_BLOCK_REASON`.
const SUBAGENTS_BLOCK: &str = "background subagents are still running — switching would stop them";
/// `BACKGROUND_COMMANDS_BLOCK_REASON`.
const COMMANDS_BLOCK: &str =
    "commands are still running in the background — switching would stop them";

/// `MAX_LIVE_TOOL_CALLS`: the markers a chat keeps live.
const MAX_LIVE_SWITCHES: usize = 500;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSwitchEvent {
    pub session_id: String,
    pub from_config_id: Value,
    pub to_config_id: Value,
    /// The session the conversation continues in.
    pub new_session_id: Value,
    pub summary_text: Value,
}

/// `PendingLauncherOptions`, every field optional: the picks a launcher or
/// an armed switch holds for a session that does not exist yet.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherPicks {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub mode_id: Option<String>,
    #[serde(default)]
    pub config_values: Option<Map<String, Value>>,
}

impl State {
    /// `_onAgentSwitch`: add the separator, or update the one a reconnect
    /// already installed (same id, or same target and summary).
    pub(crate) fn on_agent_switch(
        &mut self,
        event: AgentSwitchEvent,
        event_seq: Option<f64>,
        env: &Env<'_>,
    ) {
        let session_id = event.session_id.as_str();
        if self.book.history_covered(session_id, event_seq) || !self.accepts_switch(session_id) {
            return;
        }
        let seq = event_seq
            .filter(|seq| seq.is_finite())
            .unwrap_or_else(|| self.book.next_seq());
        let record = json!({
            "id": format!("switch:seq-{}", js::string_of(Some(&json!(seq)))),
            "fromConfigId": event.from_config_id,
            "toConfigId": event.to_config_id,
            "newSessionId": event.new_session_id,
            "summaryText": event.summary_text,
            "timestamp": env.now,
            "seq": seq,
        });
        let target = js::string_of(Some(&event.new_session_id));
        self.stamp_switch_continuation(&target, session_id);
        let list = self
            .agent_switches
            .entry(session_id.to_owned())
            .or_default();
        let same = |held: &Value| {
            held.get("id") == record.get("id")
                || (held.get("toConfigId") == record.get("toConfigId")
                    && held.get("newSessionId") == record.get("newSessionId")
                    && held.get("summaryText") == record.get("summaryText"))
        };
        if let Some(held) = list.iter_mut().find(|held| same(held)) {
            // The installed record keeps its durable id, seq and arrival time.
            for key in ["fromConfigId", "toConfigId", "newSessionId", "summaryText"] {
                held[key] = record[key].clone();
            }
            return;
        }
        list.push(record);
        if list.len() > MAX_LIVE_SWITCHES {
            list.drain(..list.len() - MAX_LIVE_SWITCHES);
        }
    }

    fn accepts_switch(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .is_some_and(super::types::Session::accepts_transcript_events)
    }

    /// `stampSwitchContinuation`: the target's index row names the chat it
    /// continues, once, and only within the source's project. The stamped
    /// row moves to the front.
    pub(crate) fn stamp_switch_continuation(&mut self, target: &str, source: &str) {
        if target == source {
            return;
        }
        let find = |id: &str| {
            self.session_index
                .iter()
                .position(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        };
        let Some(at) = find(target) else {
            return;
        };
        let row = &self.session_index[at];
        if row.contains_key("continuedFrom") {
            return;
        }
        if let Some(source_row) = find(source).map(|at| &self.session_index[at])
            && source_row.get("projectId") != row.get("projectId")
        {
            return;
        }
        let mut row = self.session_index.remove(at);
        row.insert("continuedFrom".into(), json!(source));
        self.session_index.insert(0, row);
    }

    /// `switchBlockedReason`: the busy gate. A switch never stops a live
    /// turn, a restore, a launch, queued prompts, a wait on the user, or
    /// background work a new agent process would kill.
    pub(crate) fn switch_blocked_reason(&self, session_id: &str) -> Option<&'static str> {
        let Some(session) = self.sessions.get(session_id) else {
            return Some("session not found");
        };
        let mine = |id: &str| id == session_id;
        if session.turn_busy() {
            Some(
                "the agent is still working on a turn — cancel it or wait for it to finish before switching",
            )
        } else if session.replaying().is_some() {
            Some(
                "the chat history is still being restored — wait for it to finish before switching",
            )
        } else if self
            .launching_session_ids
            .get(session_id)
            .is_some_and(js::truthy)
        {
            Some("the chat is still starting up — wait for it to open before switching")
        } else if self
            .prompt_queues
            .get(session_id)
            .is_some_and(|queue| !queue.is_empty())
        {
            Some("queued prompts are waiting — let them send before switching")
        } else if self
            .pending_permissions
            .values()
            .any(|p| mine(&p.session_id))
        {
            Some("a permission request is waiting for your answer before switching")
        } else if self.pending_questions.values().any(|q| mine(&q.session_id)) {
            Some("the agent asked a question — answer it before switching")
        } else if self
            .pending_elicitations
            .values()
            .any(|e| mine(&e.session_id))
        {
            Some("the agent is waiting for your input before switching")
        } else if !session.agent_id.is_empty()
            && self
                .pending_sign_in
                .get(&session.agent_id)
                .is_some_and(js::truthy)
        {
            Some("the agent is waiting for you to sign in before switching")
        } else if self.runs_background_subagents(session_id) {
            Some(SUBAGENTS_BLOCK)
        } else if running_background_commands(session) {
            Some(COMMANDS_BLOCK)
        } else {
            None
        }
    }

    /// `setSwitchRejection`: the banner of the old chat says why.
    pub(crate) fn reject_switch(&mut self, session_id: &str, reason: &str) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.rest.insert("switching".into(), Value::Null);
            session.last_error = Some(Some(format!("Could not switch agent: {reason}")));
            session.last_error_code = Some(None);
            session.last_error_data = None;
        }
    }

    /// `armAgentSwitch`: the chat's next prompt goes to `to_config_id`.
    /// Answers whether the switch armed; the target warms meanwhile.
    pub(crate) fn arm_agent_switch(
        &mut self,
        session_id: &str,
        to_config_id: &str,
        request: RequestId,
        fx: &mut Vec<Effect>,
    ) {
        match self.try_arm_agent_switch(session_id, to_config_id, fx) {
            Ok(armed) => fx.push(Effect::ActionResolved {
                request,
                value: json!(armed),
            }),
            Err(message) => fx.push(Effect::ActionRejected { message }),
        }
    }

    fn try_arm_agent_switch(
        &mut self,
        session_id: &str,
        to_config_id: &str,
        fx: &mut Vec<Effect>,
    ) -> Result<bool, String> {
        if let Some(blocked) = self.switch_blocked_reason(session_id) {
            self.reject_switch(session_id, blocked);
            fx.push(Effect::warn(
                ARM_SOURCE,
                format!(
                    "Switch arm rejected for session {session_id} (busy gate) → config {to_config_id}: {blocked}"
                ),
            ));
            return Ok(false);
        }
        let agent_id = self.sessions[session_id].agent_id.clone();
        let current = self.config_id_for_agent(&agent_id).or_else(|| {
            self.session_index
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id))
                .and_then(|entry| entry.get("agentConfigId"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        if current.as_deref() == Some(to_config_id) {
            self.reject_switch(session_id, "that agent already owns this chat");
            return Ok(false);
        }
        let known = self
            .agent_configs
            .iter()
            .any(|config| config.get("id").and_then(Value::as_str) == Some(to_config_id));
        if !known {
            self.reject_switch(session_id, &format!("unknown agent config {to_config_id}"));
            fx.push(Effect::warn(
                ARM_SOURCE,
                format!(
                    "Switch arm rejected for session {session_id}: unknown config {to_config_id}"
                ),
            ));
            return Ok(false);
        }
        let session = self.sessions.get_mut(session_id).expect("gated above");
        session.rest.insert(
            "switching".into(),
            json!({ "toConfigId": to_config_id, "status": "pending" }),
        );
        // The TypeScript warm-up throws on a chat with no folder, after the
        // switch armed; the caller sees the arm fail.
        let Some(cwd) = session.rest.get("cwd").and_then(Value::as_str) else {
            return Err(format!(
                "TypeError: session {session_id} has no cwd to warm {to_config_id} in"
            ));
        };
        let cwd = cwd.to_owned();
        let project_id = session
            .rest
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let warm = PrepareRequest(
            to_config_id.to_owned(),
            cwd,
            None,
            project_id,
            Some(PrepareOptions { silent: true }),
        );
        self.prepare_chat(warm, fx);
        Ok(true)
    }

    /// `setSwitchPendingOption`: a composer pick on an armed chat is held
    /// for the session the switch opens and saved under the target config,
    /// never sent to the chat's current agent.
    pub(crate) fn set_switch_pending_option(
        &mut self,
        session_id: &str,
        patch: LauncherPicks,
        fx: &mut Vec<Effect>,
    ) {
        let armed = self
            .sessions
            .get(session_id)
            .and_then(|session| session.rest.get("switching"))
            .filter(|switching| js::truthy(switching))
            .cloned();
        let Some(switching) = armed else {
            fx.push(Effect::warn(
                PICK_SOURCE,
                format!("Dropped a composer pick for session {session_id}: no switch is armed"),
            ));
            return;
        };
        // The switch already read the picks for its new session.
        if self.book.switch.in_flight.contains(session_id) {
            fx.push(Effect::warn(
                PICK_SOURCE,
                format!(
                    "Dropped a composer pick for session {session_id}: the agent switch is already executing"
                ),
            ));
            return;
        }
        let to_config_id = js::string_of(switching.get("toConfigId"));
        let cwd = js::string_of(self.sessions[session_id].rest.get("cwd"));
        let key = super::warm::prepare_chat_key(&to_config_id, &cwd, None);
        let prepared = self
            .prepared_sessions
            .get(&key)
            .and_then(|id| self.sessions.get(id));
        let cached = self.agent_options_cache.get(&to_config_id);
        let field = |record: Option<&Value>| record.filter(|value| !value.is_null()).cloned();
        let target_options: Vec<Value> =
            field(prepared.and_then(|session| session.rest.get("configOptions")))
                .or_else(|| field(cached.and_then(|entry| entry.get("configOptions"))))
                .and_then(|options| options.as_array().cloned())
                .unwrap_or_default();
        // `sessionModelSnapshot(prepared ?? cache entry)`.
        let (snapshot_options, snapshot_models) = match prepared {
            Some(session) => (
                super::controls::config_options(session).to_vec(),
                field(session.rest.get("models")),
            ),
            None => (
                cached
                    .and_then(|entry| entry.get("configOptions"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                field(cached.and_then(|entry| entry.get("models"))),
            ),
        };
        let prev = switching.get("pendingOptions");
        let prev_values = prev
            .and_then(|prev| prev.get("configValues"))
            .and_then(Value::as_object);
        // A model pick brings its own values: only the agent-wide ones stay.
        let mut values = match patch.model_id {
            None => prev_values.cloned().unwrap_or_default(),
            Some(_) => agent_wide_values(prev_values.cloned().unwrap_or_default(), &target_options),
        };
        for (id, value) in patch.config_values.iter().flatten() {
            values.insert(id.clone(), value.clone());
        }
        let kept = |key: &str, picked: &Option<String>| {
            picked
                .clone()
                .map(Value::String)
                .or_else(|| prev.and_then(|prev| prev.get(key)).cloned())
                .filter(|value| !value.is_null())
        };
        let mut pending = Map::new();
        if let Some(model_id) = kept("modelId", &patch.model_id) {
            pending.insert("modelId".into(), model_id);
        }
        if let Some(mode_id) = kept("modeId", &patch.mode_id) {
            pending.insert("modeId".into(), mode_id);
        }
        pending.insert("configValues".into(), Value::Object(values));
        let session = self.sessions.get_mut(session_id).expect("armed above");
        session.rest["switching"]["pendingOptions"] = Value::Object(pending.clone());
        let picked_values = patch.config_values.clone().unwrap_or_default();
        if patch.model_id.is_none() && patch.mode_id.is_none() && picked_values.is_empty() {
            return;
        }
        let shown = |pick: &Option<String>| pick.clone().unwrap_or_else(|| "-".into());
        let keys = picked_values.keys().cloned().collect::<Vec<_>>().join(",");
        fx.push(Effect::info(
            PICK_SOURCE,
            format!(
                "Queued switch pick on session {session_id} → config {to_config_id}: model={} mode={} config={}",
                shown(&patch.model_id),
                shown(&patch.mode_id),
                if keys.is_empty() { "-".into() } else { keys },
            ),
        ));
        // Scoped values belong to the armed model; a model pick brings its own.
        let armed_model = pending
            .get("modelId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                super::controls::model_snapshot_of(&snapshot_options, snapshot_models.as_ref())
                    .map(|(id, _)| id)
            });
        let mut save = Map::new();
        if let Some(model_id) = &patch.model_id {
            save.insert("modelId".into(), json!(model_id));
        }
        if let Some(mode_id) = &patch.mode_id {
            save.insert("modeId".into(), json!(mode_id));
        }
        if let Some(config_values) = &patch.config_values {
            save.insert("configValues".into(), Value::Object(config_values.clone()));
        }
        let scoped = scoped_values(&picked_values, &target_options);
        if let Some(model) = armed_model.filter(|model| !model.is_empty())
            && !scoped.is_empty()
        {
            save.insert("modelValues".into(), json!({ model: scoped }));
        }
        fx.push(Effect::PersistComposerOptions {
            config_id: to_config_id,
            patch: Value::Object(save),
        });
    }

    /// `cancelAgentSwitch`: the armed switch is dropped.
    pub(crate) fn cancel_agent_switch(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.get_mut(session_id)
            && session.rest.get("switching").is_some_and(js::truthy)
        {
            session.rest.insert("switching".into(), Value::Null);
        }
    }
}

/// `runningBackgroundCommands(session) > 0`: commands the chat's own agent
/// reported running, while the chat is live.
fn running_background_commands(session: &super::types::Session) -> bool {
    if matches!(session.status, SessionStatus::Closed | SessionStatus::Error) {
        return false;
    }
    let Some(held) = &session.background_commands else {
        return false;
    };
    if held.get("agentId").and_then(Value::as_str) != Some(session.agent_id.as_str()) {
        return false;
    }
    held.get("commands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|command| command.get("state").and_then(Value::as_str) == Some("running"))
}

fn option_category<'a>(options: &'a [Value], id: &str) -> Option<Option<&'a str>> {
    options
        .iter()
        .find(|option| option.get("id").and_then(Value::as_str) == Some(id))
        .map(super::controls::category)
}

/// `agentWideValues`: the values that stay across a model pick, the
/// mode-category ones; all of them while no option is known.
fn agent_wide_values(values: Map<String, Value>, options: &[Value]) -> Map<String, Value> {
    if options.is_empty() {
        return values;
    }
    values
        .into_iter()
        .filter(|(id, _)| option_category(options, id) == Some(Some(super::picks::MODE_CATEGORY)))
        .collect()
}

/// `scopedValues`: the values of options that follow the model (neither the
/// model nor a mode); an option the list lacks is dropped.
fn scoped_values(values: &Map<String, Value>, options: &[Value]) -> Map<String, Value> {
    values
        .iter()
        .filter(|(id, _)| {
            option_category(options, id).is_some_and(|category| {
                !matches!(
                    category,
                    Some(super::controls::MODEL_CATEGORY | super::picks::MODE_CATEGORY)
                )
            })
        })
        .map(|(id, value)| (id.clone(), value.clone()))
        .collect()
}
