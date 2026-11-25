use client::ws::{Command, Output};
use rune::Runtime;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;
use ui::composer::agent_menu::SwitchAct;

use crate::State;
use crate::composer::Host;

const CHAT: &str = "chat-1";

fn host() -> Host {
    Host::Chat(CHAT.into())
}

/// A launched Claude chat in `/work` with Claude and Gemini configured.
fn claude_chat() -> Runtime<State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(Output::Reply { call: 7, result: Ok(None) });
    s.apply(Output::Reply { call: 8, result: Ok(None) });
    s.agents_loaded(Some(json!([
        { "id": "claude", "name": "Claude", "command": "claude-acp" },
        { "id": "gemini", "name": "Gemini", "command": "gemini-acp" },
    ])));
    let mut rt = Runtime::new(s, crate::view, (1280.0, 800.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.update_if(|s, _| {
        let launch = crate::launch::Launch {
            project_id: "p-a".into(),
            config_id: "claude".into(),
            cwd: "/work".into(),
            send: ui::composer::Send::Text { text: String::new() },
        };
        s.install_launched_chat(CHAT, "agent-1", &launch);
        s.workspace.step(model::workspace::Event::AddAgentChatTab(CHAT.into(), None));
        s.sync_chats();
        true
    });
    rt.frame(0.05);
    rt
}

/// The calls of `kind` sent so far, with their ids.
fn calls(s: &State, kind: WsRequestType) -> Vec<(u64, Value)> {
    s.sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { call, kind: k, payload, .. } if *k == kind => Some((*call, payload.clone())),
            _ => None,
        })
        .collect()
}

fn answer(rt: &mut Runtime<State>, kind: WsRequestType, result: Value) {
    let (call, _) = calls(&rt.state, kind).pop().unwrap_or_else(|| panic!("no {kind:?} sent"));
    rt.update_if(|s, _| {
        s.apply(Output::Reply { call, result: Ok(Some(result.clone())) });
        true
    });
    rt.frame(0.2);
}

fn act(rt: &mut Runtime<State>, act: SwitchAct) {
    rt.update_if(|s, tx| {
        s.composer_switch(&host(), act.clone(), tx);
        true
    });
    rt.frame(0.1);
}

fn event(rt: &mut Runtime<State>, value: Value) {
    rt.update_if(|s, _| {
        s.acp_step(serde_json::from_value(value.clone()).expect("event"));
        s.flush_chat();
        true
    });
    rt.frame(0.1);
}

/// Arms a switch to Gemini and answers the target's warm-up.
fn armed_to_gemini() -> Runtime<State> {
    let mut rt = claude_chat();
    act(&mut rt, SwitchAct::Agent("gemini".into()));
    answer(&mut rt, WsRequestType::SpawnAgent, json!({ "agentId": "agent-2", "capabilities": {}, "authMethods": [] }));
    rt
}

#[test]
fn the_chat_menu_lists_every_agent_and_a_pick_arms_the_switch() {
    let mut rt = claude_chat();
    let view = rt.state.switch_view(&host()).expect("a chat has a switch");
    let agents: Vec<(&str, bool)> = view.agents.iter().map(|a| (a.name.as_str(), a.picked)).collect();
    assert_eq!(agents, [("Claude", true), ("Gemini", false)], "the chat's own agent is picked");
    act(&mut rt, SwitchAct::Agent("gemini".into()));
    let spawns = calls(&rt.state, WsRequestType::SpawnAgent);
    assert_eq!(spawns.len(), 1, "arming warms the target");
    assert_eq!(spawns[0].1["config"]["configId"], "gemini");
    assert_eq!(rt.state.armed_config(CHAT).as_deref(), Some("gemini"));
    assert_eq!(rt.state.agent_label(&host()).as_deref(), Some("→ Gemini"));
    let view = rt.state.switch_view(&host()).expect("switch");
    assert_eq!(view.armed.as_deref(), Some("Gemini"));
    assert!(crate::tests::present(&rt, "composer-chat-chat-1-agent-switch-cancel"), "an armed chip has its cancel button");
}

#[test]
fn cancel_keeps_the_chat_on_its_agent() {
    let mut rt = armed_to_gemini();
    crate::tests::tap(&mut rt, "composer-chat-chat-1-agent-switch-cancel");
    assert_eq!(rt.state.armed_config(CHAT), None);
    assert_eq!(rt.state.agent_label(&host()).as_deref(), Some("Claude"));
    assert!(!crate::tests::present(&rt, "composer-chat-chat-1-agent-switch-cancel"));
}

