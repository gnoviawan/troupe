//! The first half of a history open: the chat record and its transcript
//! from the durable payload (`installableTranscript`, the restored controls,
//! the open turn, the last turn's error and the plan), before any agent
//! work.

use serde_json::{Value, json};

use super::host_error::HostError;
use super::subagent::Reader;
use super::transcript::HistoryPayload;
use super::types::{ChatMessage, LogPoint, Object, PlanEntry, Role, Session, SessionStatus};
use super::{Effect, State, js};

/// What the install learned for the rest of the open.
pub(crate) struct Installed {
    /// The payload says a turn may still run (`turnActive`, or an active chat
    /// ending on an unanswered prompt).
    pub turn_live: bool,
    /// The restored last-turn error stays once the chat is live again.
    pub keep_error: bool,
}

/// `projectTranscript` of a head-anchored payload.
pub(crate) struct Projected {
    pub messages: Vec<ChatMessage>,
    pub tool_calls: Vec<Object>,
    pub switches: Vec<Value>,
}

/// `hasVisibleContent`.
fn has_visible_content(message: &ChatMessage) -> bool {
    message.blocks.iter().any(|block| {
        block.get("type").and_then(Value::as_str) != Some("text")
            || !crate::js_trim(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .is_empty()
    })
}

fn boundary(message: &ChatMessage) -> bool {
    message.role == Role::User && message.rest.get("handoffBoundary") == Some(&Value::Bool(true))
}

/// `partitionTranscriptTurns`: the visible messages and the seq intervals
/// of the hidden turns, whose tool cards do not render either.
fn partition(messages: Vec<ChatMessage>) -> (Vec<ChatMessage>, Vec<(f64, f64)>) {
    let mut visible: Vec<ChatMessage> = Vec::new();
    let mut hidden = Vec::new();
    let mut hidden_turn = false;
    let mut start: Option<f64> = None;
    for message in messages {
        if boundary(&message) {
            hidden_turn = false;
            if let (Some(open), Some(seq)) = (start, message.seq) {
                hidden.push((open, seq));
            }
            start = None;
            continue;
        }
        if message.role == Role::User {
            hidden_turn = !has_visible_content(&message);
            if !hidden_turn {
                if let (Some(open), Some(seq)) = (start, message.seq) {
                    hidden.push((open, seq));
                }
                start = None;
                visible.push(message);
                continue;
            }
        } else if !hidden_turn {
            visible.push(message);
            continue;
        }
        if start.is_none()
            && let Some(seq) = message.seq
        {
            start = Some(if visible.is_empty() && hidden.is_empty() {
                0.0
            } else {
                seq
            });
        }
    }
    if let Some(open) = start {
        hidden.push((open, f64::INFINITY));
    }
    (visible, hidden)
}

/// `maxPayloadSeq` over the messages and tool calls.
pub(crate) fn max_payload_seq(payload: &Value) -> f64 {
    let seqs = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| js::finite(item.get("seq")))
            .collect::<Vec<_>>()
    };
    seqs("messages")
        .into_iter()
        .chain(seqs("toolCalls"))
        .chain(seqs("switches"))
        .fold(0.0, f64::max)
}

/// `openTurnPromptSeq`: the host seq of the prompt that opened the
/// trailing turn, when it is a real prompt.
pub(crate) fn open_turn_prompt_seq(messages: &[ChatMessage]) -> Option<f64> {
    let last = messages.iter().rev().find(|m| m.role == Role::User)?;
    let prompt = !boundary(last) && has_visible_content(last);
    last.seq
        .filter(|seq| prompt && seq.is_finite() && *seq > 0.0)
}

/// `deriveOpenTurn`: the turn a trailing unanswered prompt leaves open.
fn derive_open_turn(messages: &[ChatMessage], turn_active: Option<bool>) -> Option<String> {
    if turn_active == Some(false) {
        return None;
    }
    let (index, last) = messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| m.role == Role::User)?;
    if turn_active != Some(true) && messages[index + 1..].iter().any(|m| m.role == Role::Agent) {
        return None;
    }
    Some(if last.id.starts_with("turn:") {
        last.id.clone()
    } else if let Some(seq) = last.seq {
        format!("turn:seq-{}", js::string_of(Some(&json!(seq))))
    } else {
        format!("turn:open-{index}")
    })
}

