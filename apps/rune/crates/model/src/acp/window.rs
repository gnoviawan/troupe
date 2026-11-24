//! The live window of a chat's transcript (`slices/transcript.ts`): the
//! bound on the messages and tool calls a session holds in memory, the
//! durability probe that lets the message bound engage, and the reader's
//! backfill allowance.
//!
//! A flush trims the sessions its streamed events touched. The model has no
//! flush: it trims the session of a chunk, tool call or tool update right
//! after the event, as the recorder's per-step flush does.

use serde_json::{Map, Value, json};

use super::diff_stat::content_diff_stat;
use super::types::Object;
use super::{Effect, Event, State};
use crate::js_trim;

/// Messages a session keeps live (`MAX_LIVE_WINDOW_MESSAGES`).
pub const MAX_LIVE_WINDOW_MESSAGES: usize = 300;
/// Tool calls a session keeps live, summaries aside (`MAX_LIVE_TOOL_CALLS`).
pub const MAX_LIVE_TOOL_CALLS: usize = 500;

const PATH_KEYS: [&str; 10] = [
    "path",
    "filePath",
    "file_path",
    "file",
    "target_file",
    "targetFile",
    "abspath",
    "absPath",
    "filename",
    "fileName",
];

/// Why the model asked for a session's durable payload.
#[derive(Clone, Debug, PartialEq)]
pub enum PayloadRead {
    /// The durability probe of the live window trim.
    Probe,
}

/// How a [`Effect::ReadSessionPayload`] ended: the payload, `None` when the
/// session has no durable history, or the read's error message.
pub type PayloadReadOutcome = Result<Option<Value>, String>;

impl Event {
    /// The session whose live window a flush would trim after this event:
    /// one a chunk, tool call or tool update coalesced into. A replaying
    /// session applies at once and is not trimmed.
    pub(crate) fn coalesced_session(&self, state: &State) -> Option<String> {
        let (session_id, covered) = match self {
            Self::MessageChunk(event, seq) => {
                let child = event
                    .parent_tool_call_id
                    .as_deref()
                    .is_some_and(|id| !id.is_empty());
                let covered = if child {
                    state.book.history_covered_tool(&event.session_id, *seq)
                } else {
                    state.book.history_covered(&event.session_id, *seq)
                };
                (&event.session_id, covered)
            }
            Self::ToolCall(event, seq) => (
                &event.session_id,
                state.book.history_covered_tool(&event.session_id, *seq),
            ),
            Self::ToolCallUpdate(event, seq) => (
                &event.session_id,
                state.book.history_covered_tool(&event.session_id, *seq),
            ),
            _ => return None,
        };
        let replaying = state
            .sessions
            .get(session_id)
            .is_some_and(|session| session.replaying().is_some());
        (!covered && !replaying).then(|| session_id.clone())
    }
}

impl State {
    /// The flush's trim of one session: its messages over the live bound,
    /// then its tool calls.
    pub(crate) fn trim_flushed(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        if self
            .messages
            .get(session_id)
            .is_some_and(|list| list.len() > MAX_LIVE_WINDOW_MESSAGES)
        {
            self.trim_live_window(session_id, fx);
        }
        if let Some(calls) = self.tool_calls.get_mut(session_id)
            && calls.len() > MAX_LIVE_TOOL_CALLS
        {
            trim_live_tool_calls(calls);
        }
    }

    /// `trimLiveWindow`: keep the newest messages, the streaming tail always.
    /// A session trims only once its history is known to be durable, so a
    /// dropped message can be paged back; the first time it goes over the
    /// bound, the model probes for that history instead.
    pub(crate) fn trim_live_window(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        let Some(len) = self.messages.get(session_id).map(Vec::len) else {
            return;
        };
        if len <= MAX_LIVE_WINDOW_MESSAGES {
            return;
        }
        if !self.book.durable_history.contains(session_id) {
            if self.book.untrimmable.contains(session_id)
                || !self.book.durability_probes.insert(session_id.to_owned())
            {
                return;
            }
            return fx.push(Effect::ReadSessionPayload {
                session_id: session_id.to_owned(),
                read: PayloadRead::Probe,
            });
        }
        let messages = self.messages.get_mut(session_id).expect("measured above");
        let streaming = messages
            .iter()
            .rev()
            .take_while(|message| message.streaming)
            .count();
        let backfill = self
            .book
            .backfill_counts
            .get(session_id)
            .copied()
            .unwrap_or(0);
        let keep = streaming.max(MAX_LIVE_WINDOW_MESSAGES + backfill);
        if keep < len {
            messages.drain(..len - keep);
        }
    }

    /// The answer to a [`Effect::ReadSessionPayload`].
    pub(crate) fn on_session_payload_read(
        &mut self,
        read: Effect,
        outcome: PayloadReadOutcome,
        fx: &mut Vec<Effect>,
    ) {
        let Effect::ReadSessionPayload { session_id, read } = read else {
            return;
        };
        match read {
            PayloadRead::Probe => self.on_durability_probe(session_id, outcome, fx),
        }
    }

    /// The probe answered. A close that dropped the transcript meanwhile
    /// wins: nothing is marked for a session that is gone.
    fn on_durability_probe(
        &mut self,
        session_id: String,
        outcome: PayloadReadOutcome,
        fx: &mut Vec<Effect>,
    ) {
        let live = self.messages.contains_key(&session_id);
        match outcome {
            Ok(Some(_)) if live => {
                self.book.durable_history.insert(session_id.clone());
            }
            Ok(None) if live => {
                self.book.untrimmable.insert(session_id.clone());
                fx.push(Effect::warn(
                    "acp.store",
                    format!(
                        "Live window trim skipped for session {session_id}: no durable history"
                    ),
                ));
            }
            Ok(_) => {}
            // A failed probe may run again on the next flush over the bound.
            Err(message) => fx.push(Effect::warn(
                "acp.store",
                format!("Durability probe failed for session {session_id}: Error: {message}"),
            )),
        }
        self.book.durability_probes.remove(&session_id);
    }

