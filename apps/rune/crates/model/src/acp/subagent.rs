//! Subagents: the host's canonical record on a dispatch tool call, the
//! parent link on the calls a subagent makes, and the subagent's own text
//! kept on its dispatch call instead of in the main reply. Port of
//! `subagent-lifecycle.ts` (the record parse) and `subagent-transcript.ts`.

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::types::Object;
use super::{Book, Effect, js};

const SUBAGENT_META_KEY: &str = "termul/subagent";
const SUBAGENT_CONTROL_META_KEY: &str = "termul/subagentControl";
const MAX_PARENT_ID_CHARS: usize = 512;
const MAX_MESSAGE_ID_CHARS: usize = 256;
const MAX_SUBAGENT_TEXT_CHARS: usize = 8_000;
const MAX_SUBAGENT_MESSAGES: usize = 200;
/// The warn log and the orphan log remember this many keys.
const MAX_LOGGED_KEYS: usize = 256;

const SOURCES: [&str; 9] = [
    "acp-rfd",
    "acp-air",
    "claude-meta",
    "codex-collab",
    "cursor-task",
    "opencode-task",
    "gemini-agent",
    "tool-heuristic",
    "orchestrated",
];
const END_REASONS: [&str; 4] = ["completed", "cancelled", "failed", "unexplained"];
const TRANSCRIPTS: [&str; 3] = ["live", "final", "none"];
const WORKSPACE_MODES: [&str; 2] = ["shared", "worktree"];
const USAGE_FIELDS: [&str; 5] = [
    "inputTokens",
    "outputTokens",
    "totalTokens",
    "toolUses",
    "durationMs",
];

/// The host's record of one subagent (`@shared/types/subagent`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subagent {
    pub id: String,
    pub root_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub dispatch_tool_call_id: String,
    pub source: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub background: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<SubagentAgent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<SubagentWorkspace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
    pub caps: SubagentCaps,
    #[serde(flatten)]
    pub state: SubagentState,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentAgent {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    pub agent_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentWorkspace {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_stat: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCaps {
    /// `null` or `session-cancel`.
    pub stop: Option<String>,
    pub transcript: String,
    pub live_progress: bool,
    pub usage: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SubagentEnd {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "activity", rename_all = "snake_case")]
pub enum SubagentState {
    Idle { end: SubagentEnd },
    Running,
    RequiresAction,
    Unknown,
}

/// A malformed record: names the bad field, never its value.
type Parsed<T> = Result<T, String>;

/// `asRecord`: any non-null object, arrays included, reads as a record.
fn is_record(value: &Value) -> bool {
    value.is_object() || value.is_array()
}

fn record(value: Option<&Value>, field: &str) -> Parsed<Value> {
    value
        .filter(|value| is_record(value))
        .cloned()
        .ok_or_else(|| field.to_owned())
}

fn required_string(raw: &Value, field: &str) -> Parsed<String> {
    match raw.get(field) {
        Some(Value::String(text)) if !text.is_empty() => Ok(text.clone()),
        _ => Err(field.to_owned()),
    }
}

fn optional_string(raw: &Value, field: &str) -> Parsed<Option<String>> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        _ => Err(field.to_owned()),
    }
}

fn optional_number(raw: &Value, field: &str) -> Parsed<Option<f64>> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        value => js::finite(value)
            .filter(|n| *n >= 0.0)
            .map(Some)
            .ok_or_else(|| field.to_owned()),
    }
}

fn one_of(set: &[&str], value: Option<&Value>, field: &str) -> Parsed<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| set.contains(value))
        .map(str::to_owned)
        .ok_or_else(|| field.to_owned())
}

fn parse_state(raw: &Value) -> Parsed<SubagentState> {
    let activity = raw.get("activity").and_then(Value::as_str);
    if activity == Some("idle") {
        let end = record(raw.get("end"), "end")?;
        let error = optional_string(&end, "error")?;
        return Ok(SubagentState::Idle {
            end: SubagentEnd {
                reason: one_of(&END_REASONS, end.get("reason"), "end.reason")?,
                error,
            },
        });
    }
    if raw.get("end").is_some_and(|end| !end.is_null()) {
        return Err("end".into());
    }
    match activity {
        Some("running") => Ok(SubagentState::Running),
        Some("requires_action") => Ok(SubagentState::RequiresAction),
        Some("unknown") => Ok(SubagentState::Unknown),
        _ => Err("activity".into()),
    }
}

