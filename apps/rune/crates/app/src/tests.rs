use client::link::Status;
use client::ws::Output;
use rune::{Env, Runtime};
use serde_json::{Value, json};
use troupe_protocol::ws::WsWatchName;

use crate::{State, view};

fn projects_frame() -> Output {
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

fn read_reply(call: u64, value: Value) -> Output {
    let revision = if value.is_null() { 0 } else { 3 };
    Output::Reply {
        call,
        result: Ok(Some(
            json!({ "value": value, "revision": revision, "clientInstanceId": null }),
        )),
    }
}

/// A state that asked for its projects, layout (call 1) and active project (call 2).
fn connected() -> State {
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
    assert_eq!(names(&s), ["alpha", "beta", "gamma"]);
    assert_eq!(s.active_project().map(|p| p.name.as_str()), Some("beta"));
}

#[test]
fn a_device_without_a_selection_lands_on_the_host_default() {
    let mut s = connected();
    s.apply(read_reply(2, Value::Null));
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

fn runtime() -> Runtime<State> {
    let mut s = connected();
    s.apply(read_reply(1, Value::Null));
    s.apply(read_reply(2, Value::Null));
    s.apply(projects_frame());
    let mut rt = Runtime::new(s, view, (1280.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    rt.frame(0.032);
}

fn present(rt: &Runtime<State>, key: &str) -> bool {
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
