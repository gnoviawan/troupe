use client::ws::{Command, Output};
use model::acp::Event;
use rune::Runtime;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::tests::present;

/// Taps `key`, then lets the sheet's slide settle.
fn tap(rt: &mut Runtime<State>, key: &str) {
    crate::tests::tap(rt, key);
    rt.frame(2.0);
}

/// A phone chat on Claude with two modes and two models, Claude and Gemini configured.
fn phone_chat() -> Runtime<State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(Output::Reply { call: 7, result: Ok(None) });
    s.apply(Output::Reply { call: 8, result: Ok(None) });
    s.agents_loaded(Some(json!([
        { "id": "claude", "name": "Claude", "command": "claude-acp" },
        { "id": "gemini", "name": "Gemini", "command": "gemini-acp" },
    ])));
    let mut rt = Runtime::new(s, crate::view, (390.0, 844.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.update_if(|s, _| {
        let launch = crate::launch::Launch {
            project_id: "p-a".into(),
            config_id: "claude".into(),
            cwd: "/work".into(),
            send: ui::composer::Send::Text { text: String::new() },
        };
        s.install_launched_chat("chat-1", "agent-1", &launch);
        let created = json!({
            "agentId": "agent-1",
            "sessionId": "chat-1",
            "modes": { "currentModeId": "default", "availableModes": [
                { "id": "default", "name": "Default" },
                { "id": "plan", "name": "Plan" },
            ] },
            "configOptions": [{ "id": "model", "name": "Model", "category": "model", "type": "select",
                "currentValue": "matrix-large",
                "options": [{ "value": "matrix-large", "name": "Matrix Large" }, { "value": "matrix-fast", "name": "Matrix Fast" }] }],
        });
        s.acp_step(Event::SessionCreated(serde_json::from_value(created).expect("created")));
        s.workspace.step(model::workspace::Event::AddAgentChatTab("chat-1".into(), None));
        s.sync_chats();
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

#[test]
fn the_phone_chip_opens_the_model_sheet_with_provider_mode_and_models() {
    let mut rt = phone_chat();
    tap(&mut rt, "composer-chat-chat-1-agent");
    assert!(!present(&rt, "composer-chat-chat-1-agent-menu-menu"), "the phone shows no dropdown");
    for key in ["model-sheet-provider", "model-sheet-mode", "model-sheet-model-matrix-large", "model-sheet-model-matrix-fast"] {
        assert!(present(&rt, key), "the sheet shows {key}");
    }
    tap(&mut rt, "model-sheet-model-matrix-fast");
    rt.update_if(|s, _| {
        let call = s.sync.last_call();
        s.apply(Output::Reply { call, result: Ok(None) });
        true
    });
    rt.frame(0.2);
    assert_eq!(
        sent(&rt.state, WsRequestType::SetConfigOption),
        [json!({ "agentId": "agent-1", "sessionId": "chat-1", "configId": "model", "valueId": "matrix-fast" })],
        "a model row picks the model, as the desktop menu does"
    );
    assert!(present(&rt, "model-sheet-provider"), "a model pick keeps the sheet open");
}

#[test]
fn the_sheet_sets_the_mode_from_its_mode_view() {
    let mut rt = phone_chat();
    tap(&mut rt, "composer-chat-chat-1-agent");
    tap(&mut rt, "model-sheet-mode");
    tap(&mut rt, "model-sheet-mode-plan");
    assert_eq!(sent(&rt.state, WsRequestType::SetMode), [json!({ "agentId": "agent-1", "sessionId": "chat-1", "modeId": "plan" })]);
    assert!(present(&rt, "model-sheet-provider"), "the pick goes back to the sheet's root");
}

#[test]
fn the_providers_view_arms_a_switch_to_another_agent() {
    let mut rt = phone_chat();
    tap(&mut rt, "composer-chat-chat-1-agent");
    tap(&mut rt, "model-sheet-provider");
    assert!(present(&rt, "model-sheet-agent-gemini"), "the providers view lists every agent");
    tap(&mut rt, "model-sheet-agent-gemini");
    assert_eq!(rt.state.armed_config("chat-1").as_deref(), Some("gemini"));
    assert_eq!(sent(&rt.state, WsRequestType::SpawnAgent).len(), 1, "arming warms the target");
    assert!(present(&rt, "model-sheet-provider"), "back at the root, which names the armed agent");
}
