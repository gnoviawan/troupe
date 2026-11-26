use client::http::HttpError;
use client::ws::{Command, Output};
use model::settings::mcp::KEY;
use rune::{Runtime, TextEdit};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType, WsWatchName};

use super::PAGE;
use crate::State;
use crate::settings_mcp_dialog::{DIALOG, PASTE};
use crate::tests::{present, runtime_at, tap};

fn frame(rt: &mut Runtime<State>) {
    rt.frame(0.016);
    rt.frame(0.032);
}

fn edit(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update_if(move |s, _| {
        f(s);
        true
    });
    frame(rt);
}

/// The registry calls sent, oldest first: (call, kind, payload).
fn registry_calls(rt: &Runtime<State>) -> Vec<(u64, WsRequestType, Value)> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { call, kind, payload, .. } if payload["key"] == KEY => Some((*call, *kind, payload.clone())),
            _ => None,
        })
        .collect()
}

fn last_write(rt: &Runtime<State>) -> (u64, Value, Value) {
    let (call, _, payload) = registry_calls(rt).into_iter().rfind(|(_, k, _)| *k == WsRequestType::StoreWrite).expect("a registry write");
    (call, payload["value"]["data"].clone(), payload["baseRevision"].clone())
}

fn answer(rt: &mut Runtime<State>, call: u64, result: Result<Value, WsError>) {
    edit(rt, move |s| s.apply(Output::Reply { call, result: result.map(Some) }));
}

fn servers() -> Value {
    json!([
        { "id": "fs", "type": "stdio", "name": "files", "command": "npx", "args": ["-y", "@mcp/fs"], "enabled": true },
        { "id": "lin", "type": "http", "name": "Linear", "url": "https://mcp.linear.app/mcp", "enabled": true },
        { "id": "off", "type": "stdio", "name": "old", "command": "uvx", "enabled": false }
    ])
}

/// The MCP page open on `size`, its registry read answered with `list` at revision 4.
fn open_with(size: (f32, f32), list: Value) -> Runtime<State> {
    let mut rt = runtime_at(size);
    edit(&mut rt, |s| s.open_settings(Some(PAGE)));
    let (call, kind, _) = registry_calls(&rt).into_iter().next().expect("the page reads the registry");
    assert_eq!(kind, WsRequestType::StoreRead);
    answer(&mut rt, call, Ok(json!({ "value": { "_version": 1, "data": list }, "revision": 4 })));
    rt
}

fn opened() -> Runtime<State> {
    open_with((1280.0, 800.0), servers())
}

fn posted(rt: &Runtime<State>, path: &str) -> Vec<Value> {
    rt.state.mcp_ui.posted.iter().filter(|(p, _)| p == path).map(|(_, b)| b.clone()).collect()
}

fn toasts(rt: &Runtime<State>) -> Vec<String> {
    rt.state.toasts.visible().iter().map(|t| t.title.clone()).collect()
}

/// Answers the probe the model sent for `id` with `reply`.
fn probe_answer(rt: &mut Runtime<State>, id: &str, reply: Result<Value, HttpError>) {
    let effect = model::acp::Effect::ProbeMcpServer {
        config: Default::default(),
        server: rt.state.chat.acp.mcp_servers.iter().find(|s| s["id"] == json!(id)).cloned().unwrap(),
        deletions: rt.state.chat.acp.mcp_deletions(id),
    };
    edit(rt, move |s| s.mcp_fetched(effect, reply));
}