fn parse_caps(value: Option<&Value>) -> Parsed<SubagentCaps> {
    let raw = record(value, "caps")?;
    let stop = match raw.get("stop") {
        Some(Value::Null) => None,
        Some(Value::String(stop)) if stop == "session-cancel" => Some(stop.clone()),
        _ => return Err("caps.stop".into()),
    };
    let live_progress = raw
        .get("liveProgress")
        .and_then(Value::as_bool)
        .ok_or("caps.liveProgress")?;
    let usage = raw
        .get("usage")
        .and_then(Value::as_bool)
        .ok_or("caps.usage")?;
    Ok(SubagentCaps {
        stop,
        transcript: one_of(&TRANSCRIPTS, raw.get("transcript"), "caps.transcript")?,
        live_progress,
        usage,
    })
}

fn parse_usage(value: Option<&Value>) -> Parsed<Option<Map<String, Value>>> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let raw = record(Some(value), "usage")?;
    let mut usage = Map::new();
    for field in USAGE_FIELDS {
        if let Some(n) = optional_number(&raw, field)? {
            usage.insert(field.to_owned(), json!(n));
        }
    }
    Ok(Some(usage))
}

fn parse_agent(value: &Value) -> Parsed<SubagentAgent> {
    let raw = record(Some(value), "agent")?;
    Ok(SubagentAgent {
        name: required_string(&raw, "name")?,
        config_id: optional_string(&raw, "configId")?,
        agent_id: required_string(&raw, "agentId")?,
    })
}

fn parse_workspace(value: &Value) -> Parsed<SubagentWorkspace> {
    let raw = record(Some(value), "workspace")?;
    Ok(SubagentWorkspace {
        mode: one_of(&WORKSPACE_MODES, raw.get("mode"), "mode")?,
        path: optional_string(&raw, "path")?,
        branch: optional_string(&raw, "branch")?,
        base: optional_string(&raw, "base")?,
        diff_stat: optional_string(&raw, "diffStat")?,
    })
}

/// Parses records and remembers which malformed ones it already logged.
pub(crate) struct Reader<'a> {
    pub book: &'a mut Book,
    pub fx: &'a mut Vec<Effect>,
}

