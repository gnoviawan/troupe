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
            Effect::ActionResolved { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect()
}

/// Imports one session and answers it; returns the reload the import then
/// waits on.
fn imported(state: &mut State, session_id: &str) -> Effect {
    imported_as(state, session_id, 0)
}

fn imported_as(state: &mut State, session_id: &str, request: u64) -> Effect {
    let request = json!({ "importAgentSessions": { "agentId": "agent-1", "cwd": "/w", "sessions": [{ "sessionId": session_id }], "request": request } });
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
                request: 0,
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

/// Every answer an effect list holds, with the request it answers.
fn answers(effects: &[Effect]) -> Vec<(u64, Value)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ActionResolved { request, value } => Some((*request, value.clone())),
            Effect::ImportProgress {
                request, outcome, ..
            } => Some((*request, outcome.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn each_import_names_its_request_on_progress_and_answer() {
    let mut state = State::default();
    let request = json!({ "importAgentSessions": { "agentId": "agent-1", "cwd": "/w", "sessions": [{ "sessionId": "a" }], "request": 7 } });
    let sent = apply(&mut state, event(request));
    let command = sent
        .into_iter()
        .find(|effect| matches!(effect, Effect::ImportAgentSession { .. }))
        .expect("the import is sent");
    let first = imported_as(&mut state, "b", 8);
    let done = apply(&mut state, Event::AgentCommandDone(command, Value::Null));
    let imported = json!({ "sessionId": "a", "status": "imported" });
    assert_eq!(answers(&done), vec![(7, imported.clone())]);
    let reload = done
        .into_iter()
        .find(|effect| matches!(effect, Effect::RefreshSessionIndex { .. }))
        .expect("the import reloads the index");
    let answered = apply(&mut state, Event::SessionIndexReloaded(reload, vec![]));
    assert_eq!(answers(&answered), vec![(7, json!([imported]))]);
    let answered = apply(&mut state, Event::SessionIndexReloaded(first, vec![]));
    assert_eq!(
        answers(&answered),
        vec![(8, json!([{ "sessionId": "b", "status": "imported" }]))]
    );
}

/// A state with a connected Claude agent in `/work` that can list and load.
fn listing_state() -> State {
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
    serde_json::from_value(fields).expect("a state")
}

#[test]
fn each_listing_names_its_request_on_its_answer() {
    let mut state = listing_state();
    let pages: Vec<Effect> = [3, 4]
        .into_iter()
        .map(|request| {
            let asked = json!({ "listImportableSessions": ["claude", "/work", null, request] });
            apply(&mut state, event(asked))
                .into_iter()
                .find(|effect| matches!(effect, Effect::ListSessions { .. }))
                .expect("a page is asked for")
        })
        .collect();
    let page = |id: &str| json!({ "sessions": [{ "sessionId": id }] });
    let second = apply(
        &mut state,
        Event::AgentCommandDone(pages[1].clone(), page("y")),
    );
    let first = apply(
        &mut state,
        Event::AgentCommandDone(pages[0].clone(), page("x")),
    );
    let ready = |id: &str| json!({ "status": "ready", "agentId": "agent-1", "sessions": [{ "sessionId": id }] });
    assert_eq!(answers(&second), vec![(4, ready("y"))]);
    assert_eq!(answers(&first), vec![(3, ready("x"))]);
}

#[test]
fn a_listing_answered_with_no_page_fails() {
    let mut state = listing_state();
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