#[test]
fn the_page_reads_the_registry_shows_cards_and_probes_each_enabled_server_once() {
    let rt = opened();
    for id in ["fs", "lin", "off"] {
        assert!(present(&rt, &format!("mcp-server-card-{id}")), "a card for {id}");
    }
    let probed: Vec<String> = posted(&rt, "/mcp-servers/probe").iter().map(|b| b["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(probed, ["files", "Linear"], "the server that is off is not probed");
    assert_eq!(posted(&rt, "/mcp-servers/oauth/status"), vec![json!({ "projectId": "p-a", "serverUrl": "https://mcp.linear.app/mcp" })]);
    assert!(rt.state.sync.sent.iter().any(|c| matches!(c, Command::Watch { watch: WsWatchName::Store, scope } if scope == KEY)));
}

#[test]
fn a_probe_answer_sets_the_status_word_and_a_transport_failure_reads_unreachable() {
    let mut rt = opened();
    probe_answer(&mut rt, "fs", Ok(json!({ "status": "connected", "tools": [{ "name": "read" }, { "name": "write" }] })));
    probe_answer(&mut rt, "lin", Err(HttpError::Network("down".into())));
    assert_eq!(rt.state.server_status(&rt.state.chat.acp.mcp_servers[0]).label, "Connected");
    assert_eq!(rt.state.server_status(&rt.state.chat.acp.mcp_servers[1]).label, "Unreachable");
    assert!(present(&rt, "retry"), "an unreachable server offers Try again");
}

#[test]
fn remove_deletes_exactly_that_server_and_undo_puts_it_back() {
    let mut rt = opened();
    tap(&mut rt, "mcp-server-card-lin");
    assert!(present(&rt, "mcp-server-detail"));
    assert!(!present(&rt, "settings-page-header"), "the detail page has its own title");
    tap(&mut rt, "remove");
    let (call, list, base) = last_write(&rt);
    let ids: Vec<&str> = list.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["fs", "off"]);
    assert_eq!(base, json!(4));
    assert!(!toasts(&rt).iter().any(|t| t.starts_with("Deleted")), "Undo waits for the write");
    answer(&mut rt, call, Ok(json!({ "revision": 5 })));
    assert!(toasts(&rt).contains(&"Deleted Linear".to_owned()));
    let undo = rt.state.settings_ui.undo.as_ref().map(|(id, _)| *id).unwrap();
    edit(&mut rt, move |s| s.run_undo(undo));
    let (_, list, base) = last_write(&rt);
    let ids: Vec<&str> = list.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["fs", "off", "lin"]);
    assert_eq!(base, json!(5), "the restore goes over the delete's revision");
}

#[test]
fn a_failed_toggle_restores_the_list_and_says_so() {
    let mut rt = opened();
    tap(&mut rt, "mcp-server-card-fs/head/switch");
    assert_eq!(rt.state.chat.acp.mcp_servers[0]["enabled"], json!(false), "the switch moves at once");
    let (call, _, _) = last_write(&rt);
    answer(&mut rt, call, Err(WsError { code: "REVISION_CONFLICT".into(), message: "stale".into(), details: None }));
    assert_eq!(rt.state.chat.acp.mcp_servers[0]["enabled"], json!(true));
    let shown = toasts(&rt);
    assert!(shown.contains(&"Could not update the MCP server. Your previous setting was restored.".to_owned()), "{shown:?}");
    assert!(!rt.state.composer.rejected, "the composer is not told about it");
}

#[test]
fn add_shows_the_forms_errors_then_saves_the_parsed_command() {
    let mut rt = opened();
    tap(&mut rt, "add-server");
    assert_eq!(rt.state.overlays.top(), Some(DIALOG));
    tap(&mut rt, "kind-local");
    tap(&mut rt, "save");
    assert!(rt.state.mcp_ui.dialog.as_ref().unwrap().show_errors);
    assert_eq!(registry_calls(&rt).iter().filter(|(_, k, _)| *k == WsRequestType::StoreWrite).count(), 0, "an invalid form writes nothing");
    edit(&mut rt, |s| {
        let d = s.mcp_ui.dialog.as_mut().unwrap();
        d.name = TextEdit::new("docs");
        d.command = TextEdit::new("API_KEY=k npx -y 'my server'");
    });
    tap(&mut rt, "save");
    let (call, list, _) = last_write(&rt);
    let added = &list.as_array().unwrap()[3];
    assert_eq!(added["command"], json!("npx"));
    assert_eq!(added["args"], json!(["-y", "my server"]));
    assert_eq!(added["env"], json!([{ "name": "API_KEY", "value": "k" }]));
    answer(&mut rt, call, Ok(json!({ "revision": 5 })));
    assert!(toasts(&rt).contains(&"Added 1 MCP server".to_owned()));
    assert!(rt.state.mcp_ui.dialog.is_none());
    assert!(!rt.state.overlays.is_open(DIALOG));
}

#[test]
fn edit_keeps_the_server_id_and_its_on_switch() {
    let mut rt = opened();
    edit(&mut rt, |s| s.open_mcp_edit("off"));
    edit(&mut rt, |s| s.mcp_ui.dialog.as_mut().unwrap().name = TextEdit::new("older"));
    tap(&mut rt, "save");
    let (call, list, _) = last_write(&rt);
    assert_eq!(list[2], json!({ "id": "off", "type": "stdio", "name": "older", "command": "uvx", "enabled": false }));
    answer(&mut rt, call, Ok(json!({ "revision": 5 })));
    assert!(toasts(&rt).contains(&"MCP server updated".to_owned()));
}