impl Reader<'_> {
    /// `warnOnce`: one warn per call id and key, the oldest key forgotten
    /// past the cap.
    fn warn_once(&mut self, tool_call_id: &str, key: &str, field: &str) {
        let id = js::head(tool_call_id, 128);
        let dedup = format!("{id}:{key}");
        if self.book.subagent_warned.contains(&dedup) {
            return;
        }
        if self.book.subagent_warned.len() >= MAX_LOGGED_KEYS {
            self.book.subagent_warned.pop_front();
        }
        self.book.subagent_warned.push_back(dedup);
        self.fx.push(Effect::warn(
            "acp-store.subagentRecord",
            format!("Dropped malformed {key} on tool call {id}: bad {field}"),
        ));
    }

    /// An optional object field with its own parse: a malformed one drops
    /// the field, never the record it rides.
    fn optional_field<T>(
        &mut self,
        tool_call_id: &str,
        field: &str,
        value: Option<&Value>,
        parse: fn(&Value) -> Parsed<T>,
    ) -> Option<T> {
        let value = value.filter(|value| !value.is_null())?;
        match parse(value) {
            Ok(parsed) => Some(parsed),
            Err(bad) => {
                self.warn_once(tool_call_id, &format!("{SUBAGENT_META_KEY}.{field}"), &bad);
                None
            }
        }
    }

    fn record(&mut self, tool_call_id: &str, value: &Value) -> Parsed<Subagent> {
        let raw = record(Some(value), "record")?;
        let background = raw
            .get("background")
            .and_then(Value::as_bool)
            .ok_or("background")?;
        let id = required_string(&raw, "id")?;
        let root_session_id = required_string(&raw, "rootSessionId")?;
        let parent_id = optional_string(&raw, "parentId")?;
        let dispatch_tool_call_id = required_string(&raw, "dispatchToolCallId")?;
        let source = one_of(&SOURCES, raw.get("source"), "source")?;
        let title = required_string(&raw, "title")?;
        let description = optional_string(&raw, "description")?;
        let role = optional_string(&raw, "role")?;
        let prompt = optional_string(&raw, "prompt")?;
        let started_at = optional_number(&raw, "startedAt")?;
        let ended_at = optional_number(&raw, "endedAt")?;
        let model = optional_string(&raw, "model")?;
        let usage = parse_usage(raw.get("usage"))?;
        let agent = self.optional_field(tool_call_id, "agent", raw.get("agent"), parse_agent);
        let workspace = self.optional_field(
            tool_call_id,
            "workspace",
            raw.get("workspace"),
            parse_workspace,
        );
        let output_file = optional_string(&raw, "outputFile")?;
        let caps = parse_caps(raw.get("caps"))?;
        let state = parse_state(&raw)?;
        Ok(Subagent {
            id,
            root_session_id,
            parent_id,
            dispatch_tool_call_id,
            source,
            title,
            description,
            role,
            prompt,
            background,
            started_at,
            ended_at,
            model,
            usage,
            agent,
            workspace,
            output_file,
            caps,
            state,
        })
    }

    /// `parseMetaEntry`: one `_meta` entry, `None` when absent or malformed.
    fn meta_entry<T>(
        &mut self,
        tool_call_id: &str,
        meta: Option<&Value>,
        key: &str,
        parse: impl FnOnce(&mut Self, &Value) -> Parsed<T>,
    ) -> Option<T> {
        let value = meta
            .filter(|meta| is_record(meta))
            .and_then(|meta| meta.get(key))
            .filter(|value| !value.is_null())?;
        match parse(self, value) {
            Ok(parsed) => Some(parsed),
            Err(bad) => {
                self.warn_once(tool_call_id, key, &bad);
                None
            }
        }
    }

    /// `parseSubagent`: the record a tool call or update carries, if valid.
    pub(crate) fn subagent(&mut self, call: &Object) -> Option<Subagent> {
        let id = js::string_of(call.get("toolCallId"));
        self.meta_entry(
            &id,
            call.get("_meta"),
            SUBAGENT_META_KEY,
            |reader, value| reader.record(&id, value),
        )
    }

    fn control(&mut self, call: &Object) -> Option<Value> {
        let id = js::string_of(call.get("toolCallId"));
        self.meta_entry(
            &id,
            call.get("_meta"),
            SUBAGENT_CONTROL_META_KEY,
            |_, value| {
                let ids = value
                    .get("subagentIds")
                    .filter(|_| is_record(value))
                    .and_then(Value::as_array)
                    .ok_or("subagentIds")?;
                let ids = ids
                    .iter()
                    .map(|id| match id {
                        Value::String(id) if !id.is_empty() => Ok(json!(id)),
                        _ => Err("subagentIds".to_owned()),
                    })
                    .collect::<Parsed<Vec<Value>>>()?;
                Ok(json!({ "subagentIds": ids }))
            },
        )
    }

    /// `withSubagentIngest`: lift the host's record and control marker, the
    /// parent dispatch, and the wire subagent messages onto the call.
    pub(crate) fn ingest(&mut self, mut call: Object) -> Object {
        if let Some(subagent) = self.subagent(&call) {
            call.insert("subagent".into(), json!(subagent));
        }
        if let Some(control) = self.control(&call) {
            call.insert("subagentControl".into(), control);
        }
        let parent = call
            .get("_meta")
            .filter(|meta| is_record(meta))
            .and_then(|meta| meta.get("claudeCode"))
            .filter(|code| is_record(code))
            .and_then(|code| code.get("parentToolUseId"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && js::len(id) <= MAX_PARENT_ID_CHARS)
            .map(str::to_owned);
        if let Some(parent) = parent
            && call.get("toolCallId").and_then(Value::as_str) != Some(parent.as_str())
        {
            call.insert("parentToolCallId".into(), json!(parent));
        }
        if let Some(raw) = call.shift_remove("subagentMessages")
            && let Some(parsed) = parse_messages(&raw)
        {
            call.insert("subagentMessages".into(), parsed);
        }
        call
    }
}

