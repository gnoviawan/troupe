use client::link::Status;
use client::ws::Output;
use rune::{Env, Runtime};
use serde_json::{Value, json};
use troupe_protocol::ws::WsWatchName;

use crate::{State, view};

pub(crate) fn projects_frame() -> Output {
    let project = |id: &str, name: &str, color: &str| json!({ "id": id, "name": name, "color": color, "path": format!("/w/{name}"), "isArchived": false, "isDefault": id == "p-a" });
    Output::WatchState {
        watch: WsWatchName::Projects,
        scope: String::new(),
        state: json!({
            "projects": [project("p-a", "alpha", "blue"), project("p-b", "beta", "green"), project("p-c", "gamma", "purple")],
            "defaultProjectId": "p-a"
        }),
    }
}

pub(crate) fn read_reply(call: u64, value: Value) -> Output {
    let revision = if value.is_null() { 0 } else { 3 };
    Output::Reply {
        call,
        result: Ok(Some(
            json!({ "value": value, "revision": revision, "clientInstanceId": null }),
        )),
    }
}

/// A state that asked for its projects, its layout (call 1), active project (call 2) and
/// project prefs (call 3), `settings/app` (4), the shortcuts (5) and the chats (6).
pub(crate) fn connected() -> State {
    let mut s = State::default();
    s.begin();
    s.apply(Output::Status(Status::Live));
    s
}

fn names(s: &State) -> Vec<&str> {
    s.projects
        .store
        .projects
        .iter()
        .map(|p| p.name.as_str())
        .collect()
}

#[test]
fn the_registry_waits_for_the_saved_selection_then_selects_it() {
    let mut s = connected();
    s.apply(projects_frame());
    assert!(
        names(&s).is_empty(),
        "held until the device's selection is read"
    );
    s.apply(read_reply(2, json!("p-b")));
    assert!(names(&s).is_empty(), "held until the project prefs are read");
    s.apply(read_reply(3, Value::Null));
    assert_eq!(names(&s), ["alpha", "beta", "gamma"]);
    assert_eq!(s.active_project().map(|p| p.name.as_str()), Some("beta"));
}

#[test]
fn a_device_without_a_selection_lands_on_the_host_default() {
    let mut s = connected();
    s.apply(read_reply(2, Value::Null));
    s.apply(read_reply(3, Value::Null));
    s.apply(projects_frame());
    assert_eq!(s.active_project().map(|p| p.name.as_str()), Some("alpha"));
}

#[test]
fn the_saved_layout_hydrates_the_shell() {
    let mut s = connected();
    s.apply(read_reply(1, json!({ "version": 3, "railExpanded": false, "sidePanelVisible": true, "sidePanelWidth": 420 })));
    assert_eq!(s.layout.status, model::shell_layout::Status::Ready);
    assert!(!s.layout.record.rail_expanded);
    assert_eq!(s.layout.record.side_panel_width, 420.0);
}

/// A state with every read answered: the seeded projects, no saved settings, and one chat in
/// alpha.
pub(crate) fn loaded() -> State {
    let mut s = connected();
    for call in 1..=5 {
        s.apply(read_reply(call, Value::Null));
    }
    s.apply(projects_frame());
    let chat = json!({ "sessionId": "chat-1", "projectId": "p-a", "title": "How does the web client decide", "lastActivityAt": 0.0 });
    s.apply(Output::Reply { call: 6, result: Ok(Some(json!([chat]))) });
    s
}

