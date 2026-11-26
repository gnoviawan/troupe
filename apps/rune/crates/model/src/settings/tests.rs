use serde_json::{Value, json};

use super::{Effect, Event, HostRead, Read, State, Timeout, step};

fn loaded(data: Value) -> State {
    let mut state = State::default();
    step(
        &mut state,
        Event::Loaded {
            read: Read::Found(data),
            system_dark: true,
        },
    );
    state
}

#[test]
fn a_stored_system_appearance_takes_the_os_one() {
    let mut state = State::default();
    let effects = step(
        &mut state,
        Event::Loaded {
            read: Read::Found(json!({ "appearanceMode": "system" })),
            system_dark: false,
        },
    );
    assert_eq!(state.str("appearanceMode"), "light");
    assert!(matches!(
        &effects[0],
        Effect::Write {
            immediate: false,
            ..
        }
    ));
}

#[test]
fn a_falsy_record_reads_as_a_first_run() {
    let mut state = State::default();
    step(
        &mut state,
        Event::Loaded {
            read: Read::Found(Value::Null),
            system_dark: true,
        },
    );
    assert_eq!(state.host_read, HostRead::Missing);
    assert_eq!(state.str("colorTheme"), "termul");
}

#[test]
fn a_newer_choice_voids_the_boot_reads_window() {
    let mut state = loaded(json!({ "appearanceMode": "dark" }));
    step(&mut state, Event::SetAgentIdleWindow(json!(900)));
    let live = super::IdleWindow {
        secs: 3600.0,
        source: "setting".into(),
    };
    assert!(step(&mut state, Event::IdleWindowRead(Ok(live))).is_empty());
    assert_eq!(state.get("agentIdleWindowSecs"), &json!(900));
}

#[test]
fn an_older_idle_push_is_not_current_after_a_newer_choice() {
    let mut state = loaded(json!({ "appearanceMode": "dark" }));
    let first = step(&mut state, Event::SetAgentIdleWindow(json!(900)));
    step(&mut state, Event::SetAgentIdleWindow(json!(3600)));
    let Some(Effect::PushIdleWindow { epoch, .. }) = first.last() else {
        panic!("no push: {first:?}");
    };
    assert!(!state.idle_push_is_current(*epoch));
}

#[test]
fn a_timeout_choice_writes_then_pushes() {
    let mut state = loaded(json!({ "appearanceMode": "dark" }));
    let effects = step(
        &mut state,
        Event::SetTimeout(Timeout::TurnTimeout, json!(3600)),
    );
    assert_eq!(state.get("acpTurnTimeoutSecs"), &json!(3600));
    assert!(matches!(
        &effects[0],
        Effect::Write {
            immediate: false,
            ..
        }
    ));
    assert_eq!(
        effects[1],
        Effect::PushTimeout {
            timeout: Timeout::TurnTimeout,
            secs: json!(3600)
        }
    );
}
