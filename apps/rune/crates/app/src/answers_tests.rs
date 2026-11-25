use client::ws::{Command, Output};
use model::acp::Event;
use rune::Runtime;
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use crate::State;
use crate::tests::{present, tap};

fn chat_at(size: (f32, f32)) -> Runtime<State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(Output::Reply { call: 7, result: Ok(None) });
    s.apply(Output::Reply { call: 8, result: Ok(None) });
    let mut rt = Runtime::new(s, crate::view, size, rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.update_if(|s, _| {
        s.install_new_chat("chat-1", "agent-1");
        s.workspace.step(model::workspace::Event::AddAgentChatTab("chat-1".into(), None));
        true
    });
    rt.frame(0.05);
    rt
}

fn ask(rt: &mut Runtime<State>, event: Value) {
    rt.update_if(|s, _| {
        s.acp_step(serde_json::from_value::<Event>(event.clone()).expect("an event"));
        true
    });
    rt.frame(0.1);
    rt.frame(0.15);
}

fn permission() -> Value {
    json!({ "_onPermissionRequest": [{
        "agentId": "agent-1",
        "sessionId": "chat-1",
        "requestId": "perm-1",
        "toolCall": { "toolCallId": "call-1", "title": "Run the router test suite" },
        "options": [
            { "optionId": "allow-once", "name": "Allow", "kind": "allow_once" },
            { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" },
        ],
    }, 5] })
}

fn sent(s: &State, kind: WsRequestType) -> Vec<Value> {
    s.sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: k, payload, .. } if *k == kind => Some(payload.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn allow_and_reject_answer_the_permission_prompt_in_the_composer() {
    let mut rt = chat_at((1280.0, 800.0));
    ask(&mut rt, permission());
    assert!(present(&rt, "permission-perm-1"), "the prompt shows in the composer");
    tap(&mut rt, "permission-option-allow-once");
    assert_eq!(
        sent(&rt.state, WsRequestType::RespondPermission),
        [json!({ "agentId": "agent-1", "requestId": "perm-1", "optionId": "allow-once" })]
    );
    assert!(!present(&rt, "permission-perm-1"), "the prompt goes once answered");

    ask(&mut rt, permission());
    tap(&mut rt, "permission-option-reject-once");
    assert_eq!(sent(&rt.state, WsRequestType::RespondPermission)[1]["optionId"], "reject-once");
}

#[test]
fn a_refused_permission_answer_brings_the_prompt_back_with_a_toast() {
    let mut rt = chat_at((1280.0, 800.0));
    ask(&mut rt, permission());
    tap(&mut rt, "permission-option-allow-once");
    let call = rt.state.sync.last_call();
    rt.update_if(|s, _| {
        let error = WsError { code: "closed".into(), message: "socket closed".into(), details: None };
        s.apply(Output::Reply { call, result: Err(error) });
        true
    });
    rt.frame(0.2);
    assert!(present(&rt, "permission-perm-1"), "the prompt is back");
    assert_eq!(rt.state.toasts.visible().len(), 1, "the user hears the answer failed");
}

#[test]
fn a_question_replaces_the_composer_and_submits_the_pick() {
    let mut rt = chat_at((390.0, 844.0));
    ask(
        &mut rt,
        json!({ "_onQuestionRequest": [{
            "agentId": "agent-1",
            "sessionId": "chat-1",
            "questionId": "q-1",
            "question": "Which branch?",
            "options": [
                { "value": "main", "label": "main (Recommended)" },
                { "value": "feature", "label": "feature/router" },
            ],
        }, 6] }),
    );
    assert!(present(&rt, "question-q-1"));
    assert!(!present(&rt, "composer-chat-chat-1-tray"), "the question takes the composer's place");
    tap(&mut rt, "question-submit-q-1");
    assert!(sent(&rt.state, WsRequestType::AnswerQuestion).is_empty(), "Submit waits for a pick");
    tap(&mut rt, "question-option-1");
    tap(&mut rt, "question-submit-q-1");
    assert_eq!(
        sent(&rt.state, WsRequestType::AnswerQuestion),
        [json!({ "agentId": "agent-1", "questionId": "q-1", "values": ["feature"] })]
    );
    assert!(present(&rt, "composer-chat-chat-1-tray"), "the composer comes back");
}


fn dispatch(activity: &str, end: Option<&str>) -> Value {
    let mut record = json!({
        "id": "child-explore",
        "rootSessionId": "chat-1",
        "dispatchToolCallId": "termul-subagent:child-explore",
        "source": "acp-rfd",
        "title": "Explore the router",
        "background": false,
        "caps": { "stop": "session-cancel", "transcript": "live", "liveProgress": true, "usage": true },
        "activity": activity,
    });
    if let Some(reason) = end {
        record["end"] = json!({ "reason": reason });
        record["caps"]["stop"] = Value::Null;
    }
    json!({ "_onToolCall": [{
        "agentId": "agent-1",
        "sessionId": "chat-1",
        "toolCall": {
            "toolCallId": "termul-subagent:child-explore",
            "title": "Explore the router",
            "status": if end.is_some() { "completed" } else { "in_progress" },
            "_meta": { "termul/subagent": record },
        },
    }, 20] })
}

#[test]
fn the_subagent_panel_stops_a_running_subagent_through_the_host() {
    let mut rt = chat_at((1280.0, 800.0));
    ask(&mut rt, json!({ "_onUserPrompt": [{ "agentId": "agent-1", "sessionId": "chat-1", "turnId": "t1", "blocks": [{ "type": "text", "text": "matrix subagent" }] }, 19] }));
    ask(&mut rt, dispatch("running", None));
    assert!(present(&rt, "subagent-row-termul-subagent:child-explore"), "the dispatch shows as a subagent row");
    tap(&mut rt, "subagent-row-termul-subagent:child-explore");
    assert!(present(&rt, "subagent-panel-termul-subagent:child-explore"), "the row opens the panel");
    tap(&mut rt, "subagent-stop-termul-subagent:child-explore");
    assert_eq!(
        sent(&rt.state, WsRequestType::StopSubagent),
        [json!({ "agentId": "agent-1", "rootSessionId": "chat-1", "subagentId": "child-explore" })]
    );
    assert!(rt.state.chat.reader.is_stopping("child-explore"));
    ask(&mut rt, dispatch("idle", Some("cancelled")));
    assert!(!present(&rt, "subagent-stop-termul-subagent:child-explore"), "a stopped subagent offers no Stop");
}

/// A key the app's own handler gets (no field has focus while a question shows).
fn press(rt: &mut Runtime<State>, key: rune::Key) {
    rt.update_if(|s, tx| crate::keys::on_key(s, &rune::KeyEvent { key: key.clone(), mods: rune::Mods::default() }, tx));
    rt.frame(0.3);
}

#[test]
fn digits_pick_and_return_answers_the_question_on_screen() {
    use rune::Key;
    let mut rt = chat_at((1280.0, 800.0));
    ask(
        &mut rt,
        json!({ "_onQuestionRequest": [{
            "agentId": "agent-1",
            "sessionId": "chat-1",
            "questionId": "q-1",
            "question": "Which branch?",
            "options": [{ "value": "main", "label": "main" }, { "value": "feature", "label": "feature/router" }],
        }, 6] }),
    );
    for key in [Key::Text("2".into()), Key::Enter] {
        press(&mut rt, key);
    }
    assert_eq!(
        sent(&rt.state, WsRequestType::AnswerQuestion),
        [json!({ "agentId": "agent-1", "questionId": "q-1", "values": ["feature"] })]
    );
}

#[test]
fn escape_cancels_the_question_only_from_inside_it() {
    use rune::Key;
    let mut rt = chat_at((1280.0, 800.0));
    ask(
        &mut rt,
        json!({ "_onQuestionRequest": [{
            "agentId": "agent-1", "sessionId": "chat-1", "questionId": "q-1",
            "question": "Which branch?", "options": [{ "value": "main", "label": "main" }],
        }, 6] }),
    );
    press(&mut rt, Key::Escape);
    assert!(sent(&rt.state, WsRequestType::AnswerQuestion).is_empty(), "Escape outside the question keeps it");
    tap(&mut rt, "question-option-0");
    rt.key(rune::KeyEvent { key: Key::Escape, mods: rune::Mods::default() });
    rt.frame(0.3);
    assert_eq!(
        sent(&rt.state, WsRequestType::AnswerQuestion),
        [json!({ "agentId": "agent-1", "questionId": "q-1", "values": null })]
    );
}
