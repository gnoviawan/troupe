//! Restore paths the recorder cannot time: its clock stands still within a
//! step, so the re-advertise poll and an interleaved read never settle there.

use serde_json::{Value, json};

use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct NoIds;

impl Ids for NoIds {
    fn next_id(&mut self) -> String {
        unreachable!("these steps draw no ids")
    }
}

fn select(id: &str, category: &str, current: &str, values: &[&str]) -> Value {
    json!({
        "id": id,
        "name": id,
        "category": category,
        "type": "select",
        "currentValue": current,
        "options": values.iter().map(|v| json!({ "value": v, "name": v })).collect::<Vec<_>>(),
    })
}

/// A live chat on `m1` whose agent config is `cfg-1`.
fn chat(models: Value, options: Value) -> State {
    let mut value = serde_json::to_value(State::default()).expect("a state");
    value["sessions"] = json!({ "s1": {
        "id": "s1",
        "agentId": "agent-1",
        "status": "active",
        "title": null,
        "activeTurn": false,
        "openTurnId": null,
        "lastError": null,
        "models": models,
        "configOptions": options,
    }});
    value["configToLiveAgent"] = json!({ "cfg-1\u{0}/work\u{0}p1": "agent-1" });
    serde_json::from_value(value).expect("a state")
}

fn native() -> State {
    chat(
        json!({
            "currentModelId": "m1",
            "availableModels": [{ "modelId": "m1", "name": "M1" }, { "modelId": "m2", "name": "M2" }],
        }),
        json!([select("effort", "thought_level", "low", &["low", "high"])]),
    )
}

fn apply(state: &mut State, event: Event) -> Vec<Effect> {
    let mut ids = NoIds;
    let mut env = Env {
        now: 1_000.0,
        ids: &mut ids,
        server_history: true,
        desktop: false,
    };
    step(state, event, &mut env)
}

fn event(value: Value) -> Event {
    serde_json::from_value(value).expect("an event")
}

fn find(effects: &[Effect], want: fn(&Effect) -> bool) -> Option<Effect> {
    effects.iter().find(|effect| want(effect)).cloned()
}

fn set_option(effect: &Effect) -> bool {
    matches!(effect, Effect::SetConfigOption { .. })
}

/// The value of every `SetConfigOption` in `effects`, as `id=value`.
fn sets(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SetConfigOption {
                config_id, value, ..
            } => Some(format!("{config_id}={value}")),
            _ => None,
        })
        .collect()
}

/// Picks `m2` with a high effort on the native route and acknowledges the
/// model, which leaves the pick waiting for the new effort choices.
fn waiting_pick(state: &mut State) -> Vec<Effect> {
    let fx = apply(
        state,
        event(json!({ "pickModel": ["s1", "m2", { "effort": "high" }] })),
    );
    let set = find(&fx, |e| matches!(e, Effect::SetModel { .. })).expect("the model is set");
    apply(state, Event::ControlSet(set, None))
}

#[test]
fn a_native_model_pick_waits_for_the_new_options_before_its_values() {
    let mut state = native();
    let acked = waiting_pick(&mut state);
    assert!(
        find(&acked, |e| matches!(e, Effect::WaitForReadvertisedOptions { delay_ms, .. } if *delay_ms == 1000.0)).is_some(),
        "the pick waits for the options: {acked:?}"
    );
    assert_eq!(sets(&acked), Vec::<String>::new());

    let readvertised = apply(
        &mut state,
        event(json!({ "_onConfigOptionsUpdate": {
            "sessionId": "s1",
            "configOptions": [select("effort", "thought_level", "low", &["low", "high", "max"])],
        }})),
    );
    assert_eq!(sets(&readvertised), vec!["effort=\"high\"".to_owned()]);
}

#[test]
fn a_native_model_pick_applies_its_values_when_the_wait_runs_out() {
    let mut state = native();
    let acked = waiting_pick(&mut state);
    let Some(Effect::WaitForReadvertisedOptions { run, .. }) = find(&acked, |e| {
        matches!(e, Effect::WaitForReadvertisedOptions { .. })
    }) else {
        panic!("the pick waits: {acked:?}");
    };
    let over = apply(&mut state, Event::ReadvertiseWaitOver(run));
    assert_eq!(sets(&over), vec!["effort=\"high\"".to_owned()]);
    assert_eq!(
        sets(&apply(&mut state, Event::ReadvertiseWaitOver(run))),
        Vec::<String>::new(),
        "a second timeout finds no waiting run"
    );
}

#[test]
fn a_newer_model_pick_wins_over_one_still_reading_the_saved_record() {
    let mut state = chat(
        Value::Null,
        json!([
            select("model", "model", "m1", &["m1", "m2", "m3"]),
            select("effort", "thought_level", "low", &["low", "high"]),
        ]),
    );
    let first = apply(&mut state, event(json!({ "pickModel": ["s1", "m2"] })));
    let second = apply(&mut state, event(json!({ "pickModel": ["s1", "m3"] })));
    let read = |fx: &[Effect]| {
        find(fx, |e| matches!(e, Effect::ReadComposerOptions { .. })).expect("a read")
    };
    let saved =
        json!({ "modelValues": { "m2": { "effort": "high" }, "m3": { "effort": "high" } } });
    let stale = apply(
        &mut state,
        Event::ComposerOptionsRead(read(&first), saved.clone()),
    );
    assert_eq!(sets(&stale), Vec::<String>::new(), "the older pick stops");

    let fresh = apply(&mut state, Event::ComposerOptionsRead(read(&second), saved));
    let model = find(&fresh, set_option).expect("the newer pick sets its model");
    assert_eq!(sets(&fresh), vec!["model=\"m3\"".to_owned()]);
    let values = apply(&mut state, Event::ControlSet(model, None));
    assert_eq!(sets(&values), vec!["effort=\"high\"".to_owned()]);
}

#[test]
fn a_value_moved_while_the_heal_reads_the_record_is_kept() {
    let options = json!([
        select("model", "model", "m1", &["m1", "m2"]),
        select("effort", "thought_level", "low", &["low", "high", "max"]),
    ]);
    let mut state = chat(Value::Null, options.clone());
    let started = apply(
        &mut state,
        event(json!({ "_onConfigOptionsUpdate": { "sessionId": "s1", "configOptions": options } })),
    );
    let read = find(&started, |e| {
        matches!(e, Effect::ReadComposerOptions { .. })
    })
    .expect("the heal reads the record");

    let user = apply(
        &mut state,
        event(json!({ "setConfigOption": ["s1", "effort", "high"] })),
    );
    let set = find(&user, set_option).expect("the user's set");
    apply(&mut state, Event::ControlSet(set, None));

    let healed = apply(
        &mut state,
        Event::ComposerOptionsRead(
            read,
            json!({ "modelId": "m1", "configValues": { "effort": "max" } }),
        ),
    );
    assert_eq!(
        sets(&healed),
        Vec::<String>::new(),
        "the user's change wins"
    );
}
