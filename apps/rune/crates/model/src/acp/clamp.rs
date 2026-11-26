//! Bounds on what an agent can make the live transcript retain (CAP-2, F-2):
//! oversized content blocks, tool call fields and whole tool calls. Port of
//! the clamps in `slices/transcript.ts`.

use serde_json::{Value, json};

use super::js;
use super::types::Object;
use super::{Book, Effect};

/// `MAX_LIVE_RAW_OUTPUT_CHARS`.
const RAW_OUTPUT_CHARS: usize = 32 * 1024;
const RAW_OUTPUT_MARKER: &str = "\n[termul: tool output truncated]";
/// `MAX_LIVE_TOOL_CALL_FIELD_CHARS`.
pub(crate) const FIELD_CHARS: usize = 64 * 1024;
const CONTENT_CHARS: usize = 256 * 1024;
const TOTAL_CHARS: usize = 384 * 1024;
const SUBAGENT_RECORD_CHARS: usize = 16 * 1024;
const TITLE_CHARS: usize = 1024;
const LOCATIONS: usize = 32;
const FIELD_MARKER: &str = "\n[termul: tool call content truncated]";
/// `MAX_LIVE_TOOL_CALLS`; the clamp log remembers twice as many ids.
const LIVE_TOOL_CALLS: usize = 500;

/// `clampLiveContentBlock`: a non-text block over the field bound becomes a
/// marker text block.
pub(crate) fn content_block(block: Value) -> Value {
    if block.get("type").and_then(Value::as_str) == Some("text") {
        return block;
    }
    if js::serialized_len(Some(&block)) <= FIELD_CHARS {
        return block;
    }
    json!({ "type": "text", "text": "[termul: oversized content block omitted]" })
}

pub(crate) fn field_text(text: &str) -> String {
    if js::len(text) <= FIELD_CHARS {
        text.to_owned()
    } else {
        format!("{}{FIELD_MARKER}", js::head(text, FIELD_CHARS))
    }
}

fn hint(value: Option<&Value>) -> Option<Value> {
    let text = value?.as_str()?;
    (js::len(text) > TITLE_CHARS).then(|| json!(format!("{}…", js::head(text, TITLE_CHARS))))
}

pub(crate) fn log_key(tool_call_id: &str) -> String {
    if js::len(tool_call_id) > TITLE_CHARS {
        format!("{}…", js::head(tool_call_id, TITLE_CHARS))
    } else {
        tool_call_id.to_owned()
    }
}

/// `logToolCallClampOnce`: one warn per tool call id, the oldest id
/// forgotten past the cap.
fn log_once(
    book: &mut Book,
    fx: &mut Vec<Effect>,
    session_id: &str,
    tool_call_id: &str,
    what: &str,
) {
    let key = log_key(tool_call_id);
    if book.clamp_logged.contains(&key) {
        return;
    }
    if book.clamp_logged.len() >= 2 * LIVE_TOOL_CALLS {
        book.clamp_logged.pop_front();
    }
    book.clamp_logged.push_back(key.clone());
    fx.push(Effect::warn(
        "acp.store",
        format!("{what} for call {key} (session {session_id})"),
    ));
}

/// `projectUnderBound`: keep an over-budget object's small fields; drop a
/// value that is still over, or is not an object. `None` in, `None` out.
fn project_under_bound(value: Option<&Value>, bound: usize) -> Projected {
    let Some(value) = value else {
        return Projected::Same;
    };
    if value.is_null() || js::serialized_len(Some(value)) <= bound {
        return Projected::Same;
    }
    let Some(record) = value.as_object() else {
        return Projected::Dropped;
    };
    let projected: Object = record
        .iter()
        .filter(|(_, entry)| js::serialized_len(Some(entry)) <= bound)
        .map(|(key, entry)| (key.clone(), entry.clone()))
        .collect();
    let projected = Value::Object(projected);
    if js::serialized_len(Some(&projected)) <= bound {
        Projected::New(projected)
    } else {
        Projected::Dropped
    }
}

enum Projected {
    Same,
    New(Value),
    Dropped,
}

impl Projected {
    /// Write the outcome onto `key`: a dropped value deletes the key.
    fn apply(self, call: &mut Object, key: &str) -> bool {
        match self {
            Self::Same => false,
            Self::New(value) => {
                call.insert(key.to_owned(), value);
                true
            }
            Self::Dropped => {
                call.shift_remove(key);
                true
            }
        }
    }
}

fn raw_output(
    book: &mut Book,
    fx: &mut Vec<Effect>,
    session_id: &str,
    id: &str,
    raw: Option<&Value>,
) -> Projected {
    let Some(Value::String(text)) = raw else {
        let projected = project_under_bound(raw, RAW_OUTPUT_CHARS);
        if !matches!(projected, Projected::Same) {
            log_once(book, fx, session_id, id, "Clamped tool rawOutput");
        }
        return projected;
    };
    if js::len(text) <= RAW_OUTPUT_CHARS {
        return Projected::Same;
    }
    log_once(book, fx, session_id, id, "Clamped tool rawOutput");
    Projected::New(json!(format!(
        "{}{RAW_OUTPUT_MARKER}",
        js::head(text, RAW_OUTPUT_CHARS)
    )))
}