#[test]
fn connect_opens_the_paste_dialog_and_a_fresh_token_finishes_it() {
    let mut rt = opened();
    probe_answer(&mut rt, "lin", Ok(json!({ "status": "authRequired", "tools": [] })));
    tap(&mut rt, "mcp-server-card-lin/foot/connect");
    let start = rt.state.chat.acp.mcp_oauth_connecting.get("lin").cloned();
    assert_eq!(start, Some(json!(true)));
    assert_eq!(posted(&rt, "/mcp-servers/oauth/start"), vec![json!({ "projectId": "p-a", "serverUrl": "https://mcp.linear.app/mcp" })]);
    let effect = model::acp::Effect::StartMcpOAuth {
        project_id: "p-a".into(),
        url: "https://mcp.linear.app/mcp".into(),
        server_id: "lin".into(),
        server_name: "Linear".into(),
        flow: 1,
        deletions: 0,
    };
    let reply = json!({ "authUrl": "https://auth.example/x", "redirectUri": "http://127.0.0.1:4321/callback", "tokenGeneration": 2 });
    edit(&mut rt, move |s| s.mcp_fetched(effect, Ok(reply)));
    assert!(rt.state.overlays.is_open(PASTE));
    assert!(present(&rt, "complete"));
    edit(&mut rt, |s| {
        s.now += 1.5;
        let now = s.now;
        s.mcp_tick(now);
    });
    let url = "https://mcp.linear.app/mcp";
    edit(&mut rt, move |s| s.mcp_polled(url, Ok(json!({ "hasToken": true, "tokenGeneration": 2 }))));
    assert!(rt.state.overlays.is_open(PASTE), "the token the start already saw does not count");
    edit(&mut rt, |s| {
        s.now += 1.5;
        let now = s.now;
        s.mcp_tick(now);
    });
    edit(&mut rt, move |s| s.mcp_polled(url, Ok(json!({ "hasToken": true, "tokenGeneration": 3 }))));
    assert!(!rt.state.overlays.is_open(PASTE));
    assert!(toasts(&rt).contains(&"Connected to Linear".to_owned()));
}

#[test]
fn a_bad_paste_says_what_to_paste_without_a_host_call() {
    let mut rt = opened();
    probe_answer(&mut rt, "lin", Ok(json!({ "status": "authRequired", "tools": [] })));
    edit(&mut rt, |s| s.connect_mcp("lin"));
    let effect = model::acp::Effect::StartMcpOAuth {
        project_id: "p-a".into(),
        url: "https://mcp.linear.app/mcp".into(),
        server_id: "lin".into(),
        server_name: "Linear".into(),
        flow: 1,
        deletions: 0,
    };
    let reply = json!({ "authUrl": "https://auth.example/x", "redirectUri": "http://127.0.0.1:4321/callback" });
    edit(&mut rt, move |s| s.mcp_fetched(effect, Ok(reply)));
    edit(&mut rt, |s| s.mcp_ui.paste.address = TextEdit::new("https://example.com/cb?code=1"));
    tap(&mut rt, "complete");
    assert!(rt.state.mcp_ui.paste.error.as_ref().is_some_and(|(m, _)| m.starts_with("Paste the address your browser landed on.")));
    assert!(posted(&rt, "/mcp-servers/oauth/complete").is_empty());
    tap(&mut rt, "dismiss");
    assert!(!rt.state.overlays.is_open(PASTE));
    assert_eq!(rt.state.chat.acp.mcp_oauth_connecting.get("lin"), Some(&json!(false)));
}

#[test]
fn the_phone_page_puts_one_card_on_each_line() {
    let rt = open_with((390.0, 844.0), servers());
    let (_, a) = rt.find("mcp-server-card-fs").unwrap();
    let (_, b) = rt.find("mcp-server-card-lin").unwrap();
    assert!(b > a + 40.0, "the second card sits under the first");
}

#[test]
fn an_empty_registry_offers_add_server() {
    let rt = open_with((1280.0, 800.0), Value::Null);
    assert!(present(&rt, "mcp-empty"));
    assert!(present(&rt, "add-server"));
}

#[test]
fn the_phone_drawer_opens_preferences() {
    let mut rt = runtime_at((390.0, 844.0));
    edit(&mut rt, |s| s.overlays.open(crate::DRAWER));
    tap(&mut rt, "drawer-settings");
    assert!(rt.state.overlays.is_open(crate::settings_view::APP));
    assert!(!rt.state.overlays.is_open(crate::DRAWER));
}
