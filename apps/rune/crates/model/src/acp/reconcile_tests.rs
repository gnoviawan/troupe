//! What the recorder's vectors cannot reach: the resend ladder's timers, a
//! reconnect holding it, a resend still on the wire at Stop, and an echo or
//! a turn end that lands before the lost reply.

use serde_json::{Value, json};

use super::prompt::{DispatchOutcome, PromptSettled};
use super::transcript::UserPromptEvent;
use super::types::SessionStatus;
use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct Counter(u64);

impl Ids for Counter {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

struct Chat {
    state: State,
    ids: Counter,
}

impl Chat {
    /// Session `s1` on the connected `agent-1`, idle.
    fn new() -> Self {
        let state = serde_json::from_value(json!({
            "sessions": { "s1": {
                "id": "s1", "agentId": "agent-1", "cwd": "/work", "status": "active",
                "title": null, "activeTurn": false, "openTurnId": null, "createdAt": 1
            } },
            "messages": { "s1": [] }, "toolCalls": {}, "plans": {}, "commands": {},
            "commandsSeq": {}, "sessionUsage": {}, "sessionIndex": [{ "id": "s1" }],
            "pendingPermissions": {}, "pendingQuestions": {}, "pendingElicitations": {},
            "pendingBrowserConsents": {}, "agentStatus": { "agent-1": "connected" },
            "turnEndNotices": {}, "promptQueues": {}, "suppressQueueFlush": {},
            "agentOptionsCache": {}, "configToLiveAgent": {}, "activeSessionId": "s1",
            "preparedSessions": {}, "agentSwitches": {}, "agents": {}, "pendingSignIn": {},
            "autoOpenBrowserUrl": {}, "launchingSessionIds": {}, "discoveredSessions": {},
            "book": { "seq": 0, "historyWatermarks": {} }
        }))
        .expect("a state");
        Self {
            state,
            ids: Counter(0),
        }
    }

    fn apply(&mut self, event: Event) -> Vec<Effect> {
        let mut env = Env {
            now: 1_000.0,
            ids: &mut self.ids,
            server_history: false,
        };
        step(&mut self.state, event, &mut env)
    }

    fn settle(&mut self, outcome: DispatchOutcome) -> Vec<Effect> {
        self.apply(Event::PromptSettled(PromptSettled {
            session_id: "s1".into(),
            turn_id: "id-1".into(),
            outcome,
        }))
    }

    /// Send a prompt whose reply is lost; the ladder's first timer.
    fn send_lost(&mut self) -> Vec<Effect> {
        self.apply(Event::SendPrompt("s1".into(), "hello".into()));
        self.settle(lost())
    }

    fn published(&self) -> Option<Value> {
        self.state.prompt_command_states.get("s1").cloned()
    }
}

fn lost() -> DispatchOutcome {
    DispatchOutcome::Uncertain {
        message: "connection lost".into(),
        outcome_unknown: false,
    }
}

fn unknown() -> DispatchOutcome {
    DispatchOutcome::Uncertain {
        message: "Outcome unknown".into(),
        outcome_unknown: true,
    }
}

/// The ladder's timer among `effects`: its delay and generation.
fn timer(effects: &[Effect]) -> Option<(f64, u64)> {
    effects.iter().find_map(|effect| match effect {
        Effect::PromptResendAfter {
            delay_ms,
            generation,
            ..
        } => Some((*delay_ms, *generation)),
        _ => None,
    })
}

fn resent(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::DispatchPrompt { turn_id, .. } if turn_id == "id-1"))
}

fn cancels(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::CancelPrompt { .. }))
        .count()
}

fn due(chat: &mut Chat, generation: u64) -> Vec<Effect> {
    chat.apply(Event::PromptResendDue("s1".into(), generation))
}

#[test]
fn the_ladder_resends_the_same_turn_and_backs_off_to_its_cap() {
    let mut chat = Chat::new();
    let (mut delay, mut generation) = timer(&chat.send_lost()).expect("the first tick");
    let mut delays = vec![delay];
    for attempt in 2..=6 {
        let effects = due(&mut chat, generation);
        assert!(resent(&effects), "tick {attempt} resends id-1");
        assert_eq!(chat.published().unwrap()["attempts"], json!(attempt - 1));
        (delay, generation) = timer(&chat.settle(lost())).expect("the next tick");
        assert_eq!(chat.published().unwrap()["attempts"], json!(attempt));
        assert_eq!(chat.published().unwrap()["checking"], json!(false));
        delays.push(delay);
    }
    assert_eq!(
        delays,
        [2_000.0, 4_000.0, 8_000.0, 15_000.0, 15_000.0, 15_000.0]
    );
}

#[test]
fn a_tick_from_an_earlier_timer_does_nothing() {
    let mut chat = Chat::new();
    let (_, first) = timer(&chat.send_lost()).expect("a tick");
    chat.apply(Event::RecheckPromptCommand("s1".into()));
    let (_, second) = timer(&chat.settle(lost())).expect("a new tick");
    assert_ne!(first, second);
    assert!(due(&mut chat, first).is_empty());
    assert!(resent(&due(&mut chat, second)));
}

