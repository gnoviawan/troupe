use client::ws::{Command, Output};
use model::acp::Event;
use rune::Runtime;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::tests::{present, tap};

fn chat_with_options() -> Runtime<State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(Output::Reply { call: 7, result: Ok(None) });
    s.apply(Output::Reply { call: 8, result: Ok(None) });
    let mut rt = Runtime::new(s, crate::view, (1280.0, 800.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.update_if(|s, _| {
        s.install_new_chat("chat-1", "agent-1");
        let created = json!({
            "agentId": "agent-1",
            "sessionId": "chat-1",
            "modes": { "currentModeId": "default", "availableModes": [
                { "id": "default", "name": "Default", "description": "Ask before edits" },
                { "id": "acceptEdits", "name": "Accept edits", "description": "Edit without asking" },
            ] },
            "configOptions": [{ "id": "model", "name": "Model", "category": "model", "type": "select",
                "currentValue": "matrix-large",
                "options": [{ "value": "matrix-large", "name": "Matrix Large" }, { "value": "matrix-fast", "name": "Matrix Fast" }] }],
        });
        s.acp_step(Event::SessionCreated(serde_json::from_value(created).expect("created")));
        s.workspace.step(model::workspace::Event::AddAgentChatTab("chat-1".into(), None));
        true
    });
    rt.frame(0.05);
    rt
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

fn reply(rt: &mut Runtime<State>, result: Value) {
    let call = rt.state.sync.last_call();
    rt.update_if(|s, _| {
        s.apply(Output::Reply { call, result: Ok(Some(result.clone())) });
        true
    });
    rt.frame(0.2);
}

#[test]
fn the_mode_chip_switches_the_chat_mode_through_the_model() {
    let mut rt = chat_with_options();
    assert!(present(&rt, "composer-chat-chat-1-mode"), "the chat shows its mode chip");
    tap(&mut rt, "composer-chat-chat-1-mode");
    tap(&mut rt, "composer-chat-chat-1-mode-acceptEdits");
    assert_eq!(
        sent(&rt.state, WsRequestType::SetMode),
        [json!({ "agentId": "agent-1", "sessionId": "chat-1", "modeId": "acceptEdits" })]
    );
    reply(&mut rt, Value::Null);
    let modes = &rt.state.chat.acp.sessions["chat-1"].rest["modes"];
    assert_eq!(modes["currentModeId"], "acceptEdits", "the acknowledged mode is the chat's");
}

#[test]
fn the_chat_chip_names_the_model_and_its_menu_picks_another() {
    let mut rt = chat_with_options();
    let host = crate::composer::Host::Chat("chat-1".into());
    assert_eq!(rt.state.agent_label(&host).as_deref(), Some("Matrix Large"));
    rt.update_if(|s, tx| {
        s.composer_pick_agent(&host, "matrix-fast", tx);
        true
    });
    rt.frame(0.1);
    reply(&mut rt, Value::Null);
    assert_eq!(
        sent(&rt.state, WsRequestType::SetConfigOption),
        [json!({ "agentId": "agent-1", "sessionId": "chat-1", "configId": "model", "valueId": "matrix-fast" })],
        "a pick reads the saved picks, then sets the model option"
    );
}

#[test]
fn the_context_ring_shows_once_the_conversation_uses_the_window() {
    let mut rt = chat_with_options();
    let usage = |used: f64| json!({ "_onUsageUpdate": { "agentId": "agent-1", "sessionId": "chat-1", "used": used, "size": 200000.0 } });
    rt.update_if(|s, _| {
        s.acp_step(serde_json::from_value(json!({ "_onUserPrompt": [{ "agentId": "agent-1", "sessionId": "chat-1", "turnId": "t1", "blocks": [{ "type": "text", "text": "hi" }] }, 3] })).expect("prompt"));
        s.acp_step(serde_json::from_value(usage(18000.0)).expect("usage"));
        true
    });
    rt.frame(0.1);
    assert!(!present(&rt, "composer-chat-chat-1-context"), "the first report is the baseline");
    rt.update_if(|s, _| {
        s.acp_step(serde_json::from_value(usage(66000.0)).expect("usage"));
        true
    });
    rt.frame(0.2);
    tap(&mut rt, "composer-chat-chat-1-context");
    assert!(present(&rt, "composer-chat-chat-1-context-card"), "the ring opens the context card");
}
