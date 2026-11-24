use serde_json::{Value, json};

use super::answer::Answer;
use super::{Effect, Env, Event, State, step};
use crate::Ids;

struct NoIds;

impl Ids for NoIds {
    fn next_id(&mut self) -> String {
        unreachable!("these steps draw no ids")
    }
}

fn apply(state: &mut State, event: Value) -> Vec<Effect> {
    let event: Event = serde_json::from_value(event).expect("an event");
    apply_event(state, event)
}

fn apply_event(state: &mut State, event: Event) -> Vec<Effect> {
    let mut ids = NoIds;
    let mut env = Env {
        now: 1_000.0,
        ids: &mut ids,
        server_history: true,
    };
    step(state, event, &mut env)
}

fn asked() -> State {
    let mut state = State::default();
    apply(
        &mut state,
        json!({ "_onPermissionRequest": [{
            "agentId": "agent-1",
            "sessionId": "chat-1",
            "requestId": "perm-1",
            "toolCall": { "toolCallId": "call-1", "title": "Run the tests" },
            "options": [{ "optionId": "allow-once", "name": "Allow", "kind": "allow_once" }],
        }, 5] }),
    );
    apply(
        &mut state,
        json!({ "_onQuestionRequest": [{
            "agentId": "agent-1",
            "sessionId": "chat-1",
            "questionId": "q-1",
            "question": "Which branch?",
            "options": [{ "value": "main", "label": "main" }],
        }, 6] }),
    );
    state
}

fn the_answer(effects: Vec<Effect>) -> Answer {
    let answers: Vec<Answer> = effects
        .into_iter()
        .filter_map(|e| match e {
            Effect::Answer(answer) => Some(answer),
            _ => None,
        })
        .collect();
    assert_eq!(answers.len(), 1, "one answer goes out");
    answers.into_iter().next().expect("an answer")
}

#[test]
fn a_permission_answer_goes_out_once_with_its_option() {
    let mut state = asked();
    let answer = the_answer(apply(&mut state, json!({ "respondPermission": ["perm-1", "allow-once"] })));
    let Answer::Permission { pending, option_id } = answer else { panic!("a permission answer") };
    assert_eq!((pending.agent_id.as_str(), option_id.as_deref()), ("agent-1", Some("allow-once")));
    assert!(state.pending_permissions.is_empty());
    let again = apply(&mut state, json!({ "respondPermission": ["perm-1", "allow-once"] }));
    assert_eq!(again, Vec::new(), "a second click sends nothing");
}

#[test]
fn a_refused_answer_brings_the_prompt_back_unless_already_resolved() {
    let mut state = asked();
    let answer = the_answer(apply(&mut state, json!({ "respondPermission": ["perm-1", null] })));
    apply_event(&mut state, Event::AnswerFailed(answer.clone(), "socket closed".into(), Some("closed".into())));
    assert_eq!(state.pending_permissions.keys().collect::<Vec<_>>(), ["perm-1"]);

    let answer = the_answer(apply(&mut state, json!({ "respondPermission": ["perm-1", null] })));
    apply_event(&mut state, Event::AnswerFailed(answer, "gone".into(), Some("already_resolved".into())));
    assert!(state.pending_permissions.is_empty());
}

#[test]
fn a_question_answer_carries_the_values() {
    let mut state = asked();
    let answer = the_answer(apply(&mut state, json!({ "answerQuestion": ["q-1", ["main"]] })));
    let Answer::Question { pending, values } = answer.clone() else { panic!("a question answer") };
    assert_eq!((pending.question_id.as_str(), values), ("q-1", vec!["main".to_owned()]));
    assert!(state.pending_questions.is_empty());
    apply_event(&mut state, Event::AnswerFailed(answer, "refused".into(), None));
    assert_eq!(state.pending_questions.keys().collect::<Vec<_>>(), ["q-1"]);
}

#[test]
fn an_answer_for_an_unknown_prompt_sends_nothing() {
    let mut state = asked();
    assert_eq!(apply(&mut state, json!({ "respondPermission": ["perm-9", "allow-once"] })), Vec::new());
    assert_eq!(apply(&mut state, json!({ "answerQuestion": ["q-9", []] })), Vec::new());
    assert_eq!(apply(&mut state, json!({ "respondElicitation": ["e-9", "cancel"] })), Vec::new());
}
