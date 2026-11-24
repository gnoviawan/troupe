//! MCP paths the recorder cannot interleave: it answers every command within
//! the step that sent it, so a write queued behind a save, a probe that
//! outlives an edit or a delete, and a late answer of an aborted flow never
//! happen there.

use serde_json::{Value, json};

use super::mcp::McpError;
use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct Counter(u32);

impl Ids for Counter {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

fn server(id: &str, url: &str) -> Value {
    json!({ "id": id, "type": "http", "name": format!("srv-{id}"), "url": url, "headers": [], "enabled": true })
}

fn registry(servers: Value) -> State {
    let mut value = serde_json::to_value(State::default()).expect("a state");
    value["mcpServers"] = servers;
    value["mcpServersLoaded"] = json!(true);
    serde_json::from_value(value).expect("a state")
}

fn apply(state: &mut State, event: Event) -> Vec<Effect> {
    let mut ids = Counter(0);
    let mut env = Env {
        now: 1_000.0,
        ids: &mut ids,
        server_history: false,
    };
    step(state, event, &mut env)
}

fn action(state: &mut State, event: Value) -> Vec<Effect> {
    apply(state, serde_json::from_value(event).expect("an event"))
}

fn find(effects: &[Effect], kind: fn(&Effect) -> bool) -> Effect {
    effects
        .iter()
        .find(|e| kind(e))
        .cloned()
        .expect("the effect")
}

fn is_save(effect: &Effect) -> bool {
    matches!(effect, Effect::SaveMcpServers { .. })
}

fn is_probe(effect: &Effect) -> bool {
    matches!(effect, Effect::ProbeMcpServer { .. })
}

fn is_start(effect: &Effect) -> bool {
    matches!(effect, Effect::StartMcpOAuth { .. })
}

fn ids(state: &State) -> Vec<String> {
    state
        .mcp_servers
        .iter()
        .filter_map(|server| server.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

#[test]
fn a_write_waits_for_the_save_in_flight_and_rolls_back_against_its_own_turn() {
    let mut state = registry(json!([server("a", "https://a")]));
    let first = action(
        &mut state,
        json!({ "saveMcpServer": server("b", "https://b") }),
    );
    let save = find(&first, is_save);
    let queued = action(&mut state, json!({ "deleteMcpServer": "a" }));
    assert!(queued.iter().all(|e| !is_save(e)), "the delete waits");
    assert_eq!(ids(&state), ["a", "b"]);

    let failed = McpError {
        message: "disk full".into(),
        code: None,
    };
    let next = apply(&mut state, Event::McpCommandFailed(save, failed));
    assert_eq!(
        next.iter().filter(|e| is_save(e)).count(),
        1,
        "the delete runs once the save settled"
    );
    assert_eq!(
        ids(&state),
        Vec::<String>::new(),
        "the rollback ran first, then the delete"
    );
}

#[test]
fn a_probe_that_outlived_an_edit_probes_again() {
    let mut state = registry(json!([server("a", "https://a")]));
    let probe = find(
        &action(&mut state, json!({ "probeMcpServer": "a" })),
        is_probe,
    );
    state.mcp_servers[0].insert("url".into(), json!("https://moved"));
    let result = json!({ "status": "connected", "tools": [] });
    let again = apply(&mut state, Event::McpCommandDone(probe, result));
    assert!(again.iter().any(is_probe), "the edited config is probed");
    assert!(
        state.mcp_probe_status.is_empty(),
        "the stale result does not land"
    );
}

#[test]
fn a_probe_of_a_deleted_server_does_not_land() {
    let mut state = registry(json!([server("a", "https://a")]));
    let probe = find(
        &action(&mut state, json!({ "probeMcpServer": "a" })),
        is_probe,
    );
    let delete = find(
        &action(&mut state, json!({ "deleteMcpServer": "a" })),
        is_save,
    );
    apply(&mut state, Event::McpCommandDone(delete, Value::Null));
    let result = json!({ "status": "connected", "tools": [] });
    let fx = apply(&mut state, Event::McpCommandDone(probe, result));
    assert!(fx.is_empty());
    assert!(state.mcp_probe_status.is_empty() && state.mcp_probing.is_empty());
}

#[test]
fn a_sign_in_during_a_probe_reprobes_after_it() {
    let mut state = registry(json!([server("a", "https://a")]));
    let probe = find(
        &action(&mut state, json!({ "probeMcpServer": "a" })),
        is_probe,
    );
    let start = find(
        &action(&mut state, json!({ "connectMcpOAuth": ["p1", "a"] })),
        is_start,
    );
    let signed_in = apply(&mut state, Event::McpCommandDone(start, Value::Null));
    assert!(!signed_in.iter().any(is_probe), "one probe at a time");
    let result = json!({ "status": "authRequired", "tools": [] });
    let again = apply(&mut state, Event::McpCommandDone(probe, result));
    assert!(again.iter().any(is_probe), "the stale result probes again");
}

#[test]
fn a_late_answer_of_an_aborted_flow_is_ignored() {
    let mut state = registry(json!([server("a", "https://a")]));
    let old = find(
        &action(&mut state, json!({ "connectMcpOAuth": ["p1", "a"] })),
        is_start,
    );
    let cancelled = action(&mut state, json!({ "cancelMcpOAuth": "a" }));
    assert!(matches!(cancelled[0], Effect::AbortMcpOAuth { .. }));
    let fx = apply(&mut state, Event::McpCommandDone(old, Value::Null));
    assert!(fx.is_empty(), "the model ended the flow at the abort");
    assert!(state.mcp_oauth_connected.is_empty());
}

#[test]
fn the_shared_watch_starts_once() {
    let mut state = registry(json!([]));
    let mut watches = 0;
    for _ in 0..2 {
        let load = action(&mut state, json!({ "loadMcpServers": [] }));
        let fx = apply(
            &mut state,
            Event::McpCommandDone(load[0].clone(), json!([])),
        );
        watches += fx
            .iter()
            .filter(|e| matches!(e, Effect::WatchSharedMcpServers))
            .count();
    }
    assert_eq!(watches, 1);
}
