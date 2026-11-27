use client::ws::{Command, Output};
use rune::{Runtime, TextEdit};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use super::PAGE;
use crate::State;
use crate::tests::{present, runtime_at, tap};

fn edit(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update_if(move |s, _| {
        f(s);
        true
    });
    rt.frame(0.016);
    rt.frame(0.032);
}

fn calls(rt: &Runtime<State>, want: WsRequestType) -> Vec<(u64, Value)> {
    rt.state.sync.sent.iter().filter_map(|c| match c {
        Command::Call { call, kind, payload, .. } if *kind == want => Some((*call, payload.clone())),
        _ => None,
    }).collect()
}

fn reply(rt: &mut Runtime<State>, call: u64, result: Result<Value, WsError>) {
    edit(rt, move |s| s.apply(Output::Reply { call, result: result.map(Some) }));
}

fn view() -> Value {
    json!({ "enabled": true, "maxRunning": 3, "maxDepth": 1, "profiles": [
        { "id": "claude", "agentId": "claude-acp", "enabled": true, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": null, "agentName": "Claude Agent", "implicit": true },
        { "id": "codex", "agentId": "codex-acp", "enabled": true, "model": null, "mode": null, "configOptions": {}, "approvals": "allowAll", "description": "Fast", "agentName": "Codex", "implicit": false },
        { "id": "codex-2", "agentId": "codex-acp", "enabled": false, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": null, "agentName": "Codex", "implicit": false }
    ] })
}

fn opened() -> Runtime<State> {
    let mut rt = runtime_at((1280.0, 800.0));
    edit(&mut rt, |s| s.open_settings(Some(PAGE)));
    let (call, _) = calls(&rt, WsRequestType::SubagentSettingsGet).pop().expect("the page reads the settings");
    reply(&mut rt, call, Ok(view()));
    rt
}

#[test]
fn the_page_shows_the_switch_the_caps_and_each_profile() {
    let rt = opened();
    for key in ["subagent-enabled", "max-running", "max-depth", "subagent-profile-claude", "subagent-profile-codex", "subagent-profile-codex-2", "add-profile", "footnote"] {
        assert!(present(&rt, key), "the page shows {key}");
    }
    assert!(present(&rt, "shield"), "Allow everything marks its profile");
}

#[test]
fn changes_go_one_at_a_time_and_show_at_once() {
    let mut rt = opened();
    tap(&mut rt, "max-running-inc");
    tap(&mut rt, "max-running-inc");
    let sets = calls(&rt, WsRequestType::SubagentSettingsSet);
    assert_eq!(sets.len(), 1, "the second change waits for the first answer");
    assert_eq!(sets[0].1, json!({ "maxRunning": 4 }));
    assert_eq!(rt.state.subagents_view().unwrap()["maxRunning"], json!(5), "both show at once");
    let mut answered = view();
    answered["maxRunning"] = json!(4);
    reply(&mut rt, sets[0].0, Ok(answered));
    let sets = calls(&rt, WsRequestType::SubagentSettingsSet);
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[1].1, json!({ "maxRunning": 5 }), "the next change is built on the answer");
}

#[test]
fn a_refused_change_toasts_and_reads_the_host_again() {
    let mut rt = opened();
    tap(&mut rt, "subagent-enabled-switch");
    let (call, input) = calls(&rt, WsRequestType::SubagentSettingsSet).pop().unwrap();
    assert_eq!(input, json!({ "enabled": false }));
    reply(&mut rt, call, Err(WsError { code: "FORBIDDEN".into(), message: "read-only".into(), details: None }));
    assert!(rt.state.toasts.visible().iter().any(|t| t.title == "Could not save the sub-agent setting."));
    assert_eq!(calls(&rt, WsRequestType::SubagentSettingsGet).len(), 2);
    assert_eq!(rt.state.subagents_view().unwrap()["enabled"], json!(true));
}

#[test]
fn a_profile_edits_its_switch_approvals_name_and_note() {
    let mut rt = opened();
    edit(&mut rt, |s| s.subagents_toggle("codex-2"));
    assert!(present(&rt, "details"));
    tap(&mut rt, "switch");
    let set = calls(&rt, WsRequestType::SubagentSettingsSet).pop().unwrap();
    assert_eq!(set.1["upsertProfiles"], json!([{ "profile": { "id": "claude", "agentId": "claude-acp", "enabled": false, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": null } }]), "the first row's switch stores its implicit default, turned off");
    reply(&mut rt, set.0, Ok(view()));
    let mut rt = opened();
    edit(&mut rt, |s| s.subagents_toggle("codex-2"));
    edit(&mut rt, |s| {
        s.subagents_ui.name = TextEdit::new("codex-fast");
        s.subagents_ui.note = TextEdit::new("  Quick fixes ");
    });
    edit(&mut rt, |s| s.subagents_toggle("codex-2"));
    let set = calls(&rt, WsRequestType::SubagentSettingsSet).pop().unwrap();
    assert_eq!(set.1["upsertProfiles"], json!([{ "profile": { "id": "codex-2", "agentId": "codex-acp", "enabled": false, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": "Quick fixes" } }]));
    reply(&mut rt, set.0, Ok(rules_applied(&set.1)));
    let rename = calls(&rt, WsRequestType::SubagentSettingsSet).pop().unwrap();
    assert_eq!(rename.1["upsertProfiles"][0]["previousId"], json!("codex-2"));
    assert_eq!(rename.1["upsertProfiles"][0]["profile"]["id"], json!("codex-fast"));
    assert_eq!(rename.1["upsertProfiles"][0]["profile"]["description"], json!("Quick fixes"), "the rename is built on the saved note");
}

fn rules_applied(input: &Value) -> Value {
    model::settings::subagents::apply_input(&view(), input)
}

#[test]
fn add_and_delete_profiles() {
    let mut rt = opened();
    tap(&mut rt, "add-profile");
    let set = calls(&rt, WsRequestType::SubagentSettingsSet).pop().unwrap();
    let upserts = set.1["upsertProfiles"].as_array().unwrap();
    assert_eq!(upserts.last().unwrap()["profile"]["id"], json!("claude-2"));
    assert_eq!(rt.state.subagents_ui.expanded.as_deref(), Some("claude-2"));
    reply(&mut rt, set.0, Ok(rules_applied(&set.1)));
    edit(&mut rt, |s| s.subagents_change(super::Change::Delete("codex-2".into())));
    let del = calls(&rt, WsRequestType::SubagentSettingsSet).pop().unwrap();
    assert_eq!(del.1, json!({ "deleteProfiles": ["codex-2"] }));
}
