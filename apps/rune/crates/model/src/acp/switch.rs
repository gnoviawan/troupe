//! The in-chat agent switch's durable markers (`slices/switch.ts`): the
//! host's `agent_switch` event, the continuation edge it stamps on the
//! index (`stampSwitchContinuation`), and `cancelAgentSwitch`. The switch
//! itself (`armAgentSwitch`, `switchAgent`) needs the warm pool and the
//! handoff summary and is not ported yet.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Env, State, js};

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

    /// `cancelAgentSwitch`: the armed switch is dropped.
    pub(crate) fn cancel_agent_switch(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.get_mut(session_id)
            && session.rest.get("switching").is_some_and(js::truthy)
        {
            session.rest.insert("switching".into(), Value::Null);
        }
    }
}
