//! What the recorder cannot answer: its index reload never settles, so the
//! import's outcomes and the reload failure are checked here, with the
//! `Date.parse` forms an agent sends.

use serde_json::{Value, json};

use super::import::date_parse;
use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct NoIds;

impl Ids for NoIds {
    fn next_id(&mut self) -> String {
        unreachable!("these steps draw no ids")
    }
}

fn apply(state: &mut State, event: Event) -> Vec<Effect> {
    let mut ids = NoIds;
    let mut env = Env {
        now: 1_000.0,
        ids: &mut ids,
        server_history: false,
    };
    step(state, event, &mut env)
}

fn event(value: Value) -> Event {
    serde_json::from_value(value).expect("an event")
}

fn resolved(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ActionResolved { value } => Some(value.clone()),
            _ => None,
        })
        .collect()
}

/// Imports one session and answers it; returns the reload the import then
/// waits on.
fn imported(state: &mut State, session_id: &str) -> Effect {
    let request = json!({ "importAgentSessions": { "agentId": "agent-1", "cwd": "/w", "sessions": [{ "sessionId": session_id }] } });
    let sent = apply(state, event(request));
    let command = sent
        .into_iter()
        .find(|effect| matches!(effect, Effect::ImportAgentSession { .. }))
        .expect("the import is sent");
    let done = apply(state, Event::AgentCommandDone(command, Value::Null));
    assert_eq!(resolved(&done), Vec::<Value>::new());
    done.into_iter()
        .find(|effect| matches!(effect, Effect::RefreshSessionIndex { import: Some(_) }))
        .expect("the import reloads the index")
}

#[test]
fn an_import_answers_once_its_own_reload_arrives() {
    let mut state = State::default();
    let reload = imported(&mut state, "a");
    let other = apply(&mut state, Event::ApplySessionIndexSnapshot(Vec::new()));
    assert_eq!(resolved(&other), Vec::<Value>::new());
    let reloaded = apply(
        &mut state,
        Event::SessionIndexReloaded(reload.clone(), vec![]),
    );
    assert_eq!(
        resolved(&reloaded),
        vec![json!([{ "sessionId": "a", "status": "imported" }])]
    );
    let again = apply(&mut state, Event::SessionIndexReloaded(reload, vec![]));
    assert_eq!(resolved(&again), Vec::<Value>::new());
}

#[test]
fn each_import_answers_on_its_own_reload() {
    let mut state = State::default();
    let first = imported(&mut state, "a");
    let second = imported(&mut state, "b");
    let reloaded = apply(&mut state, Event::SessionIndexReloaded(second, vec![]));
    assert_eq!(
        resolved(&reloaded),
        vec![json!([{ "sessionId": "b", "status": "imported" }])]
    );
    let reloaded = apply(&mut state, Event::SessionIndexReloaded(first, vec![]));
    assert_eq!(
        resolved(&reloaded),
        vec![json!([{ "sessionId": "a", "status": "imported" }])]
    );
}

#[test]
fn an_import_answers_after_a_failed_reload_too() {
    let mut state = State::default();
    let reload = imported(&mut state, "a");
    let failed = apply(
        &mut state,
        Event::SessionIndexReloadFailed(reload.clone(), "disk gone".into()),
    );
    assert_eq!(
        failed,
        vec![
            Effect::warn(
                "acp.importSessions.refresh",
                "Chat history refresh after import failed: Error: disk gone".into()
            ),
            Effect::ActionResolved {
                value: json!([{ "sessionId": "a", "status": "imported" }])
            },
        ]
    );
    let none = apply(
        &mut state,
        Event::SessionIndexReloadFailed(reload, "again".into()),
    );
    assert_eq!(none, Vec::new());
}

#[test]
fn a_listing_answered_with_no_page_fails() {
    let mut fields = serde_json::to_value(State::default()).expect("a state");
    let live = json!({
        "agentConfigs": [{ "id": "claude", "name": "Claude", "command": "c" }],
        "agents": { "agent-1": { "id": "agent-1", "capabilities": { "loadSession": true, "sessionCapabilities": { "list": {} } } } },
        "agentStatus": { "agent-1": "connected" },
        "configToLiveAgent": { "claude\u{0}/work": "agent-1" }
    });
    for (key, value) in live.as_object().expect("an object") {
        fields[key] = value.clone();
    }
    let mut state: State = serde_json::from_value(fields).expect("a state");
    let sent = apply(
        &mut state,
        event(json!({ "listImportableSessions": ["claude", "/work"] })),
    );
    let page = sent
        .into_iter()
        .find(|effect| matches!(effect, Effect::ListSessions { .. }))
        .expect("a page is asked for");
    let failed = apply(&mut state, Event::AgentCommandDone(page, Value::Null));
    assert_eq!(
        failed,
        vec![Effect::ActionRejected {
            message: "Error: session/list answered with no page".into()
        }]
    );
}

#[test]
fn date_parse_reads_what_javascript_reads() {
    // Expected values printed by `Date.parse` under bun.
    for (text, ms) in [
        ("2026-01-02", Some(1_767_312_000_000)),
        ("2026-01-02T03:04:05Z", Some(1_767_323_045_000)),
        ("2026-01-02T03:04:05.678Z", Some(1_767_323_045_678)),
        ("2026-01-02T03:04:05.6Z", Some(1_767_323_045_600)),
        ("2026-01-02T03:04Z", Some(1_767_323_040_000)),
        ("2026-01-02T03:04:05+02:00", Some(1_767_315_845_000)),
        ("2026-01-02T03:04:05-05:30", Some(1_767_342_845_000)),
        ("2024-02-29T00:00:00Z", Some(1_709_164_800_000)),
        ("1969-12-31T23:59:59Z", Some(-1_000)),
        ("2026-02-30", Some(1_772_409_600_000)),
        ("2026-02-32", None),
        ("2026-13-01", None),
        ("2026-01-02T25:00:00Z", None),
        ("2026-01-02 03:04:05Z", Some(1_767_323_045_000)),
        ("2026-01-02t03:04Z", Some(1_767_323_040_000)),
        ("yesterday", None),
        ("", None),
    ] {
        assert_eq!(date_parse(text), ms, "{text}");
    }
}
