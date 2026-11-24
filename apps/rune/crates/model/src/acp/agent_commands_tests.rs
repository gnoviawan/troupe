//! A click that shares an authenticate in flight: the recorder's mocks settle
//! each command within its step, so two clicks never overlap there.

use serde_json::json;

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
        server_history: true,
        desktop: false,
    };
    step(state, event, &mut env)
}

fn click(state: &mut State) -> Vec<Effect> {
    let event = serde_json::from_value(json!({ "authenticateAgent": ["agent-1", "oauth"] }))
        .expect("an event");
    apply(state, event)
}

fn rejections(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::ActionRejected { message } if message == "Error: denied"))
        .count()
}

#[test]
fn a_click_sharing_a_sign_in_fails_with_it() {
    let mut state = State::default();
    state.agents.insert(
        "agent-1".into(),
        json!({ "id": "agent-1", "authMethods": [{ "id": "oauth", "type": "agent" }] }),
    );
    let first = click(&mut state);
    let shared = click(&mut state);
    assert_eq!(shared, Vec::new(), "the second click sends nothing");
    let sent = first
        .into_iter()
        .find(|effect| matches!(effect, Effect::Authenticate { .. }))
        .expect("the first click sends authenticate");

    let failed = apply(&mut state, Event::AgentCommandFailed(sent, "denied".into()));
    assert_eq!(rejections(&failed), 2, "both clicks fail: {failed:?}");
    let again = click(&mut state);
    assert!(
        again
            .iter()
            .any(|effect| matches!(effect, Effect::Authenticate { .. })),
        "a click after the flight starts a new one"
    );
    assert_eq!(state.book.authenticated_agents, Default::default());
}
