//! The one-shots' timers, which the recorder's vectors never reach: the
//! run's overall time, a session that opens after it ran out, and the
//! dispose time.

use serde_json::{Value, json};

use super::{Effect, Env, Event, State, step};
use crate::Ids;

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

/// The selected `gemini` config with `agent-1` connected for `/work`.
fn live() -> State {
    serde_json::from_value(json!({
        "sessions": {}, "messages": {}, "toolCalls": {}, "plans": {}, "commands": {},
        "commandsSeq": {}, "sessionUsage": {}, "sessionIndex": [], "pendingPermissions": {},
        "pendingQuestions": {}, "pendingElicitations": {}, "pendingBrowserConsents": {},
        "agentStatus": { "agent-1": "connected" }, "turnEndNotices": {}, "promptQueues": {},
        "suppressQueueFlush": {}, "agentOptionsCache": {},
        "configToLiveAgent": { "gemini\u{0}/work": "agent-1" },
        "activeSessionId": null, "preparedSessions": {}, "agentSwitches": {},
        "agents": { "agent-1": { "id": "agent-1", "capabilities": {} } },
        "pendingSignIn": {}, "autoOpenBrowserUrl": {}, "launchingSessionIds": {},
        "discoveredSessions": {},
        "agentConfigs": [{ "id": "gemini", "name": "Gemini", "command": "g" }],
        "selectedAgentConfigId": "gemini",
        "book": { "seq": 0, "historyWatermarks": {} }
    }))
    .expect("a state")
}

fn take<F: Fn(&Effect) -> bool>(effects: &[Effect], wanted: F) -> Effect {
    effects
        .iter()
        .find(|effect| wanted(effect))
        .cloned()
        .expect("the effect is there")
}

fn overall(effects: &[Effect]) -> Effect {
    take(effects, |e| {
        matches!(e, Effect::StartOneShotTimer { cleanup: false, .. })
    })
}

fn kinds(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .map(|effect| {
            let value = serde_json::to_value(effect).expect("serializes");
            value["kind"].as_str().unwrap_or_default().to_owned()
        })
        .collect()
}

fn commit() -> Event {
    Event::GenerateCommitMessage("/work".into(), "diff".into(), 4)
}

#[test]
fn a_run_out_of_time_disposes_its_session_then_fails() {
    let mut state = live();
    let started = apply(&mut state, commit());
    let timer = overall(&started);
    let new_session = take(&started, |e| matches!(e, Effect::NewSession { .. }));
    let opened = apply(
        &mut state,
        Event::AgentCommandDone(new_session, json!({ "sessionId": "t1" })),
    );
    assert!(kinds(&opened).contains(&"dispatchPrompt".to_owned()));
    let timed_out = apply(&mut state, Event::OneShotTimerDue(timer));
    assert_eq!(
        kinds(&timed_out),
        [
            "log",
            "cancelPrompt",
            "disposeEphemeralSession",
            "startOneShotTimer"
        ]
    );
    let dispose = take(&timed_out, |e| {
        matches!(e, Effect::DisposeEphemeralSession { .. })
    });
    let answered = apply(&mut state, Event::AgentCommandDone(dispose, Value::Null));
    assert_eq!(
        answered,
        vec![Effect::ActionRejected {
            message: "Error: Commit message generation timed out".into()
        }]
    );
    assert!(state.sessions.is_empty());
}

#[test]
fn a_session_that_opens_after_the_timeout_is_disposed_without_an_answer() {
    let mut state = live();
    let started = apply(&mut state, commit());
    let new_session = take(&started, |e| matches!(e, Effect::NewSession { .. }));
    let timed_out = apply(&mut state, Event::OneShotTimerDue(overall(&started)));
    assert_eq!(kinds(&timed_out), ["log", "actionRejected"]);
    let late = apply(
        &mut state,
        Event::AgentCommandDone(new_session, json!({ "sessionId": "t9" })),
    );
    assert_eq!(
        kinds(&late),
        [
            "cancelPrompt",
            "disposeEphemeralSession",
            "startOneShotTimer"
        ]
    );
    assert!(state.sessions.contains_key("t9"));
    let dispose = take(&late, |e| {
        matches!(e, Effect::DisposeEphemeralSession { .. })
    });
    let failed = apply(
        &mut state,
        Event::AgentCommandFailed(dispose, "gone".into(), None),
    );
    assert_eq!(
        failed,
        vec![Effect::warn(
            "acp.generateCommitMessage.lateCleanup",
            "Failed to close late temporary ACP session t9: Error: gone".into()
        )]
    );
    assert!(state.sessions.is_empty());
}

#[test]
fn a_dispose_out_of_time_is_logged_and_the_run_answers() {
    let mut state = live();
    let started = apply(&mut state, commit());
    let new_session = take(&started, |e| matches!(e, Effect::NewSession { .. }));
    let opened = apply(
        &mut state,
        Event::AgentCommandDone(new_session, json!({ "sessionId": "t1" })),
    );
    assert!(kinds(&opened).contains(&"dispatchPrompt".to_owned()));
    let failed = apply(
        &mut state,
        serde_json::from_value(json!({ "_onToolCall": [{ "agentId": "agent-1", "sessionId": "t1", "toolCall": { "toolCallId": "c" } }] }))
            .expect("an event"),
    );
    let cleanup = take(&failed, |e| {
        matches!(e, Effect::StartOneShotTimer { cleanup: true, .. })
    });
    let answered = apply(&mut state, Event::OneShotTimerDue(cleanup));
    assert_eq!(
        answered,
        vec![
            Effect::warn(
                "acp.generateCommitMessage.cleanup",
                "Failed to dispose temporary ACP session t1: Error: Temporary ACP session disposal timed out".into()
            ),
            Effect::ActionRejected {
                message: "Error: The ACP agent attempted to use a tool".into()
            },
        ]
    );
    let again = apply(&mut state, Event::OneShotTimerDue(overall(&started)));
    assert_eq!(again, Vec::new());
}

#[test]
fn a_turn_that_ends_before_its_prompt_is_acknowledged_waits_for_it() {
    let mut state = live();
    let started = apply(&mut state, commit());
    let new_session = take(&started, |e| matches!(e, Effect::NewSession { .. }));
    let opened = apply(
        &mut state,
        Event::AgentCommandDone(new_session, json!({ "sessionId": "t1" })),
    );
    let Effect::DispatchPrompt { turn_id, .. } =
        take(&opened, |e| matches!(e, Effect::DispatchPrompt { .. }))
    else {
        unreachable!()
    };
    let chunk = json!({ "_onMessageChunk": [{ "agentId": "agent-1", "sessionId": "t1", "role": "agent", "content": { "type": "text", "text": "{\"summary\":\"Early\"}" } }] });
    apply(&mut state, serde_json::from_value(chunk).expect("an event"));
    let end = json!({ "_onPromptComplete": [{ "agentId": "agent-1", "sessionId": "t1", "stopReason": "end_turn" }] });
    let ended = apply(&mut state, serde_json::from_value(end).expect("an event"));
    assert_eq!(ended, Vec::new());
    let settled = json!({ "promptSettled": { "sessionId": "t1", "turnId": turn_id, "outcome": { "answered": { "receipt": "accepted" } } } });
    let sent = apply(
        &mut state,
        serde_json::from_value(settled).expect("an event"),
    );
    assert_eq!(
        kinds(&sent),
        [
            "log",
            "cancelPrompt",
            "disposeEphemeralSession",
            "startOneShotTimer"
        ]
    );
}