pub(crate) fn runtime_at(size: (f32, f32)) -> Runtime<State> {
    let mut rt = Runtime::new(loaded(), view, size, Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

fn runtime() -> Runtime<State> {
    runtime_at((1280.0, 800.0))
}

pub(crate) fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    rt.frame(0.032);
}

pub(crate) fn present(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn the_rail_lists_the_server_projects_and_switches_between_them() {
    let mut rt = runtime();
    for id in ["p-a", "p-b", "p-c"] {
        assert!(present(&rt, &format!("project-item-{id}")), "row for {id}");
    }
    tap(&mut rt, "project-item-p-c");
    assert_eq!(
        rt.state.active_project().map(|p| p.name.as_str()),
        Some("gamma")
    );
}

#[test]
fn the_tool_strip_opens_switches_and_closes_the_side_panel() {
    let mut rt = runtime();
    assert!(!present(&rt, "side-panel"));
    tap(&mut rt, "tool-strip-changes");
    assert!(present(&rt, "side-panel-changes"));
    tap(&mut rt, "tool-strip-github");
    assert!(present(&rt, "side-panel-github"));
    assert_eq!(rt.state.layout.tab("p-a"), model::shell_layout::Tab::Github);
    tap(&mut rt, "tool-strip-github");
    assert!(!present(&rt, "side-panel"));
}

#[test]
fn the_rail_collapses_to_its_icon_column_and_expands_again() {
    let mut rt = runtime();
    assert!(present(&rt, "rail-resize"));
    tap(&mut rt, "rail-toggle");
    assert!(!rt.state.layout.record.rail_expanded);
    assert!(!present(&rt, "rail-resize"));
    tap(&mut rt, "rail-toggle");
    assert!(rt.state.layout.record.rail_expanded);
}

#[test]
fn a_hash_that_names_no_screen_routes_to_not_found() {
    assert_eq!(crate::Route::from_hash(""), crate::Route::Workspace);
    assert_eq!(crate::Route::from_hash("#/"), crate::Route::Workspace);
    assert_eq!(
        crate::Route::from_hash("#/nowhere"),
        crate::Route::NotFound("/nowhere".to_owned())
    );
}

fn failed(call: u64) -> Output {
    Output::Reply {
        call,
        result: Err(troupe_protocol::ws::WsError {
            code: "INTERNAL".into(),
            message: "disk full".into(),
            details: None,
        }),
    }
}

/// Alpha selected and shown, every read `begin` sent answered (calls 1 to 6): its workspace
/// record is read with call 7 and its terminals with call 8.
pub(crate) fn showing_alpha() -> State {
    let mut s = connected();
    s.apply(read_reply(2, json!("p-a")));
    for call in [3, 4, 5] {
        s.apply(read_reply(call, Value::Null));
    }
    s.apply(projects_frame());
    s
}

fn tab_kinds(s: &State) -> Vec<String> {
    let mut kinds = Vec::new();
    let mut stack = vec![s.workspace.tree().root.clone()];
    while let Some(node) = stack.pop() {
        match node {
            model::workspace::PaneNode::Leaf(leaf) => {
                kinds.extend(leaf.tabs.iter().map(|tab| tab.id().to_owned()));
            }
            model::workspace::PaneNode::Split(split) => stack.extend(split.children),
        }
    }
    kinds
}

#[test]
fn a_workspace_record_that_failed_to_read_is_never_written_over() {
    let mut s = showing_alpha();
    s.apply(failed(7));
    assert_eq!(s.workspace.project(), Some("p-a"), "a fresh workspace still shows");
    s.workspace.focus_pane("elsewhere");
    s.persist_workspace();
    assert_eq!(s.workspace_writes(), Vec::<&str>::new());
}

#[test]
fn a_workspace_record_that_does_not_decode_is_never_written_over() {
    let mut s = showing_alpha();
    s.apply(read_reply(7, json!({ "openFiles": "not a list" })));
    s.apply(read_reply(8, Value::Null));
    s.persist_workspace();
    assert_eq!(s.workspace_writes(), Vec::<&str>::new());
}

#[test]
fn saved_terminal_tabs_survive_while_no_terminal_is_live() {
    let mut s = showing_alpha();
    let record = json!({
        "openFiles": [], "activeFilePath": null, "expandedDirs": [], "activeTabId": "term-t1",
        "paneLayout": { "type": "leaf", "id": "l1", "activeTabId": "term-t1", "tabs": [
            { "type": "terminal", "terminalId": "t1" }, { "type": "git", "id": "git-/w/alpha", "cwd": "/w/alpha" }
        ] },
        "activePaneId": "l1"
    });
    s.apply(read_reply(7, record));
    s.apply(read_reply(8, json!({ "activeTerminalId": "t1", "terminals": [{ "id": "t1", "name": "Terminal 1", "shell": "bash", "cwd": "/w/alpha" }], "updatedAt": "2026-11-24T00:00:00Z" })));
    assert_eq!(tab_kinds(&s), ["term-t1", "git-/w/alpha"]);
    s.persist_workspace();
    assert_eq!(s.workspace_writes(), ["p-a"]);
}

#[test]
fn a_failed_workspace_write_is_sent_again_with_the_same_value() {
    let mut s = showing_alpha();
    s.apply(read_reply(7, Value::Null));
    s.apply(read_reply(8, Value::Null));
    s.persist_workspace();
    assert_eq!(s.workspace_writes(), ["p-a"]);
    s.apply(failed(9));
    s.workspace.revision += 1;
    s.persist_workspace();
    assert_eq!(s.workspace_writes(), ["p-a"], "the same record goes out again");
}