#[test]
fn a_reconnect_holds_the_ladder_and_its_recovery_resends() {
    let mut chat = Chat::new();
    let (_, generation) = timer(&chat.send_lost()).expect("a tick");
    chat.apply(Event::TransportReconnecting(true));
    let held = due(&mut chat, generation);
    assert!(!resent(&held));
    assert_eq!(timer(&held).map(|(delay, _)| delay), Some(2_000.0));
    chat.apply(Event::TransportReconnecting(false));
    let recovered = chat.apply(Event::ResendPendingPrompts(None));
    assert!(resent(&recovered));
    // One resend at a time: a second recovery waits for the first.
    assert!(!resent(
        &chat.apply(Event::ResendPendingPrompts(Some("s1".into())))
    ));
}

#[test]
fn an_unknown_outcome_goes_out_again_only_when_the_user_checks() {
    let mut chat = Chat::new();
    chat.apply(Event::SendPrompt("s1".into(), "hello".into()));
    let held = chat.settle(unknown());
    assert_eq!(timer(&held), None);
    assert_eq!(chat.published().unwrap()["outcomeUnknown"], json!(true));
    assert!(!resent(&chat.apply(Event::ResendPendingPrompts(None))));
    let checked = chat.apply(Event::RecheckPromptCommand("s1".into()));
    assert!(resent(&checked));
    assert_eq!(chat.published().unwrap()["phase"], json!("pending"));
    assert_eq!(chat.published().unwrap()["checking"], json!(true));
}

#[test]
fn stop_during_a_resend_cancels_again_once_it_settles() {
    let mut chat = Chat::new();
    chat.send_lost();
    assert!(resent(
        &chat.apply(Event::RecheckPromptCommand("s1".into()))
    ));
    let stopped = chat.apply(Event::CancelPrompt("s1".into()));
    assert_eq!(cancels(&stopped), 1);
    assert_eq!(chat.published(), None);
    let settled = chat.settle(DispatchOutcome::Answered(json!({ "receipt": "accepted" })));
    assert_eq!(
        cancels(&settled),
        1,
        "the turn the resend started is cancelled"
    );
    assert!(!resent(
        &chat.apply(Event::RecheckPromptCommand("s1".into()))
    ));
}

#[test]
fn an_echo_before_the_lost_reply_accepts_the_turn() {
    let mut chat = Chat::new();
    chat.apply(Event::SendPrompt("s1".into(), "hello".into()));
    chat.apply(Event::UserPrompt(
        UserPromptEvent {
            session_id: "s1".into(),
            turn_id: Some("id-1".into()),
            content: Some(vec![json!({ "type": "text", "text": "hello" })]),
        },
        Some(3.0),
    ));
    let settled = chat.settle(lost());
    assert_eq!(timer(&settled), None);
    assert_eq!(chat.published(), None);
    assert!(settled.iter().any(|effect| matches!(
        effect,
        Effect::Log { message, .. } if message.contains("lost its reply after its user_prompt echo")
    )));
}

#[test]
fn a_turn_that_moved_on_before_the_lost_reply_waits_for_nothing() {
    let mut chat = Chat::new();
    chat.apply(Event::SendPrompt("s1".into(), "hello".into()));
    let session = chat.state.sessions.get_mut("s1").unwrap();
    session.open_turn_id = None;
    session.active_turn = false;
    assert!(chat.settle(lost()).is_empty());
    assert_eq!(chat.published(), None);
}

#[test]
fn a_tick_after_the_chat_closed_drops_the_command() {
    let mut chat = Chat::new();
    let (_, generation) = timer(&chat.send_lost()).expect("a tick");
    chat.state.sessions.get_mut("s1").unwrap().status = SessionStatus::Closed;
    assert!(!resent(&due(&mut chat, generation)));
    assert_eq!(chat.published(), None);
}

#[test]
fn a_resend_whose_command_was_confirmed_meanwhile_is_ignored() {
    let mut chat = Chat::new();
    chat.send_lost();
    chat.apply(Event::RecheckPromptCommand("s1".into()));
    chat.apply(Event::UserPrompt(
        UserPromptEvent {
            session_id: "s1".into(),
            turn_id: Some("id-1".into()),
            content: Some(vec![json!({ "type": "text", "text": "hello" })]),
        },
        Some(3.0),
    ));
    assert_eq!(chat.published(), None);
    assert!(chat.settle(lost()).is_empty());
    assert_eq!(chat.published(), None);
}

#[test]
fn a_check_that_comes_back_unknown_holds_without_a_timer() {
    let mut chat = Chat::new();
    chat.send_lost();
    chat.apply(Event::RecheckPromptCommand("s1".into()));
    let held = chat.settle(unknown());
    assert_eq!(timer(&held), None);
    assert_eq!(chat.published().unwrap()["outcomeUnknown"], json!(true));
}

#[test]
fn a_late_terminator_for_an_abandoned_turn_does_not_end_the_chat_turn() {
    let mut chat = Chat::new();
    chat.send_lost();
    chat.apply(Event::CancelPrompt("s1".into()));
    let late = chat.apply(Event::PromptComplete(
        serde_json::from_value(json!({
            "agentId": "agent-1", "sessionId": "s1", "stopReason": "end_turn", "turnId": "id-1"
        }))
        .expect("an event"),
        None,
    ));
    assert!(
        !late
            .iter()
            .any(|effect| matches!(effect, Effect::EndTurn(_))),
        "the abandoned turn id already ended"
    );
}

#[test]
fn a_lost_reply_for_a_wire_turn_this_client_no_longer_runs_waits_for_nothing() {
    let mut chat = Chat::new();
    chat.apply(Event::SendPrompt("s1".into(), "hello".into()));
    chat.state.book.active_wire_turns.remove("s1");
    assert!(chat.settle(lost()).is_empty());
    assert_eq!(chat.published(), None);
}
