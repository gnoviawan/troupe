//! Session lifecycle: the host's `session_created` and `session_closed`, the
//! cancel of the subagents a gone process leaves running, the active chat and
//! a failed resume. Port of the event handlers of `slices/session.ts`,
//! `cancelRunningSubagents` (`subagent-lifecycle.ts`) and the helpers they
//! call in `helpers.ts` and `components/chat/subagent-strip.ts`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::types::{Object, Session, SessionStatus};
use super::{Effect, Env, State, clamp, js};

const RESUME_FAILED_PREFIX: &str = "Resume failed: ";
const CODEX_CLI_SIGNED_OUT_MESSAGE: &str = "Codex signed out. Sign in again to continue this chat.";
/// The transport tags a host sign-in refusal carries before its sentence.
const SIGNED_OUT_TAGS: [&str; 2] = [
    "ACP_CLAUDE_ACCOUNT_SIGNED_OUT: ",
    "ACP_CODEX_ACCOUNT_SIGNED_OUT: ",
];

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreatedEvent {
    pub agent_id: String,
    pub session_id: String,
    #[serde(default)]
    pub modes: Option<Value>,
    #[serde(default)]
    pub models: Option<Value>,
    #[serde(default)]
    pub config_options: Option<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionClosedEvent {
    #[serde(default)]
    pub agent_id: Option<String>,
    pub session_id: String,
}

/// `creationOptionDefaultsFrom`: the option values a session was created
/// with, the baseline the stale-echo guards compare against.
fn creation_option_defaults(
    modes: Option<&Value>,
    models: Option<&Value>,
    config_options: &[Value],
) -> Value {
    let mut config_values = Object::new();
    for option in config_options {
        let id = js::string_of(option.get("id"));
        match option.get("currentValue") {
            Some(Value::Bool(flag)) => {
                config_values.insert(id, json!(flag.to_string()));
            }
            Some(value) => {
                config_values.insert(id, value.clone());
            }
            None => {
                config_values.shift_remove(&id);
            }
        }
    }
    let mut defaults = Object::new();
    if let Some(id) = modes.and_then(|modes| modes.get("currentModeId")) {
        defaults.insert("modeId".into(), id.clone());
    }
    if let Some(id) = models.and_then(|models| models.get("currentModelId")) {
        defaults.insert("modelId".into(), id.clone());
    }
    defaults.insert("configValues".into(), Value::Object(config_values));
    Value::Object(defaults)
}

/// `appendUnseen`: `current` with each item enriched from its namesake in
/// `incoming`, then the incoming items whose key `current` lacks.
fn append_unseen(
    current: &[Value],
    incoming: &[Value],
    key: &dyn Fn(&Value) -> String,
    enrich: Option<fn(&Value, &Value) -> Value>,
) -> Vec<Value> {
    let mut merged: Vec<Value> = current
        .iter()
        .map(|item| {
            let other = enrich.and_then(|enrich| {
                let wanted = key(item);
                incoming
                    .iter()
                    .rev()
                    .find(|other| key(other) == wanted)
                    .map(|other| enrich(item, other))
            });
            other.unwrap_or_else(|| item.clone())
        })
        .collect();
    let seen: Vec<String> = current.iter().map(key).collect();
    merged.extend(
        incoming
            .iter()
            .filter(|item| !seen.contains(&key(item)))
            .cloned(),
    );
    merged
}

fn items(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// `!x.options`: absent or `null` reads as no list.
fn options_of(value: &Value) -> Option<&[Value]> {
    match value.get("options") {
        None | Some(Value::Null) => None,
        Some(options) => Some(items(Some(options))),
    }
}

fn with_options(value: &Value, options: Vec<Value>) -> Value {
    let mut next = value.clone();
    next["options"] = Value::Array(options);
    next
}

/// `configEntryKey`: a group by its name, a value by its value.
fn config_entry_key(entry: &Value) -> String {
    if entry.get("options").is_some_and(Value::is_array) {
        let group = entry
            .get("group")
            .filter(|group| !group.is_null())
            .or_else(|| entry.get("name"));
        format!("group:{}", js::string_of(group))
    } else {
        format!("value:{}", js::string_of(entry.get("value")))
    }
}

/// `appendUnseenGroupValues`.
fn append_unseen_group_values(entry: &Value, other: &Value) -> Value {
    let (Some(mine), Some(theirs)) = (options_of(entry), options_of(other)) else {
        return entry.clone();
    };
    let value_key = |value: &Value| js::string_of(value.get("value"));
    with_options(entry, append_unseen(mine, theirs, &value_key, None))
}

/// `appendUnseenConfigValues`.
fn append_unseen_config_values(option: &Value, other: &Value) -> Value {
    let (Some(mine), Some(theirs)) = (options_of(option), options_of(other)) else {
        return option.clone();
    };
    with_options(
        option,
        append_unseen(
            mine,
            theirs,
            &config_entry_key,
            Some(append_unseen_group_values),
        ),
    )
}

/// `addAdvertisedOptionsKeepingCurrent`: keep the session's current picks
/// and add the modes, models and option values it did not list yet.
fn add_advertised_options(session: &mut Session, event: &SessionCreatedEvent) {
    for (field, list, id, payload) in [
        ("modes", "availableModes", "id", event.modes.as_ref()),
        (
            "models",
            "availableModels",
            "modelId",
            event.models.as_ref(),
        ),
    ] {
        let current = session.rest.get(field).filter(|value| !value.is_null());
        let next = match (current, payload.filter(|value| !value.is_null())) {
            (Some(current), Some(payload)) if current.is_object() && payload.is_object() => {
                let mut merged = current.clone();
                merged[list] = Value::Array(append_unseen(
                    items(current.get(list)),
                    items(payload.get(list)),
                    &|item| js::string_of(item.get(id)),
                    None,
                ));
                merged
            }
            // TypeScript throws on a list it cannot read; the model keeps
            // what the session showed.
            (current, payload) => current.or(payload).cloned().unwrap_or(Value::Null),
        };
        session.rest.insert(field.into(), next);
    }
    let options = append_unseen(
        items(session.rest.get("configOptions")),
        event.config_options.as_deref().unwrap_or_default(),
        &|option| js::string_of(option.get("id")),
        Some(append_unseen_config_values),
    );
    session
        .rest
        .insert("configOptions".into(), Value::Array(options));
}

/// The latest user message's `seq` and `timestamp` (`lastUserBoundary`).
struct Boundary {
    seq: Option<f64>,
    timestamp: Option<f64>,
}

/// `isAfterBoundary`.
fn after_boundary(call: &Object, boundary: &Boundary) -> bool {
    let seq = call.get("seq").and_then(Value::as_f64);
    let timestamp = call.get("timestamp").and_then(Value::as_f64);
    if let (Some(call), Some(boundary)) = (seq, boundary.seq) {
        return call > boundary;
    }
    if let (Some(call), Some(boundary)) = (timestamp, boundary.timestamp) {
        return call > boundary;
    }
    boundary.seq.is_none() && boundary.timestamp.is_none()
}

fn activity(record: &Value) -> Option<&str> {
    record.get("activity").and_then(Value::as_str)
}

fn is_active(record: &Value) -> bool {
    matches!(activity(record), Some("running" | "requires_action"))
}

fn live_progress(record: &Value) -> bool {
    record
        .get("caps")
        .and_then(|caps| caps.get("liveProgress"))
        .is_some_and(js::truthy)
}

fn background(record: &Value) -> bool {
    record.get("background").is_some_and(js::truthy)
}

/// `isSubagentRunning` for a turn that is not active: `ended` (the latest
/// turn) or not (`superseded`).
fn running_after_turn(call: &Object, record: &Value, latest_turn: bool) -> bool {
    if matches!(activity(record), Some("idle" | "unknown")) {
        return false;
    }
    if live_progress(record) {
        return true;
    }
    background(record)
        && call.get("status").and_then(Value::as_str) == Some("completed")
        && latest_turn
}

/// `runningBackgroundSubagents`: the background subagents of a chat that
/// have not ended, by call id.
fn running_background(calls: &[Object], boundary: Option<&Boundary>) -> Vec<String> {
    calls
        .iter()
        .filter_map(|call| {
            let record = call.get("subagent").filter(|record| record.is_object())?;
            let launch = background(record) || (live_progress(record) && is_active(record));
            if !launch || call.get("restoredSummary").is_some_and(js::truthy) {
                return None;
            }
            let latest = boundary.is_none_or(|boundary| after_boundary(call, boundary));
            running_after_turn(call, record, latest).then(|| js::string_of(call.get("toolCallId")))
        })
        .collect()
}

impl State {
    pub(crate) fn on_session_created(
        &mut self,
        event: SessionCreatedEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let id = event.session_id.clone();
        if let Some(session) = self.sessions.get_mut(&id) {
            add_advertised_options(session, &event);
            if session
                .rest
                .get("creationOptionDefaults")
                .is_none_or(Value::is_null)
            {
                let defaults = creation_option_defaults(
                    event.modes.as_ref(),
                    event.models.as_ref(),
                    event.config_options.as_deref().unwrap_or_default(),
                );
                session
                    .rest
                    .insert("creationOptionDefaults".into(), defaults);
            }
        } else {
            let defaults = creation_option_defaults(
                event.modes.as_ref(),
                event.models.as_ref(),
                event.config_options.as_deref().unwrap_or_default(),
            );
            let rest = json!({
                "cwd": "",
                "projectId": "",
                "mcpServerCount": 0,
                "modes": event.modes.clone().unwrap_or(Value::Null),
                "models": event.models.clone().unwrap_or(Value::Null),
                "configOptions": event.config_options.clone().unwrap_or_default(),
                "creationOptionDefaults": defaults,
                "createdAt": env.now,
            });
            let Value::Object(rest) = rest else {
                unreachable!()
            };
            self.sessions.insert(
                id.clone(),
                Session {
                    id: id.clone(),
                    agent_id: event.agent_id,
                    status: SessionStatus::Active,
                    title: None,
                    active_turn: false,
                    open_turn_id: None,
                    agent_working: None,
                    replaying: Some(None),
                    opencode_notice: None,
                    last_error: None,
                    last_error_code: None,
                    last_error_data: None,
                    background_commands: None,
                    rest,
                },
            );
            self.messages.entry(id.clone()).or_default();
        }
        self.cache_options_from_session(&id, env, fx);
        self.refresh_index(env, fx);
    }

    pub(crate) fn on_session_closed(
        &mut self,
        event: SessionClosedEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let id = event.session_id;
        let owner = self.sessions.get(&id).map(|s| s.agent_id.as_str());
        if let (Some(agent), Some(owner)) = (event.agent_id.as_deref(), owner)
            && !agent.is_empty()
            && !owner.is_empty()
            && owner != agent
        {
            return;
        }
        self.cancel_running_subagents(std::slice::from_ref(&id), env.now, fx);
        self.pending_permissions
            .retain(|_, pending| pending.session_id != id);
        self.pending_questions
            .retain(|_, pending| pending.session_id != id);
        self.prepared_sessions.retain(|_, prepared| *prepared != id);
        if let Some(session) = self.sessions.get_mut(&id) {
            if session.status != SessionStatus::Error {
                session.status = SessionStatus::Closed;
            }
            session.active_turn = false;
            session.open_turn_id = None;
            session.replaying = Some(None);
            fx.push(Effect::PersistSession {
                session_id: id.clone(),
            });
        }
        let keep_transcript = self
            .sessions
            .get(&id)
            .is_some_and(|session| session.status != SessionStatus::Error)
            && self.book.open_chat_tabs.contains(&id);
        if keep_transcript {
            self.drop_session_bookkeeping(&id);
        } else if self.sessions.contains_key(&id) {
            fx.push(Effect::DropSessionTranscript { session_id: id });
        } else {
            self.drop_session_transcript(&id);
        }
        self.refresh_index(env, fx);
    }

    /// `refreshHostOwnedIndex`: the desktop keeps history in its own store
    /// and reloads the chat index after a lifecycle event; the web client's
    /// host pushes it. A relay (`live_only`) app skips the reload too.
    fn refresh_index(&self, env: &Env<'_>, fx: &mut Vec<Effect>) {
        if !env.server_history {
            fx.push(Effect::RefreshSessionIndex);
        }
    }

    /// `dropSessionBookkeeping`: what the model keeps per session beside its
    /// transcript.
    pub(crate) fn drop_session_bookkeeping(&mut self, session_id: &str) {
        self.book.history_watermarks.remove(session_id);
        self.book.active_wire_turns.remove(session_id);
        self.forget_composer_session(session_id);
    }

    /// `dropSessionTranscriptState`: free the transcript maps of a session
    /// whose last tab closed, and forget its clamp logs.
    pub(crate) fn drop_session_transcript(&mut self, session_id: &str) {
        self.drop_session_bookkeeping(session_id);
        for call in self.tool_calls.get(session_id).into_iter().flatten() {
            let key = clamp::log_key(&js::string_of(call.get("toolCallId")));
            self.book.clamp_logged.retain(|logged| *logged != key);
        }
        self.messages.remove(session_id);
        self.tool_calls.remove(session_id);
        self.agent_switches.remove(session_id);
        self.commands.remove(session_id);
        self.commands_seq.remove(session_id);
        self.session_usage.remove(session_id);
        self.plans.remove(session_id);
    }

    /// `cancelRunningSubagents`: the agent process is gone, so a subagent it
    /// reports live, or a background launch that never ended, ends as
    /// `unexplained`.
    pub(crate) fn cancel_running_subagents(
        &mut self,
        session_ids: &[String],
        ended_at: f64,
        fx: &mut Vec<Effect>,
    ) {
        for session_id in session_ids {
            let Some(calls) = self.tool_calls.get_mut(session_id) else {
                continue;
            };
            let boundary = self.messages.get(session_id).and_then(|messages| {
                messages
                    .iter()
                    .rev()
                    .find(|message| message.role == super::types::Role::User)
                    .map(|message| Boundary {
                        seq: message.seq,
                        timestamp: Some(message.timestamp),
                    })
            });
            let background_ids = running_background(calls, boundary.as_ref());
            let mut count = 0;
            for call in calls.iter_mut() {
                let call_id = js::string_of(call.get("toolCallId"));
                let Some(record) = call.get_mut("subagent").filter(|r| r.is_object()) else {
                    continue;
                };
                if !is_active(record) {
                    continue;
                }
                if !live_progress(record) && !background_ids.contains(&call_id) {
                    continue;
                }
                count += 1;
                let ended = record
                    .get("endedAt")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .unwrap_or(json!(ended_at));
                record["activity"] = json!("idle");
                record["end"] = json!({ "reason": "unexplained" });
                record["endedAt"] = ended;
            }
            if count > 0 {
                fx.push(Effect::info(
                    "acp-store.subagentRecord",
                    format!(
                        "Cancelled {count} running subagents on chat {session_id}: the agent process is gone"
                    ),
                ));
            }
        }
    }

    /// `markResumeFailed`: a closed chat whose reopen failed says why, unless
    /// it already shows a failed resume or the Codex sign-in bar.
    pub(crate) fn mark_resume_failed(&mut self, session_id: &str, reason: &str) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        let last = session.last_error.as_deref();
        if session.status != SessionStatus::Closed
            || last.is_some_and(|last| last.starts_with(RESUME_FAILED_PREFIX))
            || last == Some(CODEX_CLI_SIGNED_OUT_MESSAGE)
        {
            return;
        }
        let message = SIGNED_OUT_TAGS
            .iter()
            .find_map(|tag| reason.strip_prefix(tag))
            .unwrap_or(reason);
        session.replaying = Some(None);
        session.last_error = Some(format!("{RESUME_FAILED_PREFIX}{message}"));
        session.last_error_code = Some(None);
        session.last_error_data = None;
    }
}