/// The last ```` ```termul-plan ```` fence's JSON in the text
/// (`extractTermulPlanFenceJson`).
fn plan_fence_json(text: &str) -> Option<&str> {
    let mut last = None;
    let mut rest = text;
    while let Some(at) = rest.find("```termul-plan") {
        let after = &rest[at + "```termul-plan".len()..];
        let Some(body) = after
            .strip_prefix("\r\n")
            .or_else(|| after.strip_prefix('\n'))
        else {
            rest = after;
            continue;
        };
        let Some(end) = body.find("\n```") else {
            break;
        };
        let json = &body[..end];
        last = Some(json.strip_suffix('\r').unwrap_or(json));
        rest = &body[end + "\n```".len()..];
    }
    last
}

/// `parseTermulPlanFence`: the entries with a string `content`, and a
/// string `status` and `priority` when present.
fn parse_plan_fence(text: &str) -> Option<Vec<PlanEntry>> {
    let parsed: Value = serde_json::from_str(plan_fence_json(text)?).ok()?;
    let optional_text = |entry: &Object, key: &str| entry.get(key).is_none_or(Value::is_string);
    Some(
        parsed
            .as_array()?
            .iter()
            .filter_map(Value::as_object)
            .filter(|entry| {
                entry.get("content").is_some_and(Value::is_string)
                    && optional_text(entry, "status")
                    && optional_text(entry, "priority")
            })
            .map(|entry| PlanEntry(entry.clone()))
            .collect(),
    )
}

fn text_blocks(message: &ChatMessage) -> impl DoubleEndedIterator<Item = &str> {
    message
        .blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
        })
}

/// `scanPlanFenceFromMessages`: the newest fence of any agent message.
fn scan_plan_fence(messages: &[ChatMessage]) -> Option<Vec<PlanEntry>> {
    let fenced = messages
        .iter()
        .rev()
        .filter(|m| m.role == Role::Agent)
        .find_map(|m| {
            text_blocks(m)
                .rev()
                .find(|text| plan_fence_json(text).is_some())
        })?;
    parse_plan_fence(fenced)
}

/// `projectTranscript` for a payload that starts at the conversation head:
/// hidden turns and their tool cards dropped, user bubbles normalized.
pub(crate) fn project_transcript(
    state: &mut State,
    session_id: &str,
    payload: &Value,
) -> Option<Projected> {
    let history: HistoryPayload = serde_json::from_value(payload.clone()).ok()?;
    state.note_history_watermark(session_id, &history);
    let messages: Vec<ChatMessage> =
        serde_json::from_value(payload.get("messages").cloned().unwrap_or(json!([]))).ok()?;
    let (mut visible, hidden) = partition(messages);
    for message in &mut visible {
        super::turn::normalize_user_message(message);
    }
    let mut fx = Vec::new();
    let mut calls: Vec<Object> = payload
        .get("toolCalls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|call| {
            call.get("toolCallId")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
        })
        .map(|call| {
            Reader {
                book: &mut state.book,
                fx: &mut fx,
            }
            .ingest(call.clone())
        })
        .collect();
    if visible.is_empty() {
        calls.clear();
    } else {
        calls.retain(|call| {
            js::finite(call.get("seq")).is_none_or(|seq| {
                !hidden
                    .iter()
                    .any(|(start, end)| seq >= *start && seq < *end)
            })
        });
    }
    let switches = payload
        .get("switches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| {
            js::finite(record.get("seq")).is_some()
                && record
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty())
        })
        .cloned()
        .collect();
    Some(Projected {
        messages: visible,
        tool_calls: calls,
        switches,
    })
}

