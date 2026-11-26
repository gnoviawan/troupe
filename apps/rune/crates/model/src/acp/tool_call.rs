//! Tool call events: upsert a call by id, merge an update into it, carry the
//! Codex shell stream, and fold plan tools into the session's plan. Port of
//! `_onToolCall` and `_onToolCallUpdate` in `slices/transcript.ts` and of
//! `components/chat/terminal-output.ts`.

use serde_json::{Value, json};

use super::subagent::Reader;
use super::types::Object;
use super::{Effect, Env, State, ToolCallEvent, ToolCallUpdateEvent, clamp, js, plan};

const TERMINAL_OUTPUT_CHARS: usize = 64 * 1024;
const TERMINAL_MARKER: &str = "\n[termul: tool call content truncated]";

/// `applyTerminalStream` then `boundTerminalStream`: the stored call's shell
/// output with this event's chunk appended, and the exit code.
fn terminal_stream(previous: Option<&Object>, incoming: &Object) -> Object {
    let mut output = previous
        .and_then(|prior| prior.get("terminalOutput"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut exit_code = previous
        .and_then(|prior| prior.get("terminalExitCode"))
        .filter(|code| code.is_number())
        .cloned();
    let meta = incoming.get("_meta").filter(|meta| meta.is_object());
    let delta = meta
        .and_then(|meta| meta.get("terminal_output_delta"))
        .and_then(|delta| delta.get("data"))
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty());
    if let Some(data) = delta {
        let room = TERMINAL_OUTPUT_CHARS.saturating_sub(js::len(&output));
        if room > 0 {
            if js::len(data) <= room {
                output.push_str(data);
            } else {
                let keep = room.saturating_sub(js::len(TERMINAL_MARKER));
                output.push_str(js::head(data, keep));
                output.push_str(TERMINAL_MARKER);
            }
        }
    }
    let exit = meta
        .and_then(|meta| meta.get("terminal_exit"))
        .and_then(|exit| exit.get("exit_code"));
    if js::finite(exit).is_some() {
        exit_code = exit.cloned();
    }
    let mut stream = Object::new();
    if !output.is_empty() {
        stream.insert("terminalOutput".into(), json!(clamp::field_text(&output)));
    }
    if let Some(code) = exit_code {
        stream.insert("terminalExitCode".into(), code);
    }
    stream
}

/// `{ ...base, ...patch }`.
fn spread(mut base: Object, patch: Object) -> Object {
    for (key, value) in patch {
        base.insert(key, value);
    }
    base
}

fn seq_of(call: &Object) -> Option<&Value> {
    call.get("seq").filter(|seq| seq.is_number())
}

fn timestamp_of(call: &Object) -> Option<&Value> {
    call.get("timestamp").filter(|ts| ts.is_number())
}

impl State {
    /// `planPatch`: fold a plan tool into the session's plan and log a
    /// replacement or a clear.
    fn fold_plan_tool(&mut self, session_id: &str, tool: &Object, fx: &mut Vec<Effect>) {
        let Some((entries, change)) = plan::after_tool_call(self.plans.get(session_id), tool)
        else {
            return;
        };
        match entries {
            Some(entries) => {
                let count = entries.len();
                self.plans.insert(session_id.to_owned(), entries);
                if change == plan::Change::New {
                    fx.push(Effect::info(
                        "planSession",
                        format!("Replaced the plan for session {session_id} from a plan tool ({count} entries)"),
                    ));
                }
            }
            None => {
                self.plans.remove(session_id);
                fx.push(Effect::info(
                    "planSession",
                    format!("Cleared the plan for session {session_id} from a plan tool"),
                ));
            }
        }
    }

    pub(crate) fn on_tool_call(
        &mut self,
        event: ToolCallEvent,
        event_seq: Option<f64>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id;
        let tool = Err("Error: The ACP agent attempted to use a tool".to_owned());
        if self.settle_collector(&session_id, tool) {
            return;
        }
        if self.book.history_covered_tool(&session_id, event_seq) {
            return;
        }
        self.dismiss_settled_child_approvals(&session_id, &event.tool_call, fx);
        let replay_seq = self.book.live_turn_replay_seq(&session_id, event_seq);
        if !self
            .sessions
            .get(&session_id)
            .is_some_and(|session| session.accepts_transcript_events())
        {
            return;
        }
        let incoming = event.tool_call;
        let tool_call_id = incoming.get("toolCallId").cloned();
        let list = self.tool_calls.get(&session_id);
        let idx = list.and_then(|list| {
            list.iter()
                .position(|t| t.get("toolCallId") == tool_call_id.as_ref())
        });
        let prior = idx.and_then(|idx| list.map(|list| &list[idx]));
        let stream = terminal_stream(prior, &incoming);
        let has_list = plan::has_list(&incoming);
        let raw_input = incoming.get("rawInput").cloned();
        let timestamp = timestamp_of(&incoming)
            .cloned()
            .unwrap_or_else(|| json!(env.now));
        let seq = match seq_of(&incoming) {
            Some(seq) => seq.clone(),
            None => json!(replay_seq.unwrap_or_else(|| self.book.next_seq())),
        };
        let ingested = Reader {
            book: &mut self.book,
            fx,
        }
        .ingest(incoming);
        let mut stamped = spread(ingested, stream);
        stamped.insert("timestamp".into(), timestamp);
        stamped.insert("seq".into(), seq);
        let Some(mut marked) = clamp::tool_call(&mut self.book, fx, &session_id, stamped) else {
            return;
        };
        plan::mark(&mut marked);
        let Some(idx) = idx else {
            if has_list {
                self.fold_plan_tool(&session_id, &marked, fx);
            }
            self.tool_calls.entry(session_id).or_default().push(marked);
            return;
        };
        let stored = &self.tool_calls[&session_id][idx];
        let timestamp = present(stored, "timestamp").or_else(|| present(&marked, "timestamp"));
        let seq = present(stored, "seq").or_else(|| present(&marked, "seq"));
        let mut merged = spread(stored.clone(), marked);
        merged.shift_remove("restoredSummary");
        set_or_remove(&mut merged, "timestamp", timestamp);
        set_or_remove(&mut merged, "seq", seq);
        let Some(mut stored) = clamp::tool_call(&mut self.book, fx, &session_id, merged) else {
            return;
        };
        plan::mark(&mut stored);
        if has_list {
            let mut with_input = stored.clone();
            set_or_remove(&mut with_input, "rawInput", raw_input);
            self.fold_plan_tool(&session_id, &with_input, fx);
        }
        if let Some(list) = self.tool_calls.get_mut(&session_id) {
            list[idx] = stored;
        }
    }

    pub(crate) fn on_tool_call_update(
        &mut self,
        event: ToolCallUpdateEvent,
        event_seq: Option<f64>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id;
        if self.book.history_covered_tool(&session_id, event_seq) {
            return;
        }
        self.dismiss_settled_child_approvals(&session_id, &event.update, fx);
        if !self
            .sessions
            .get(&session_id)
            .is_some_and(|session| session.accepts_transcript_events())
        {
            return;
        }
        let update = event.update;
        let tool_call_id = update.get("toolCallId").cloned();
        let Some(idx) = self.tool_calls.get(&session_id).and_then(|list| {
            list.iter()
                .position(|t| t.get("toolCallId") == tool_call_id.as_ref())
        }) else {
            return;
        };
        let has_list = plan::has_list(&update);
        let raw_input = update.get("rawInput").cloned();
        let stored = self.tool_calls[&session_id][idx].clone();
        let stream = terminal_stream(Some(&stored), &update);
        let ingested = Reader {
            book: &mut self.book,
            fx,
        }
        .ingest(update);
        let Some(clamped) =
            clamp::tool_call(&mut self.book, fx, &session_id, spread(ingested, stream))
        else {
            return;
        };
        let carries_locations = clamped.contains_key("locations");
        let timestamp = present(&stored, "timestamp").unwrap_or_else(|| json!(env.now));
        let seq = match present(&stored, "seq") {
            Some(seq) => seq,
            None => json!(self.book.next_seq()),
        };
        let mut merged = spread(stored.clone(), clamped);
        merged.shift_remove("restoredSummary");
        merged.insert("timestamp".into(), timestamp);
        merged.insert("seq".into(), seq);
        let Some(mut merged) = clamp::tool_call(&mut self.book, fx, &session_id, merged) else {
            return;
        };
        if !carries_locations && let Some(locations) = stored.get("locations") {
            merged.insert("locations".into(), locations.clone());
        }
        plan::mark(&mut merged);
        if has_list {
            let mut with_input = merged.clone();
            set_or_remove(&mut with_input, "rawInput", raw_input);
            self.fold_plan_tool(&session_id, &with_input, fx);
        }
        if let Some(list) = self.tool_calls.get_mut(&session_id) {
            list[idx] = merged;
        }
    }
}

/// `call[key]` unless it is `null` or absent, as `call[key] ?? fallback` reads.
fn present(call: &Object, key: &str) -> Option<Value> {
    call.get(key).filter(|value| !value.is_null()).cloned()
}

fn set_or_remove(call: &mut Object, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            call.insert(key.to_owned(), value);
        }
        None => {
            call.shift_remove(key);
        }
    }
}
