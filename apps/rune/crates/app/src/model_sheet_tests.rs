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
    let mode = rt.elements().iter().find(|e| e.key.as_deref() == Some("model-sheet-mode")).and_then(|e| e.label.clone());
    assert_eq!(mode.as_deref(), Some("Mode, Ask first"), "the default mode reads as the chip does");
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

#[test]
fn a_mode_picked_while_a_switch_is_armed_goes_with_the_switch() {
    let mut rt = phone_chat();
    tap(&mut rt, "composer-chat-chat-1-agent");
    tap(&mut rt, "model-sheet-provider");
    tap(&mut rt, "model-sheet-agent-gemini");
    tap(&mut rt, "model-sheet-mode");
    tap(&mut rt, "model-sheet-mode-plan");
    assert!(sent(&rt.state, WsRequestType::SetMode).is_empty(), "the chat's own agent keeps its mode");
    let pending = &rt.state.chat.acp.sessions["chat-1"].rest["switching"]["pendingOptions"];
    assert_eq!(pending["modeId"], "plan", "the switch carries the mode");
}

#[test]
fn the_phone_chip_and_the_switch_cancel_take_a_44_point_tap() {
    let mut rt = phone_chat();
    tap(&mut rt, "composer-chat-chat-1-agent");
    tap(&mut rt, "model-sheet-provider");
    tap(&mut rt, "model-sheet-agent-gemini");
    rt.update_if(|s, _| {
        s.close_model_sheet();
        true
    });
    rt.frame(0.5);
    let size = |key: &str| {
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("no {key}")).rect;
        (r[2].round(), r[3].round())
    };
    assert_eq!(size("composer-chat-chat-1-agent").1, 44.0, "the chip's tap height");
    assert_eq!(size("composer-chat-chat-1-agent-switch-cancel"), (44.0, 44.0), "the cancel's tap area");
}
