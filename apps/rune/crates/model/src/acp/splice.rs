//! The switch splice (`helpers.ts` `spliceSwitchTranscript`,
//! `withSwitchMarker`): a source session's transcript copied in front of
//! the session the conversation continues in, under one id namespace.

use serde_json::{Value, json};

use super::history_install::max_payload_seq;
use super::types::{ChatMessage, Object};

/// `SWITCH_SPLICE_ID_PREFIX`: spliced records are
/// `switch-splice:<source session>:<original id>`.
pub(crate) const SPLICE_PREFIX: &str = "switch-splice:";

/// A session's messages, tool calls and switch markers.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Band {
    pub messages: Vec<ChatMessage>,
    pub tool_calls: Vec<Object>,
    pub switches: Vec<Value>,
}

impl Band {
    /// `maxPayloadSeq` of the band.
    fn max_seq(&self) -> f64 {
        max_payload_seq(&json!({
            "messages": self.messages,
            "toolCalls": self.tool_calls,
            "switches": self.switches,
        }))
    }
}

fn seq_of(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

/// `withSwitchMarker`: the band with a separator naming `marker`'s
/// `newSessionId`, fabricated on top of the band when it has none.
pub(crate) fn with_switch_marker(band: Band, marker: &Value) -> (Band, bool) {
    let target = marker.get("newSessionId");
    if band
        .switches
        .iter()
        .any(|sw| sw.get("newSessionId") == target)
    {
        return (band, false);
    }
    let top = band.max_seq() + 1.0;
    let mut record = marker.clone();
    record["id"] = json!(format!("switch:fabricated:{}", crate::js_number(top)));
    record["seq"] = json!(top);
    let mut band = band;
    band.switches.push(record);
    (band, true)
}

/// `spliceSwitchTranscript`: `installed` re-stamped below the target's
/// lowest seq, its ids namespaced by `source`, in front of the target's
/// records; a record already spliced is skipped.
pub(crate) fn splice_switch_transcript(source: &str, installed: &Band, target: Band) -> Band {
    let floor = target
        .messages
        .iter()
        .map(|m| m.seq.unwrap_or(0.0))
        .chain(target.tool_calls.iter().map(|t| seq_of(t.get("seq"))))
        .chain(target.switches.iter().map(|sw| seq_of(sw.get("seq"))))
        .fold(0.0, f64::min);
    let offset = floor - installed.max_seq() - 1.0;
    let spliced = |id: &str| format!("{SPLICE_PREFIX}{source}:{id}");

    let mut seen: Vec<String> = target.messages.iter().map(|m| m.id.clone()).collect();
    let mut messages = Vec::new();
    for message in &installed.messages {
        let id = spliced(&message.id);
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        let mut copy = message.clone();
        copy.id = id;
        copy.seq = Some(message.seq.unwrap_or(0.0) + offset);
        messages.push(copy);
    }
    messages.extend(target.messages);

    let call_id = |call: &Object| super::js::string_of(call.get("toolCallId"));
    let mut seen: Vec<String> = target.tool_calls.iter().map(call_id).collect();
    let mut tool_calls = Vec::new();
    for call in &installed.tool_calls {
        let id = spliced(&call_id(call));
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        let mut copy = call.clone();
        copy.insert("toolCallId".into(), json!(id));
        copy.insert("seq".into(), json!(seq_of(call.get("seq")) + offset));
        tool_calls.push(copy);
    }
    tool_calls.extend(target.tool_calls);

    let switch_id = |record: &Value| super::js::string_of(record.get("id"));
    let mut seen: Vec<String> = target.switches.iter().map(switch_id).collect();
    let mut switches = Vec::new();
    for record in &installed.switches {
        let id = spliced(&switch_id(record));
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        let mut copy = record.clone();
        copy["id"] = json!(id);
        copy["seq"] = json!(seq_of(record.get("seq")) + offset);
        switches.push(copy);
    }
    switches.extend(target.switches);
    Band {
        messages,
        tool_calls,
        switches,
    }
}