/// `resumeMissesSession`: `session/resume` failed because the agent lacks
/// the session, not for a sign-in, a running turn or a timeout.
pub(crate) fn resume_misses_session(error: &HostError) -> bool {
    if let Some(code) = error.code.as_deref().map(str::to_ascii_lowercase) {
        if matches!(code.as_str(), "timeout" | "closed" | "agent_crashed")
            || code.contains("auth")
            || code.contains("turn_active")
        {
            return false;
        }
        if matches!(code.as_str(), "not_found" | "session_not_found") {
            return true;
        }
    }
    let message = error.message.to_lowercase();
    if [
        "acp_auth_required",
        "acp_reopen_turn_active",
        "timed out",
        "timeout",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        return false;
    }
    [
        "session not found",
        "no such session",
        "unknown session",
        "session does not exist",
        "session_not_found",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

impl State {
    /// Install the payload's chat: the record (closed until the agent is
    /// back), its transcript, tool cards, switch markers, commands and plan.
    pub(crate) fn install_history(
        &mut self,
        session_id: &str,
        payload: &Value,
        fx: &mut Vec<Effect>,
    ) -> Option<Installed> {
        let meta = payload.get("metadata")?.clone();
        let controls = self.reopen_controls(session_id, payload, fx);
        let projected = project_transcript(self, session_id, payload)?;
        let turn_active = meta.get("turnActive").and_then(Value::as_bool);
        let open_turn = derive_open_turn(&projected.messages, turn_active);
        let turn_live = turn_active == Some(true)
            || (meta.get("status").and_then(Value::as_str) == Some("active")
                && open_turn.is_some());
        let (last_error, last_error_code) = restored_turn_error(meta.get("lastTurnError"));
        let keep_error = last_error.is_some();
        let title = meta.get("title").and_then(Value::as_str);
        let title = self.prefer_index_title(title, session_id);
        let mut rest = Object::new();
        for key in ["cwd", "projectId"] {
            if let Some(value) = meta.get(key) {
                rest.insert(key.into(), value.clone());
            }
        }
        for (key, value) in ["modes", "models", "configOptions"]
            .into_iter()
            .zip(controls)
        {
            rest.insert(key.into(), value);
        }
        for key in ["createdAt", "worktreePath", "worktreeBranch", "kind"] {
            if let Some(value) = meta.get(key) {
                rest.insert(key.into(), value.clone());
            }
        }
        let mut session = Session {
            id: session_id.to_owned(),
            agent_id: js::string_of(meta.get("agentId")),
            status: SessionStatus::Closed,
            title,
            active_turn: turn_live,
            open_turn_id: open_turn.filter(|_| turn_live),
            agent_working: None,
            replaying: Some(None),
            opencode_notice: None,
            last_error: Some(last_error),
            last_error_code: Some(last_error_code),
            last_error_data: None,
            background_commands: None,
            rest,
        };
        if session
            .rest
            .get("models")
            .is_some_and(|models| !models.is_null())
        {
            super::controls::align_model_projection(&mut session);
        }
        self.sessions.insert(session_id.to_owned(), session);
        let messages = projected.messages;
        let plan = (!self.plans.contains_key(session_id)).then(|| scan_plan_fence(&messages));
        let last_agent_fenced = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Agent)
            .is_some_and(|m| text_blocks(m).any(|text| plan_fence_json(text).is_some()));
        self.messages.insert(session_id.to_owned(), messages);
        let mut calls = projected.tool_calls;
        super::window::trim_live_tool_calls(&mut calls);
        self.tool_calls.insert(session_id.to_owned(), calls);
        // `healChainEdges`: this chat's own edge, and the edges its markers
        // name.
        if let Some(source) = meta.get("continuedFrom").and_then(Value::as_str) {
            self.stamp_switch_continuation(session_id, source);
        }
        for record in &projected.switches {
            if let Some(target) = record
                .get("newSessionId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                self.stamp_switch_continuation(target, session_id);
            }
        }
        self.agent_switches
            .insert(session_id.to_owned(), projected.switches);
        if let Some(list) = payload.get("commands").and_then(Value::as_array) {
            let point = js::finite(payload.get("commandsSeq")).map(|seq| LogPoint {
                log_epoch: meta
                    .get("logEpoch")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                seq: seq as u64,
            });
            self.install_commands(session_id, list.clone(), point);
        }
        self.trim_live_window(session_id, fx);
        match plan {
            Some(Some(entries)) if !entries.is_empty() => {
                let entries = if turn_live {
                    entries
                } else {
                    super::turn::settle_running_plan(&entries).unwrap_or(entries)
                };
                self.plans.insert(session_id.to_owned(), entries);
            }
            Some(_) if last_agent_fenced => fx.push(Effect::warn(
                "planRehydrate",
                format!(
                    "Malformed termul-plan fence in history session {session_id}; leaving plans empty"
                ),
            )),
            _ => {}
        }
        Some(Installed {
            turn_live,
            keep_error,
        })
    }

    /// `withCommands`: a list older than the one held is skipped.
    fn install_commands(&mut self, session_id: &str, list: Vec<Value>, point: Option<LogPoint>) {
        if let (Some(point), Some(held)) = (&point, self.commands_seq.get(session_id))
            && !point.is_after(held)
            && point.seq < held.seq
        {
            return;
        }
        self.commands.insert(session_id.to_owned(), list);
        match point {
            Some(point) => {
                self.commands_seq.insert(session_id.to_owned(), point);
            }
            None => {
                self.commands_seq.remove(session_id);
            }
        }
    }

    /// `preferIndexTitle`: a placeholder gives way to a real index title.
    fn prefer_index_title(&self, title: Option<&str>, session_id: &str) -> Option<String> {
        if !super::index::is_untitled(title) {
            return title.map(str::to_owned);
        }
        let indexed = self
            .session_index
            .iter()
            .find(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id))
            .and_then(|entry| entry.get("title"))
            .and_then(Value::as_str)
            .filter(|title| !title.is_empty() && !super::index::is_untitled(Some(title)));
        indexed.or(title).map(str::to_owned)
    }

    /// `resolveReopenControls`: each control the client holds wins; the
    /// payload fills the others.
    fn reopen_controls(
        &self,
        session_id: &str,
        payload: &Value,
        fx: &mut Vec<Effect>,
    ) -> [Value; 3] {
        let held = self.sessions.get(session_id);
        let held_field = |key: &str| {
            held.and_then(|session| session.rest.get(key))
                .filter(|value| !value.is_null())
                .cloned()
        };
        let held_options = held_field("configOptions")
            .and_then(|options| options.as_array().cloned())
            .unwrap_or_default();
        let from_payload = |key: &str| match held_field(key) {
            Some(_) => None,
            None => payload.get(key).filter(|value| !value.is_null()).cloned(),
        };
        let modes = from_payload("modes");
        let models = from_payload("models");
        let options = if held_options.is_empty() {
            payload
                .get("configOptions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let controls = [
            held_field("modes").or(modes.clone()).unwrap_or(Value::Null),
            held_field("models")
                .or(models.clone())
                .unwrap_or(Value::Null),
            Value::Array(if options.is_empty() {
                held_options
            } else {
                options.clone()
            }),
        ];
        if modes.is_some() || models.is_some() || !options.is_empty() {
            let count = |value: &Option<Value>, key: &str| {
                value
                    .as_ref()
                    .and_then(|value| value.get(key))
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            };
            let seq = payload
                .get("controlsSeq")
                .filter(|seq| !seq.is_null())
                .map_or_else(|| "unknown".to_owned(), |seq| js::string_of(Some(seq)));
            fx.push(Effect::info(
                "acp.reopenControls",
                format!(
                    "Restored controls for session {session_id} from the history payload (seq {seq}: {} config options, {} models, {} modes)",
                    options.len(),
                    count(&models, "availableModels"),
                    count(&modes, "availableModes"),
                ),
            ));
        }
        controls
    }

    /// `clearDeadTurn`: the turn cannot finish.
    pub(crate) fn clear_dead_turn(&mut self, session_id: &str) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        session.active_turn = false;
        session.open_turn_id = None;
        if let Some(plan) = self
            .plans
            .get(session_id)
            .and_then(|plan| super::turn::settle_running_plan(plan))
        {
            self.plans.insert(session_id.to_owned(), plan);
        }
    }

    /// Put the payload's transcript back after a failed load.
    pub(crate) fn restore_history_transcript(
        &mut self,
        session_id: &str,
        payload: &Value,
        fx: &mut Vec<Effect>,
    ) {
        let Some(projected) = project_transcript(self, session_id, payload) else {
            return;
        };
        self.messages
            .insert(session_id.to_owned(), projected.messages);
        let mut calls = projected.tool_calls;
        super::window::trim_live_tool_calls(&mut calls);
        self.tool_calls.insert(session_id.to_owned(), calls);
        self.agent_switches
            .insert(session_id.to_owned(), projected.switches);
        self.trim_live_window(session_id, fx);
    }
}

/// `lastErrorFromPersistedTurn`: the banner of the last turn's failure.
fn restored_turn_error(turn: Option<&Value>) -> (Option<String>, Option<f64>) {
    let Some(turn) = turn.filter(|turn| turn.is_object()) else {
        return (None, None);
    };
    let message = turn
        .get("message")
        .filter(|message| js::truthy(message))
        .map(|message| js::string_of(Some(message)))
        .or_else(|| {
            turn.get("stopReason")
                .and_then(Value::as_str)
                .and_then(super::turn::note_for_stop_reason)
        });
    let Some(message) = message else {
        return (None, None);
    };
    let code = js::finite(turn.get("code")).filter(|code| code.fract() == 0.0);
    let provider = turn.get("providerCode").and_then(Value::as_str);
    let shown = super::agents::describe_provider_failure(provider, &message).unwrap_or(message);
    (Some(shown), code)
}