/// `boundedMessageId`: counted in code points, as the host counts.
fn bounded_message_id(id: Option<&Value>) -> Option<String> {
    let id = id?.as_str()?;
    let fits = !id.is_empty()
        && js::len(id) <= 2 * MAX_MESSAGE_ID_CHARS
        && id.chars().count() <= MAX_MESSAGE_ID_CHARS;
    fits.then(|| id.to_owned())
}

fn message(seq: f64, timestamp: f64, text: &str, message_id: Option<String>) -> Value {
    let mut message = json!({ "seq": seq, "timestamp": timestamp, "text": text });
    if let Some(id) = message_id {
        message["messageId"] = json!(id);
    }
    message
}

/// `parseSubagentMessages`: well-formed entries only, within the bounds.
fn parse_messages(value: &Value) -> Option<Value> {
    let messages: Vec<Value> = value
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let seq = js::finite(entry.get("seq"))?;
            let timestamp = js::finite(entry.get("timestamp"))?;
            let text = entry.get("text")?.as_str()?;
            Some(message(
                seq,
                timestamp,
                text,
                bounded_message_id(entry.get("messageId")),
            ))
        })
        .collect();
    (!messages.is_empty()).then(|| Value::Array(bound_messages(messages)))
}

/// `boundSubagentMessages`: the newest messages within both bounds; the
/// oldest kept one loses its head.
fn bound_messages(messages: Vec<Value>) -> Vec<Value> {
    let skip = messages.len().saturating_sub(MAX_SUBAGENT_MESSAGES);
    let mut kept: Vec<Value> = messages.into_iter().skip(skip).collect();
    let mut budget = MAX_SUBAGENT_TEXT_CHARS;
    for i in (0..kept.len()).rev() {
        let text = kept[i]["text"].as_str().unwrap_or_default();
        let length = js::len(text);
        if length >= budget {
            if length > budget {
                let tail = js::tail(text, budget).to_owned();
                kept[i]["text"] = json!(tail);
            }
            return kept.split_off(i);
        }
        budget -= length;
    }
    kept
}

/// `subagentChunkText`: the text an agent text chunk adds.
pub(crate) fn chunk_text(role_is_agent: bool, content: &Value) -> Option<&str> {
    if !role_is_agent || content.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    content
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

/// `withSubagentText`: add a chunk to the dispatch call's messages. It opens
/// a new message on a new ACP message id, or when one of the subagent's
/// calls arrived after the last message started.
pub(crate) fn with_text(
    call: &mut Object,
    text: &str,
    stamp: (f64, f64, Option<&Value>),
    calls: &[Object],
) {
    let (seq, timestamp, message_id) = stamp;
    let mut messages = call
        .get("subagentMessages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let message_id = bounded_message_id(message_id);
    let own_id = call.get("toolCallId");
    let opens = match messages.last() {
        None => true,
        Some(last) => {
            let last_seq = last["seq"].as_f64().unwrap_or(f64::NAN);
            message_id
                .as_ref()
                .is_some_and(|id| last.get("messageId").and_then(Value::as_str) != Some(id))
                || calls.iter().any(|tc| {
                    tc.get("parentToolCallId").is_some()
                        && tc.get("parentToolCallId") == own_id
                        && tc
                            .get("seq")
                            .and_then(Value::as_f64)
                            .is_some_and(|s| s > last_seq)
                })
        }
    };
    if opens {
        messages.push(message(seq, timestamp, text, message_id));
    } else if let Some(last) = messages.last_mut() {
        let joined = format!("{}{text}", last["text"].as_str().unwrap_or_default());
        last["text"] = json!(joined);
    }
    call.insert(
        "subagentMessages".into(),
        Value::Array(bound_messages(messages)),
    );
}

/// Log once per parent id that a chunk's dispatch call is unknown.
pub(crate) fn log_orphan(book: &mut Book, fx: &mut Vec<Effect>, parent: &str, session_id: &str) {
    let key = js::head(parent, 128).to_owned();
    if book.orphans_logged.contains(&key) {
        return;
    }
    if book.orphans_logged.len() >= MAX_LOGGED_KEYS {
        book.orphans_logged.clear();
    }
    book.orphans_logged.insert(key.clone());
    fx.push(Effect::info(
        "acp-store.subagentMessages",
        format!("Dropped subagent text for unknown dispatch {key} in session {session_id}"),
    ));
}
