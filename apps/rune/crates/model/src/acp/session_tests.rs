use serde_json::{Value, json};

use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct NoIds;

impl Ids for NoIds {
    fn next_id(&mut self) -> String {
        unreachable!("these steps draw no ids")
    }
}

fn state(sessions: Value, messages: Value, tool_calls: Value) -> State {
    let mut value = serde_json::to_value(State::default()).expect("a state");
    value["sessions"] = sessions;
    value["messages"] = messages;
    value["toolCalls"] = tool_calls;
    serde_json::from_value(value).expect("a state")
}

fn session(id: &str, extra: Value) -> Value {
    let mut session = json!({
        "id": id,
        "agentId": "agent-1",
        "status": "active",
        "title": null,
        "activeTurn": false,
        "openTurnId": null,
        "lastError": null,
    });
    session
        .as_object_mut()
        .expect("an object")
        .extend(extra.as_object().cloned().unwrap_or_default());
    session
}

fn apply(state: &mut State, event: Value) -> Vec<Effect> {
    let event: Event = serde_json::from_value(event).expect("an event");
    let mut ids = NoIds;
    let mut env = Env {
        now: 1_000.0,
        ids: &mut ids,
        server_history: true,
    };
    step(state, event, &mut env)
}

/// Where `PersistSession` and `DropSessionTranscript` sit for `session_id`.
fn persist_then_drop(effects: &[Effect], session_id: &str) -> (Option<usize>, Option<usize>) {
    let at = |want: fn(&Effect, &str) -> bool| effects.iter().position(|e| want(e, session_id));
    (
        at(|e, id| matches!(e, Effect::PersistSession { session_id } if session_id == id)),
        at(|e, id| matches!(e, Effect::DropSessionTranscript { session_id } if session_id == id)),
    )
}

#[test]
fn a_closed_session_is_persisted_before_its_transcript_is_freed() {
    let message =
        json!({ "id": "u1", "role": "user", "blocks": [], "streaming": false, "timestamp": 1 });
    let mut state = state(
        json!({ "s1": session("s1", json!({})) }),
        json!({ "s1": [message] }),
        json!({ "s1": [{ "toolCallId": "tc-1" }] }),
    );
    let effects = apply(
        &mut state,
        json!({ "_onSessionClosed": { "agentId": "agent-1", "sessionId": "s1" } }),
    );
    let (persist, drop) = persist_then_drop(&effects, "s1");
    assert!(persist < drop && persist.is_some(), "{effects:?}");
    assert_eq!(
        state.messages["s1"].len(),
        1,
        "the persist reads the transcript"
    );
    assert_eq!(
        state.tool_calls["s1"].len(),
        1,
        "the persist reads the tool calls"
    );
    apply(&mut state, json!({ "dropSessionTranscript": "s1" }));
    assert!(!state.messages.contains_key("s1"));
    assert!(!state.tool_calls.contains_key("s1"));
}

#[test]
fn a_disconnect_persists_an_empty_chat_before_freeing_it() {
    let mut state = state(
        json!({ "s1": session("s1", json!({})) }),
        json!({ "s1": [] }),
        json!({ "s1": [{ "toolCallId": "tc-1" }] }),
    );
    let effects = apply(
        &mut state,
        json!({ "_onAgentDisconnected": { "agentId": "agent-1", "reason": "killed" } }),
    );
    let (persist, drop) = persist_then_drop(&effects, "s1");
    assert!(persist < drop && persist.is_some(), "{effects:?}");
    assert_eq!(
        state.tool_calls["s1"].len(),
        1,
        "the persist reads the tool calls"
    );
}

#[test]
fn options_that_are_not_objects_do_not_panic_when_advertised_again() {
    let mut state = state(
        json!({ "s1": session("s1", json!({ "modes": "default", "models": 7, "configOptions": [] })) }),
        json!({ "s1": [] }),
        json!({}),
    );
    for _ in 0..2 {
        apply(
            &mut state,
            json!({ "_onSessionCreated": {
                "agentId": "agent-1",
                "sessionId": "s1",
                "modes": "default",
                "models": { "currentModelId": "m1", "availableModels": [] },
            } }),
        );
    }
    let rest = &state.sessions["s1"].rest;
    assert_eq!(rest["modes"], json!("default"));
    assert_eq!(rest["models"], json!(7));
}