/// `clampLiveToolCallContent`.
fn content(content: Option<&Value>) -> Projected {
    let Some(Value::Array(items)) = content else {
        return Projected::Same;
    };
    let mut changed = false;
    let next: Vec<Value> = items
        .iter()
        .map(|item| {
            let kind = item.get("type").and_then(Value::as_str);
            if kind == Some("diff") {
                let mut clamped = item.clone();
                for key in ["oldText", "newText"] {
                    if let Some(Value::String(text)) = item.get(key)
                        && js::len(text) > FIELD_CHARS
                    {
                        clamped[key] = json!(field_text(text));
                        changed = true;
                    }
                }
                return clamped;
            }
            if kind == Some("content") {
                let block = item.get("content");
                let text = block
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                    .and_then(|block| block.get("text"))
                    .and_then(Value::as_str);
                if let Some(text) = text.filter(|text| js::len(text) > FIELD_CHARS) {
                    changed = true;
                    let mut clamped = item.clone();
                    clamped["content"]["text"] = json!(field_text(text));
                    return clamped;
                }
            }
            if js::serialized_len(Some(item)) > FIELD_CHARS * 2 {
                changed = true;
                return json!({
                    "type": "content",
                    "content": {
                        "type": "text",
                        "text": "[termul: oversized tool call content item omitted]"
                    }
                });
            }
            item.clone()
        })
        .collect();
    let candidate = if changed {
        Value::Array(next)
    } else {
        Value::Array(items.clone())
    };
    if js::serialized_len(Some(&candidate)) > CONTENT_CHARS {
        Projected::Dropped
    } else if changed {
        Projected::New(candidate)
    } else {
        Projected::Same
    }
}

/// `clampLiveToolCallFields`: bound the agent-controlled fields of a tool
/// call or update. A call still over the total bound keeps only its
/// structural subset; `None` when even that is over.
pub(crate) fn tool_call(
    book: &mut Book,
    fx: &mut Vec<Effect>,
    session_id: &str,
    call: Object,
) -> Option<Object> {
    let id = match call.get("toolCallId") {
        Some(Value::String(id)) if js::len(id) <= FIELD_CHARS => id.clone(),
        other => {
            log_once(
                book,
                fx,
                session_id,
                &js::string_of(other),
                "Dropped over-size tool call",
            );
            return None;
        }
    };
    let mut next = call;
    let mut changed = false;
    for key in ["title", "kind", "status"] {
        if let Some(clamped) = hint(next.get(key)) {
            next.insert(key.to_owned(), clamped);
            changed = true;
        }
    }
    changed |= content(next.get("content")).apply(&mut next, "content");
    changed |= project_under_bound(next.get("rawInput"), FIELD_CHARS).apply(&mut next, "rawInput");
    let raw = next.get("rawOutput").cloned();
    changed |= raw_output(book, fx, session_id, &id, raw.as_ref()).apply(&mut next, "rawOutput");
    if let Some(Value::Array(locations)) = next.get_mut("locations")
        && locations.len() > LOCATIONS
    {
        locations.truncate(LOCATIONS);
        changed = true;
    }
    let whole = Value::Object(next);
    if js::serialized_len(Some(&whole)) <= TOTAL_CHARS {
        if changed {
            log_once(
                book,
                fx,
                session_id,
                &id,
                "Clamped oversized tool call fields",
            );
        }
        let Value::Object(next) = whole else {
            unreachable!()
        };
        return Some(next);
    }
    log_once(book, fx, session_id, &id, "Degraded oversized tool call");
    let Value::Object(next) = whole else {
        unreachable!()
    };
    let reduced = structural_subset(&id, &next);
    (js::serialized_len(Some(&Value::Object(reduced.clone()))) <= TOTAL_CHARS).then_some(reduced)
}

fn structural_subset(id: &str, next: &Object) -> Object {
    let mut reduced = Object::new();
    reduced.insert("toolCallId".into(), json!(id));
    let mut keep = |key: &str, ok: bool| {
        if ok && let Some(value) = next.get(key) {
            reduced.insert(key.to_owned(), value.clone());
        }
    };
    let within = |key: &str, bound: usize| {
        next.get(key)
            .is_some_and(|value| js::serialized_len(Some(value)) <= bound)
    };
    keep("title", next.get("title").is_some_and(Value::is_string));
    keep("kind", within("kind", FIELD_CHARS));
    keep("status", within("status", FIELD_CHARS));
    keep("locations", within("locations", FIELD_CHARS));
    keep("rawOutput", true);
    keep("subagent", within("subagent", SUBAGENT_RECORD_CHARS));
    keep(
        "subagentControl",
        within("subagentControl", SUBAGENT_RECORD_CHARS),
    );
    keep(
        "parentToolCallId",
        next.get("parentToolCallId")
            .and_then(Value::as_str)
            .is_some_and(|parent| js::len(parent) <= 512),
    );
    keep(
        "subagentMessages",
        next.get("subagentMessages").is_some_and(Value::is_array)
            && within("subagentMessages", FIELD_CHARS),
    );
    keep(
        "terminalOutput",
        next.get("terminalOutput")
            .and_then(Value::as_str)
            .is_some_and(|text| js::len(text) <= FIELD_CHARS),
    );
    keep(
        "terminalExitCode",
        js::finite(next.get("terminalExitCode")).is_some(),
    );
    keep(
        "timestamp",
        next.get("timestamp").is_some_and(Value::is_number),
    );
    keep("seq", next.get("seq").is_some_and(Value::is_number));
    reduced
}
