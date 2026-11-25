//! What the recorder cannot show. The chain edges a history open heals
//! (`healChainEdges`): a payload with switch markers or a continuation runs
//! the switch redirect and re-splice, which are not ported. An arm on a chat
//! with no folder: the TypeScript fails with the engine's `TypeError` text.
//! The live-switch links (`liveSwitchSources`), which no vector state shows.

use serde_json::{Value, json};

use super::{Effect, Env, Event, State, step};
use crate::Ids;

fn state_with_index(ids: &[(&str, &str)]) -> State {
    let session_index = ids
        .iter()
        .map(|(id, project)| {
            let Value::Object(row) = json!({ "id": id, "projectId": project }) else {
                unreachable!()
            };
            row
        })
        .collect();
    State {
        session_index,
        ..State::default()
    }
}

fn edges(state: &State) -> Vec<(String, Option<String>)> {
    state
        .session_index
        .iter()
        .map(|row| {
            let id = row["id"].as_str().unwrap_or_default().to_owned();
            let from = row
                .get("continuedFrom")
                .and_then(Value::as_str)
                .map(str::to_owned);
            (id, from)
        })
        .collect()
}

#[test]
fn a_history_install_stamps_its_own_edge_and_its_markers_edges() {
    let mut state = state_with_index(&[("s0", "p1"), ("s1", "p1"), ("s3", "p1")]);
    let payload = json!({
        "metadata": { "id": "s1", "agentId": "a", "cwd": "/w", "continuedFrom": "s0", "createdAt": 1 },
        "messages": [],
        "switches": [
            { "id": "switch:seq-9", "toConfigId": "codex", "newSessionId": "s3", "seq": 9 },
            { "id": "switch:seq-10", "toConfigId": "codex", "newSessionId": "", "seq": 10 }
        ]
    });
    let mut fx = Vec::new();
    assert!(state.install_history("s1", &payload, &mut fx).is_some());
    assert_eq!(
        edges(&state),
        vec![
            ("s3".into(), Some("s1".into())),
            ("s1".into(), Some("s0".into())),
            ("s0".into(), None),
        ]
    );
    assert_eq!(state.book.history_watermarks.get("s1"), Some(&10.0));
}

struct NoIds;

impl Ids for NoIds {
    fn next_id(&mut self) -> String {
        unreachable!("an arm draws no id")
    }
}

#[test]
fn an_arm_on_a_chat_with_no_folder_arms_and_then_fails() {
    let mut state: State = serde_json::from_value(json!({
        "sessions": { "s1": {
            "id": "s1", "agentId": "agent-1", "status": "active", "title": null,
            "activeTurn": false, "openTurnId": null, "createdAt": 1
        } },
        "messages": { "s1": [] }, "toolCalls": {}, "plans": {}, "commands": {},
        "commandsSeq": {}, "sessionUsage": {}, "sessionIndex": [],
        "pendingPermissions": {}, "pendingQuestions": {}, "pendingElicitations": {},
        "pendingBrowserConsents": {}, "agentStatus": {}, "turnEndNotices": {},
        "promptQueues": {}, "suppressQueueFlush": {}, "agentOptionsCache": {},
        "configToLiveAgent": {}, "activeSessionId": "s1", "preparedSessions": {},
        "agentSwitches": {}, "agents": {}, "pendingSignIn": {}, "autoOpenBrowserUrl": {},
        "launchingSessionIds": {}, "discoveredSessions": {},
        "agentConfigs": [{ "id": "codex", "name": "Codex", "command": "codex-acp" }],
        "book": { "seq": 0, "historyWatermarks": {} }
    }))
    .expect("a state");
    let mut env = Env {
        now: 1_000.0,
        ids: &mut NoIds,
        server_history: false,
    };
    let event = Event::ArmAgentSwitch("s1".into(), "codex".into(), 4);
    let effects = step(&mut state, event, &mut env);
    assert_eq!(
        effects,
        vec![Effect::ActionRejected {
            message: "TypeError: session s1 has no cwd to warm codex in".into()
        }]
    );
    assert_eq!(
        state.sessions["s1"].rest.get("switching"),
        Some(&json!({ "toConfigId": "codex", "status": "pending" }))
    );
    assert!(state.preparing_chat_keys.is_empty());
}

#[test]
fn live_switch_links_die_with_either_endpoint_unless_an_open_tab_keeps_them() {
    let mut state = State::default();
    let session = |id: &str| {
        serde_json::from_value(json!({
            "id": id, "agentId": "agent-1", "status": "active", "title": null,
            "activeTurn": false, "openTurnId": null
        }))
        .expect("a session")
    };
    for id in ["s1", "s2", "s3", "s9"] {
        state.sessions.insert(id.into(), session(id));
        state.messages.insert(id.into(), Vec::new());
    }
    let links = [("s2", "s1"), ("s3", "s9")];
    for (target, source) in links {
        let link = (source.to_owned(), json!({ "newSessionId": target }));
        state.book.switch.live_sources.insert(target.into(), link);
    }
    state.book.open_chat_tabs.insert("s2".into());
    let mut env = Env {
        now: 1_000.0,
        ids: &mut NoIds,
        server_history: false,
    };
    let linked = |state: &State| -> Vec<(String, String)> {
        let links = &state.book.switch.live_sources;
        links
            .iter()
            .map(|(t, (s, _))| (t.clone(), s.clone()))
            .collect()
    };

    let closed = json!({ "sessionId": "s2", "agentId": "agent-1" });
    let closed = Event::SessionClosed(serde_json::from_value(closed).expect("an event"));
    step(&mut state, closed, &mut env);
    assert_eq!(
        linked(&state),
        [("s2".into(), "s1".into()), ("s3".into(), "s9".into())]
    );

    step(
        &mut state,
        Event::DropSessionTranscript("s1".into()),
        &mut env,
    );
    assert_eq!(linked(&state), [("s3".into(), "s9".into())]);

    step(
        &mut state,
        Event::DropSessionTranscript("s3".into()),
        &mut env,
    );
    assert_eq!(linked(&state), []);
}
