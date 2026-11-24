//! Live window cases the vectors leave out because each would snapshot a
//! full window at every step: the exact bounds, summaries already in the
//! list, and a subagent chunk the live-turn attach replays.

use serde_json::{Value, json};

use super::types::Object;
use super::window::{MAX_LIVE_TOOL_CALLS, MAX_LIVE_WINDOW_MESSAGES, trim_live_tool_calls};
use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct Counter(u32);

impl Ids for Counter {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

/// An active chat holding `messages` finished user messages.
fn chat(messages: usize) -> State {
    let mut value = serde_json::to_value(State::default()).expect("a state");
    value["sessions"] = json!({ "s1": {
        "id": "s1",
        "agentId": "agent-1",
        "status": "active",
        "title": null,
        "activeTurn": true,
        "openTurnId": "turn:t1",
        "lastError": null,
    }});
    let list: Vec<Value> = (0..messages)
        .map(|index| {
            json!({ "id": format!("m{index}"), "role": "user", "blocks": [], "streaming": false, "timestamp": 1 })
        })
        .collect();
    value["messages"] = json!({ "s1": list });
    serde_json::from_value(value).expect("a state")
}

fn apply(state: &mut State, event: Value) -> Vec<Effect> {
    let mut ids = Counter(0);
    let mut env = Env {
        now: 1_000.0,
        ids: &mut ids,
        server_history: false,
    };
    let event: Event = serde_json::from_value(event).expect("an event");
    step(state, event, &mut env)
}

fn probes(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::ReadSessionPayload { .. }))
        .count()
}

fn call(id: &str, status: &str) -> Object {
    json!({ "toolCallId": id, "status": status })
        .as_object()
        .cloned()
        .expect("an object")
}

#[test]
fn a_window_at_the_bound_neither_trims_nor_probes() {
    let mut state = chat(MAX_LIVE_WINDOW_MESSAGES - 1);
    let fx = apply(
        &mut state,
        json!({ "_onMessageChunk": [{ "sessionId": "s1", "role": "agent", "content": { "type": "text", "text": "a" } }] }),
    );
    assert_eq!(state.messages["s1"].len(), MAX_LIVE_WINDOW_MESSAGES);
    assert_eq!(probes(&fx), 0, "a full window is not over the bound");
    let fx = apply(&mut state, json!({ "clearSessionBackfill": "s1" }));
    assert_eq!(
        probes(&fx),
        0,
        "nor is it when the reader returns to the edge"
    );
}

#[test]
fn a_subagent_chunk_the_attach_replays_is_flushed() {
    let mut state = chat(MAX_LIVE_WINDOW_MESSAGES + 1);
    state.book.history_watermarks.insert("s1".into(), 40.0);
    state.book.live_turn_replay_floors.insert("s1".into(), 30.0);
    let chunk = json!({ "_onMessageChunk": [
        { "sessionId": "s1", "role": "agent", "parentToolCallId": "d1", "content": { "type": "text", "text": "a" } },
        35
    ] });
    assert_eq!(
        probes(&apply(&mut state, chunk)),
        1,
        "covered by history, yet replayed for the open turn"
    );
}

#[test]
fn summaries_do_not_count_toward_the_tool_call_bound() {
    let mut calls = vec![
        json!({ "toolCallId": "s", "restoredSummary": true, "kind": "edit" })
            .as_object()
            .cloned()
            .expect("an object"),
    ];
    calls.extend((0..MAX_LIVE_TOOL_CALLS).map(|index| call(&format!("c{index}"), "completed")));
    trim_live_tool_calls(&mut calls);
    assert_eq!(calls.len(), MAX_LIVE_TOOL_CALLS + 1, "nothing dropped");

    calls.push(call("new", "completed"));
    trim_live_tool_calls(&mut calls);
    let ids: Vec<&str> = calls
        .iter()
        .take(2)
        .filter_map(|call| call.get("toolCallId").and_then(Value::as_str))
        .collect();
    assert_eq!(ids, ["s", "c1"], "only the oldest finished call dropped");
}

#[test]
fn the_in_flight_guard_keeps_every_summary() {
    let mut calls = vec![
        json!({ "toolCallId": "s", "restoredSummary": true })
            .as_object()
            .cloned()
            .expect("an object"),
    ];
    calls.extend((0..=MAX_LIVE_TOOL_CALLS).map(|index| call(&format!("p{index}"), "pending")));
    trim_live_tool_calls(&mut calls);
    let ids: Vec<&str> = calls
        .iter()
        .take(2)
        .filter_map(|call| call.get("toolCallId").and_then(Value::as_str))
        .collect();
    assert_eq!(
        ids,
        ["s", "p1"],
        "the oldest in-flight call goes, the summary stays"
    );
    assert_eq!(calls.len(), MAX_LIVE_TOOL_CALLS + 1);
}

#[test]
fn a_chunk_the_history_covers_is_not_flushed() {
    let mut state = chat(MAX_LIVE_WINDOW_MESSAGES + 1);
    state.book.history_watermarks.insert("s1".into(), 40.0);
    let chunk = |seq: f64| {
        json!({ "_onMessageChunk": [
            { "sessionId": "s1", "role": "agent", "content": { "type": "text", "text": "a" } },
            seq
        ] })
    };
    assert_eq!(probes(&apply(&mut state, chunk(40.0))), 0, "covered");
    assert_eq!(probes(&apply(&mut state, chunk(41.0))), 1, "live");
}
