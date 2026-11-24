use client::ws::{Command, Output};
use model::acp::Event;
use rune::Runtime;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::tests::{present, tap};

fn chat() -> Runtime<State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(Output::Reply { call: 7, result: Ok(None) });
    s.apply(Output::Reply { call: 8, result: Ok(None) });
    let mut rt = Runtime::new(s, crate::view, (1280.0, 800.0), rune::Env::new(true, 1.0));
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

fn prompt(rt: &mut Runtime<State>, text: &str) {
    ask(rt, json!({ "_onUserPrompt": [{ "agentId": "agent-1", "sessionId": "chat-1", "turnId": "t1", "content": [{ "type": "text", "text": text }] }, 2] }));
}

fn tool(rt: &mut Runtime<State>, call: Value) {
    ask(rt, json!({ "_onToolCall": [{ "agentId": "agent-1", "sessionId": "chat-1", "toolCall": call }, 3] }));
}

/// Frames until the transcript's rows have their heights and the end is in view again.
fn settle(rt: &mut Runtime<State>) {
    for n in 1..=20 {
        rt.frame(0.2 + f64::from(n) * 0.05);
    }
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
fn a_tool_call_opens_to_its_output_and_an_edit_shows_its_diff() {
    let mut rt = chat();
    prompt(&mut rt, "run the tests");
    tool(&mut rt, json!({ "toolCallId": "run-1", "title": "Run tests", "kind": "execute", "status": "completed", "rawInput": { "command": "bun test" }, "rawOutput": { "stdout": "6 passed" } }));
    assert!(present(&rt, "run-run-1-toggle"), "the turn's calls fold into a work run");
    assert!(!present(&rt, "tool-run-1-toggle"), "the run starts folded");
    tap(&mut rt, "run-run-1-toggle");
    settle(&mut rt);
    assert!(present(&rt, "tool-run-1-toggle"), "the call is a row");
    assert!(!present(&rt, "tool-run-1-detail"), "folded by default");
    tap(&mut rt, "tool-run-1-toggle");
    assert!(present(&rt, "tool-run-1-detail"), "a press shows the output");

    let old: String = (1..=12).map(|n| format!("l{n}\n")).collect();
    let new = old.replace("l8\n", "eight\n");
    tool(&mut rt, json!({ "toolCallId": "edit-1", "title": "Edit a.ts", "kind": "edit", "status": "completed", "locations": [{ "path": "/repo/a.ts" }],
        "content": [{ "type": "diff", "path": "/repo/a.ts", "oldText": old, "newText": new }] }));
    assert!(present(&rt, "tool-edit-1-diff-0"), "an edit draws its diff inline");
    assert!(!present(&rt, "tool-edit-1-toggle"), "a diff-only edit has no row");
    assert!(!rt.state.chat.reader.is_diff_open("tool-edit-1-diff-0"));
    settle(&mut rt);
    tap(&mut rt, "tool-edit-1-diff-0-rows");
    assert!(rt.state.chat.reader.is_diff_open("tool-edit-1-diff-0"), "a press on the rows shows the whole diff");
    settle(&mut rt);
    tap(&mut rt, "tool-edit-1-diff-0-head");
    let active = model::workspace::query(&rt.state.workspace.store, model::workspace::Query::GetActiveTab);
    assert!(matches!(active, model::workspace::Reply::Tab(Some(model::workspace::Tab::Editor { .. }))), "the header opens the file");
}

#[test]
fn the_plan_docks_over_the_transcript_and_folds() {
    let mut rt = chat();
    prompt(&mut rt, "plan it");
    ask(&mut rt, json!({ "_onPlanUpdate": { "sessionId": "chat-1", "plan": { "entries": [
        { "content": "Read the router module", "status": "completed", "priority": "medium" },
        { "content": "Write the tests", "status": "in_progress", "priority": "high" },
    ] } } }));
    assert!(present(&rt, "plan-chat-1"));
    let rows = |rt: &Runtime<State>| rt.elements().iter().filter(|e| e.label.as_deref() == Some("Plan, 1 of 2 tasks, task in progress")).count();
    assert_eq!(rows(&rt), 1, "the bar names the progress");
    assert!(!rt.state.chat.reader.plan_collapsed("chat-1", false, false), "an open plan starts open");
    tap(&mut rt, "plan-toggle-chat-1");
    assert!(rt.state.chat.reader.plan_collapsed("chat-1", false, false));
    assert!(rt.state.chat.reader.plan_collapsed("chat-1", true, false), "a finished plan closes");
}

#[test]
fn the_error_notice_shows_details_retries_and_dismisses() {
    let mut rt = chat();
    prompt(&mut rt, "hello");
    rt.update_if(|s, _| {
        let session = s.chat.acp.sessions.get_mut("chat-1").unwrap();
        session.last_error = Some(Some("Stream closed: NGHTTP2_INTERNAL_ERROR".into()));
        session.last_error_code = Some(Some(-32603.0));
        session.active_turn = false;
        session.open_turn_id = None;
        true
    });
    rt.frame(0.2);
    assert!(present(&rt, "chat-error-chat-1"));
    tap(&mut rt, "chat-error-chat-1-details");
    assert!(rt.state.chat.reader.error_details.contains("chat-1"));
    tap(&mut rt, "chat-error-chat-1-retry");
    assert_eq!(sent(&rt.state, WsRequestType::SendPrompt).len(), 1, "Retry sends the last turn again");
    assert!(!present(&rt, "chat-error-chat-1"), "the retried error hides");
}

#[test]
fn an_elicitation_form_checks_its_fields_then_answers() {
    let mut rt = chat();
    ask(
        &mut rt,
        json!({ "_onElicitationRequest": [{
            "agentId": "agent-1", "sessionId": "chat-1", "requestId": "el-1", "mode": "form", "message": "How many runs?",
            "fields": [
                { "name": "runs", "kind": "integer", "required": true, "title": "Runs", "options": [] },
                { "name": "branch", "kind": "enum", "required": false, "options": [{ "value": "main", "label": "main" }, { "value": "dev", "label": "dev" }] },
            ],
        }] }),
    );
    assert!(present(&rt, "elicitation-el-1"), "the form takes the composer's place");
    assert!(!present(&rt, "composer-chat-chat-1-tray"));
    tap(&mut rt, "elicitation-accept-el-1");
    assert!(sent(&rt.state, WsRequestType::RespondElicitation).is_empty(), "a missing required field stops Submit");
    assert_eq!(rt.state.chat.reader.form_of("el-1").and_then(|f| f.error.clone()), Some(("runs".into(), "Runs is required.".into())));
    rt.update_if(|s, _| {
        s.chat.reader.form("el-1").text("runs").text = "3".into();
        true
    });
    tap(&mut rt, "elicitation-field-branch-1");
    tap(&mut rt, "elicitation-accept-el-1");
    assert_eq!(
        sent(&rt.state, WsRequestType::RespondElicitation),
        [json!({ "agentId": "agent-1", "requestId": "el-1", "action": "accept", "content": { "runs": 3, "branch": "dev" } })]
    );
}
