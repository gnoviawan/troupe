use client::terminal::Command as TermCommand;
use client::ws::{Command, Output};
use rune::{Env, Runtime};
use serde_json::{Value, json};
use troupe_protocol::terminal::WebTerminalRequestType;
use troupe_protocol::ws::WsRequestType;

use crate::snapshots::{CREATE, DELETE, RESTORE};
use crate::tests::{read_reply, showing_alpha};
use crate::{Route, State, view};

fn saved(id: &str, name: &str, cwds: &[&str]) -> Value {
    let terminals: Vec<Value> = cwds
        .iter()
        .enumerate()
        .map(|(i, cwd)| json!({ "id": format!("{id}-t{i}"), "name": format!("{name} {i}"), "shell": "bash", "cwd": cwd, "scrollback": [] }))
        .collect();
    json!({ "id": id, "projectId": "p-a", "name": name, "createdAt": "2026-11-19T08:30:00.000Z", "terminals": terminals, "activeTerminalId": null })
}

fn list() -> Value {
    json!({ "snapshots": [saved("a", "Alpha", &["/w/a"]), saved("b", "Beta", &["/w/b1", "/w/b2"]), saved("c", "Gamma", &[])], "updatedAt": "2026-11-19T09:00:00.000Z" })
}

fn last_store_call(s: &State, kind: WsRequestType) -> (u64, Value) {
    s.sync
        .sent
        .iter()
        .rev()
        .find_map(|c| match c {
            Command::Call { call, kind: k, payload, .. } if *k == kind => Some((*call, payload.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {kind:?} sent"))
}

/// Alpha's snapshots page with the saved list read.
fn page() -> State {
    let mut s = showing_alpha();
    s.route = Route::Snapshots;
    s.load_snapshots();
    let (call, payload) = last_store_call(&s, WsRequestType::StoreRead);
    assert!(payload["key"].as_str().unwrap().ends_with("/snapshots/p-a"), "{payload}");
    s.apply(read_reply(call, list()));
    assert_eq!(s.snapshot_list().iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Alpha", "Beta", "Gamma"]);
    s
}

fn pick(s: &State, id: &str) -> model::snapshots::Snapshot {
    s.snapshots.model.find(id).cloned().expect("listed")
}

#[test]
fn the_delete_confirm_removes_the_picked_snapshot_only() {
    let mut s = page();
    s.ask_delete(pick(&s, "b"));
    assert!(s.overlays.is_open(DELETE));
    s.confirm_delete();
    let (read, _) = last_store_call(&s, WsRequestType::StoreRead);
    s.apply(read_reply(read, list()));
    let (write, payload) = last_store_call(&s, WsRequestType::StoreWrite);
    let ids: Vec<&str> = payload["value"]["data"]["snapshots"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["a", "c"]);
    assert!(payload["value"]["data"]["updatedAt"].as_str().is_some_and(|at| at.len() == 24));
    assert!(s.overlays.is_open(DELETE), "open until the write lands");
    assert!(s.close_top(), "Escape is taken");
    assert!(s.overlays.is_open(DELETE), "and refused while the delete runs");
    s.apply(Output::Reply { call: write, result: Ok(Some(json!({ "revision": 4 }))) });
    assert!(!s.overlays.is_open(DELETE));
    assert_eq!(s.snapshot_list().iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
}

#[test]
fn restore_reads_the_picked_snapshot_spawns_its_terminals_and_returns_to_the_workspace() {
    let mut s = page();
    s.ask_restore(pick(&s, "b"));
    s.terms.sent.clear();
    s.confirm_restore();
    assert!(s.snapshots.restoring);
    let (read, _) = last_store_call(&s, WsRequestType::StoreRead);
    s.apply(read_reply(read, list()));
    let spawns: Vec<Value> = s
        .terms
        .sent
        .iter()
        .filter_map(|c| match c {
            TermCommand::Call { kind: WebTerminalRequestType::Spawn, payload, .. } => Some(payload.clone()),
            _ => None,
        })
        .collect();
    let cwds: Vec<&str> = spawns.iter().map(|p| p["cwd"].as_str().unwrap()).collect();
    assert_eq!(cwds, ["/w/b1", "/w/b2"]);
    assert!(spawns.iter().all(|p| p["shell"] == json!("bash") && p["projectId"] == json!("p-a")));
    assert_eq!(s.route, Route::Workspace);
    assert!(!s.overlays.is_open(RESTORE));
    assert_eq!(s.snapshots.names.values().collect::<Vec<_>>(), ["Beta 0", "Beta 1"]);
}

#[test]
fn create_writes_the_new_snapshot_on_top_and_closes_once_saved() {
    let mut s = page();
    s.open_create_snapshot();
    s.create_snapshot();
    assert!(s.overlays.is_open(CREATE), "an empty name creates nothing");
    s.snapshots.name = rune::TextEdit::new("  Before deploy ");
    s.create_snapshot();
    let (read, _) = last_store_call(&s, WsRequestType::StoreRead);
    s.apply(read_reply(read, list()));
    let (write, payload) = last_store_call(&s, WsRequestType::StoreWrite);
    let first = &payload["value"]["data"]["snapshots"][0];
    assert_eq!((first["name"].as_str(), first["projectId"].as_str()), (Some("Before deploy"), Some("p-a")));
    assert!(first.get("description").is_none(), "an empty description is not saved: {first}");
    assert_eq!(payload["value"]["data"]["snapshots"].as_array().unwrap().len(), 4);
    s.apply(Output::Reply { call: write, result: Ok(Some(json!({ "revision": 4 }))) });
    assert!(!s.overlays.is_open(CREATE));
    assert_eq!(s.snapshot_list()[0].name, "Before deploy");
}

#[test]
fn the_page_lists_cards_and_a_card_delete_opens_the_confirm_naming_it() {
    for (size, phone) in [((1280.0, 800.0), false), ((390.0, 844.0), true)] {
        let mut rt = Runtime::new(page(), view, size, Env::new(true, 1.0));
        rt.frame(0.0);
        rt.frame(0.016);
        assert!(rt.find("snapshot-b").is_some(), "phone={phone}: the card draws");
        crate::tests::tap(&mut rt, "delete-b");
        assert!(rt.state.overlays.is_open(DELETE), "phone={phone}");
        assert_eq!(rt.state.snapshots.delete.as_ref().map(|x| x.id.as_str()), Some("b"));
        assert!(crate::tests::present(&rt, "delete-snapshot-dialog"), "phone={phone}");
    }
}

#[test]
fn the_route_reads_from_the_hash() {
    assert_eq!(Route::from_hash("#/snapshots"), Route::Snapshots);
}
