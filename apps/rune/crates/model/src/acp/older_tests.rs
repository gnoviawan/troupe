//! `isPersistedTwin` on the cases the vectors' plain prompts do not reach,
//! and a load whose session is dropped before its read answers.

use serde_json::{Value, json};

use super::older::is_persisted_twin;
use super::types::ChatMessage;
use super::{Effect, Env, Event, State, step};
use crate::Ids;

fn bubble(role: &str, blocks: Value, streaming: bool) -> ChatMessage {
    serde_json::from_value(json!({
        "id": "m", "role": role, "blocks": blocks, "streaming": streaming, "timestamp": 1
    }))
    .expect("a message")
}

fn said(role: &str, text: &str) -> ChatMessage {
    bubble(role, json!([{ "type": "text", "text": text }]), false)
}

#[test]
fn a_twin_has_the_same_role() {
    assert!(is_persisted_twin(
        &said("user", "hi"),
        &said("user", "hi"),
        false
    ));
    assert!(!is_persisted_twin(
        &said("agent", "hi"),
        &said("user", "hi"),
        false
    ));
}

#[test]
fn a_twin_compares_its_text_trimmed() {
    assert!(is_persisted_twin(
        &said("agent", "hi \n"),
        &said("agent", " hi"),
        false
    ));
}

#[test]
fn a_user_twin_carries_the_same_attachments() {
    let chip = |name: &str| json!([{ "type": "text", "text": "see" }, { "type": "resource_link", "name": name, "uri": "file:///a" }]);
    let persisted = bubble("user", chip("a.rs"), false);
    assert!(is_persisted_twin(
        &persisted,
        &bubble("user", chip("a.rs"), false),
        false
    ));
    assert!(!is_persisted_twin(
        &persisted,
        &bubble("user", chip("b.rs"), false),
        false
    ));
    assert!(!is_persisted_twin(&persisted, &said("user", "see"), false));
    // Other roles never carry chips: the text decides.
    assert!(is_persisted_twin(
        &bubble("agent", chip("a.rs"), false),
        &said("agent", "see"),
        false
    ));
}

#[test]
fn twins_without_text_compare_their_blocks() {
    let image = |mime: &str| json!([{ "type": "image", "mimeType": mime, "data": "x" }]);
    let persisted = bubble("agent", image("image/png"), false);
    assert!(is_persisted_twin(
        &persisted,
        &bubble("agent", image("image/png"), false),
        false
    ));
    assert!(!is_persisted_twin(
        &persisted,
        &bubble("agent", image("image/gif"), false),
        false
    ));
}

#[test]
fn a_streaming_bubble_folds_its_prefix_only_at_the_seam() {
    let persisted = said("agent", "partial answer");
    let live = bubble(
        "agent",
        json!([{ "type": "text", "text": "partial" }]),
        true,
    );
    assert!(is_persisted_twin(&persisted, &live, true));
    assert!(!is_persisted_twin(&persisted, &live, false));
    let settled = said("agent", "partial");
    assert!(!is_persisted_twin(&persisted, &settled, true));
}

struct Counter(u64);

impl Ids for Counter {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
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

#[test]
fn a_dropped_session_lets_its_next_load_read_again() {
    let mut state: State = serde_json::from_value(json!({
        "sessions": {}, "messages": { "s1": [] }, "toolCalls": {}, "plans": {}, "commands": {},
        "commandsSeq": {}, "sessionUsage": {}, "sessionIndex": [], "pendingPermissions": {},
        "pendingQuestions": {}, "pendingElicitations": {}, "pendingBrowserConsents": {},
        "agentStatus": {}, "turnEndNotices": {}, "promptQueues": {}, "suppressQueueFlush": {},
        "agentOptionsCache": {}, "configToLiveAgent": {}, "activeSessionId": null,
        "preparedSessions": {}, "agentSwitches": {}, "agents": {}, "pendingSignIn": {},
        "autoOpenBrowserUrl": {}, "launchingSessionIds": {}, "discoveredSessions": {},
        "book": { "seq": 0, "historyWatermarks": {} }
    }))
    .expect("a state");
    let reads = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::ReadSessionPayload { .. }))
            .count()
    };
    let load = || Event::LoadOlderMessages("s1".into(), 2);
    assert_eq!(reads(&apply(&mut state, load())), 1);
    assert_eq!(reads(&apply(&mut state, load())), 0, "one load at a time");
    apply(&mut state, Event::DropSessionTranscript("s1".into()));
    assert_eq!(reads(&apply(&mut state, load())), 1);
}