    /// `clearSessionBackfill`: the reader is back at the live edge, so the
    /// window drops what scrolling up paged in.
    pub(crate) fn clear_session_backfill(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        self.book.backfill_counts.remove(session_id);
        self.trim_live_window(session_id, fx);
    }
}

/// `trimLiveToolCalls`: drop the oldest finished calls over the bound. An
/// in-flight call and a subagent dispatch stay; a dropped file-changing call
/// leaves a summary, and summaries never count toward the bound.
pub(crate) fn trim_live_tool_calls(calls: &mut Vec<Object>) {
    let is_summary = |call: &Object| call.get("restoredSummary").is_some_and(super::js::truthy);
    let live = calls.iter().filter(|call| !is_summary(call)).count();
    if live <= MAX_LIVE_TOOL_CALLS {
        return;
    }
    let mut drop = live - MAX_LIVE_TOOL_CALLS;
    let mut survivors = Vec::with_capacity(calls.len());
    let mut live_survivors: usize = 0;
    for call in calls.drain(..) {
        if is_summary(&call) {
            survivors.push(call);
            continue;
        }
        let finished = matches!(
            call.get("status").and_then(Value::as_str),
            Some("completed" | "failed")
        );
        if drop > 0 && !call.contains_key("subagent") && finished {
            drop -= 1;
            if let Some(summary) = file_change_summary(&call) {
                survivors.push(summary);
            }
            continue;
        }
        survivors.push(call);
        live_survivors += 1;
    }
    // More in-flight calls than the bound: keep the newest.
    let mut excess = live_survivors.saturating_sub(MAX_LIVE_TOOL_CALLS);
    survivors.retain(|call| {
        if excess == 0 || is_summary(call) {
            return true;
        }
        excess -= 1;
        false
    });
    *calls = survivors;
}

/// `fileChangeSummary`: what the changed files list reads of a dropped
/// file-changing call, without its diff text.
fn file_change_summary(call: &Object) -> Option<Object> {
    let kind = call.get("kind").and_then(Value::as_str)?;
    if !matches!(kind, "edit" | "delete" | "move") {
        return None;
    }
    let paths = changed_paths(call);
    if paths.is_empty() {
        return None;
    }
    let mut summary = Map::new();
    summary.insert(
        "toolCallId".into(),
        call.get("toolCallId").cloned().unwrap_or(Value::Null),
    );
    summary.insert("kind".into(), json!(kind));
    let locations = paths.iter().map(|path| json!({ "path": path })).collect();
    summary.insert("locations".into(), Value::Array(locations));
    summary.insert("restoredSummary".into(), json!(true));
    for key in ["status", "seq", "timestamp"] {
        if let Some(value) = call.get(key) {
            summary.insert(key.into(), value.clone());
        }
    }
    if let Some(stat) = diff_stat(call, kind) {
        summary.insert("diffStat".into(), stat);
    }
    Some(summary)
}

/// `describeToolCall(call).diffStat`: an edit's line counts, from its diff
/// items or else the host's durable counts.
fn diff_stat(call: &Object, kind: &str) -> Option<Value> {
    if kind != "edit" || call.contains_key("subagentControl") {
        return None;
    }
    if let Some(counts) = content_diff_stat(call.get("content")) {
        return Some(json!({ "added": counts.added, "removed": counts.removed }));
    }
    let stored = call.get("diffStat")?.as_object()?;
    let count = |key: &str| stored.get(key).and_then(Value::as_u64);
    Some(json!({ "added": count("added")?, "removed": count("removed")? }))
}

/// `toolCallChangedPaths`: the call's path, each location, each diff item's
/// path; trimmed, deduped, in that order.
fn changed_paths(call: &Object) -> Vec<String> {
    let locations = call.get("locations").and_then(Value::as_array);
    let location_paths = locations
        .into_iter()
        .flatten()
        .map(|location| location.get("path").and_then(Value::as_str));
    let diff_paths = diff_items(call).map(|item| item.get("path").and_then(Value::as_str));
    let mut paths: Vec<String> = Vec::new();
    for raw in std::iter::once(call_path(call))
        .chain(location_paths)
        .chain(diff_paths)
    {
        let path = js_trim(raw.unwrap_or_default());
        if !path.is_empty() && !paths.iter().any(|known| known == path) {
            paths.push(path.to_owned());
        }
    }
    paths
}

fn diff_items(call: &Object) -> impl Iterator<Item = &Value> {
    call.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("diff"))
}

/// `toolCallPath`: the first location, a path field of the input, or the
/// first diff item's path.
fn call_path(call: &Object) -> Option<&str> {
    let first_location = call
        .get("locations")
        .and_then(Value::as_array)
        .and_then(|locations| locations.first())
        .and_then(|location| location.get("path"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty());
    if first_location.is_some() {
        return first_location;
    }
    if let Some(input) = call.get("rawInput").and_then(Value::as_object) {
        let from_input = PATH_KEYS.iter().find_map(|key| {
            input
                .get(*key)
                .and_then(Value::as_str)
                .map(js_trim)
                .filter(|path| !path.is_empty())
        });
        if from_input.is_some() {
            return from_input;
        }
    }
    diff_items(call)
        .filter_map(|item| item.get("path").and_then(Value::as_str))
        .find(|path| !path.is_empty())
}
