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

/// Imports one session and answers it; the import then waits on the index.
fn imported(state: &mut State) -> Vec<Effect> {
    let request = json!({ "importAgentSessions": { "agentId": "agent-1", "cwd": "/w", "sessions": [{ "sessionId": "a" }] } });
    let sent = apply(state, event(request));
    let command = sent
        .into_iter()
        .find(|effect| matches!(effect, Effect::ImportAgentSession { .. }))
        .expect("the import is sent");
    apply(state, Event::AgentCommandDone(command, Value::Null))
}

#[test]
fn an_import_answers_once_the_reloaded_index_arrives() {
    let mut state = State::default();
    let done = imported(&mut state);
    assert!(done.contains(&Effect::RefreshSessionIndex));
    assert_eq!(resolved(&done), Vec::<Value>::new());
    let reloaded = apply(&mut state, Event::ApplySessionIndexSnapshot(Vec::new()));
    assert_eq!(
        resolved(&reloaded),
        vec![json!([{ "sessionId": "a", "status": "imported" }])]
    );
    let again = apply(&mut state, Event::ApplySessionIndexSnapshot(Vec::new()));
    assert_eq!(resolved(&again), Vec::<Value>::new());
}

#[test]
fn an_import_answers_after_a_failed_reload_too() {
    let mut state = State::default();
    imported(&mut state);
    let failed = apply(
        &mut state,
        Event::SessionIndexReloadFailed("disk gone".into()),
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
    let none = apply(&mut state, Event::SessionIndexReloadFailed("again".into()));
    assert_eq!(none, Vec::new());
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