#[test]
fn a_busy_chat_holds_the_pick_until_the_turn_is_cancelled() {
    let mut rt = claude_chat();
    event(&mut rt, json!({ "_onUserPrompt": [{ "agentId": "agent-1", "sessionId": CHAT, "turnId": "t1", "blocks": [{ "type": "text", "text": "hi" }] }, 3] }));
    act(&mut rt, SwitchAct::Agent("gemini".into()));
    assert!(calls(&rt.state, WsRequestType::SpawnAgent).is_empty(), "a busy chat does not arm");
    let view = rt.state.switch_view(&host()).expect("switch");
    assert_eq!(view.busy, Some(("Claude".to_owned(), true)), "the menu offers Wait and Cancel turn and switch");
    act(&mut rt, SwitchAct::CancelTurnAndSwitch);
    assert_eq!(calls(&rt.state, WsRequestType::CancelPrompt).len(), 1, "the turn is stopped first");
    assert!(calls(&rt.state, WsRequestType::SpawnAgent).is_empty(), "the arm waits for the turn to end");
    event(&mut rt, json!({ "_onPromptComplete": [{ "agentId": "agent-1", "sessionId": CHAT, "turnId": "t1", "stopReason": "cancelled" }, 4] }));
    assert_eq!(calls(&rt.state, WsRequestType::SpawnAgent).len(), 1, "the switch arms once the turn ended");
    assert_eq!(rt.state.armed_config(CHAT).as_deref(), Some("gemini"));
}

#[test]
fn wait_drops_the_held_pick() {
    let mut rt = claude_chat();
    event(&mut rt, json!({ "_onUserPrompt": [{ "agentId": "agent-1", "sessionId": CHAT, "turnId": "t1", "blocks": [] }, 3] }));
    act(&mut rt, SwitchAct::Agent("gemini".into()));
    act(&mut rt, SwitchAct::Wait);
    assert_eq!(rt.state.switch_view(&host()).expect("switch").busy, None);
    assert!(calls(&rt.state, WsRequestType::CancelPrompt).is_empty());
}

#[test]
fn the_next_send_runs_the_switch_and_the_tab_follows_the_new_session() {
    let mut rt = armed_to_gemini();
    if let Some((_, warm)) = calls(&rt.state, WsRequestType::CreateSession).pop() {
        assert_eq!(warm["ephemeral"], true, "the target's warm session");
        answer(&mut rt, WsRequestType::CreateSession, json!({ "sessionId": "warm-1" }));
    }
    rt.update_if(|s, _| {
        s.draft_mut(&host()).edit.insert("carry on");
        true
    });
    rt.update_if(|s, _| {
        s.composer_send(&host());
        true
    });
    rt.frame(0.1);
    let create = calls(&rt.state, WsRequestType::CreateSession);
    let promote = calls(&rt.state, WsRequestType::PromoteSession);
    let target = if let Some((_, payload)) = promote.last() {
        answer(&mut rt, WsRequestType::PromoteSession, Value::Null);
        payload["sessionId"].as_str().expect("promoted").to_owned()
    } else {
        let (_, payload) = create.last().expect("the switch creates a session").clone();
        assert_eq!(payload["agentId"], "agent-2");
        answer(&mut rt, WsRequestType::CreateSession, json!({ "sessionId": "chat-2" }));
        "chat-2".to_owned()
    };
    let markers = calls(&rt.state, WsRequestType::RecordAgentSwitch);
    assert_eq!(markers.len(), 1, "the host records the marker");
    assert_eq!(markers[0].1["sessionId"], CHAT);
    assert_eq!(markers[0].1["toConfigId"], "gemini");
    assert_eq!(markers[0].1["newSessionId"], target.as_str());
    answer(&mut rt, WsRequestType::RecordAgentSwitch, Value::Null);
    assert!(rt.state.chat.loaded.contains(&target), "the new session is followed live");
    let tabs: Vec<String> = model::workspace::leaves(rt.state.workspace.root())
        .into_iter()
        .flat_map(|leaf| leaf.tabs.iter().map(|tab| tab.id().to_owned()))
        .collect();
    assert!(tabs.iter().any(|t| t.contains(&target)) && !tabs.iter().any(|t| t.contains(CHAT)), "the tab shows {target}: {tabs:?}");
    let prompts = calls(&rt.state, WsRequestType::SendPrompt);
    assert_eq!(prompts.last().map(|(_, p)| p["sessionId"].clone()), Some(json!(target)), "the handoff goes to the new session");
    let switches = &rt.state.chat.acp.agent_switches[&target];
    assert_eq!(switches.len(), 1, "the new session's transcript holds the separator");
    assert_eq!(switches[0]["toConfigId"], "gemini");
    let id = switches[0]["id"].as_str().expect("marker id").to_owned();
    rt.frame(0.3);
    assert!(crate::tests::present(&rt, &format!("{id}-toggle")), "the transcript draws the separator {id}");
}
