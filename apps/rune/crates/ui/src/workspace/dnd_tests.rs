use model::Ids;
use model::persistence::{self, hydrate};
use rune::{Cx, El, Env, Runtime, text};
use serde_json::{Value, json};

use super::*;
use crate::overlay;

struct Counter(u32);

impl Ids for Counter {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

struct S {
    ws: Workspace,
}

fn lens(s: &mut S) -> &mut Workspace {
    &mut s.ws
}

fn view(s: &S, cx: &mut Cx<S>) -> El {
    let t = theme::tokens("termul", true);
    let mut layer = Vec::new();
    let empty = || text("What should we do in alpha?");
    let body = super::view(
        &s.ws,
        lens,
        t,
        cx,
        Slots {
            leading: Vec::new(),
            empty: &empty,
            tab: &|_| None,
            title: &|_| None,
            dark: true,
        },
        &mut layer,
    );
    overlay::root(body, layer)
}

fn open_file(path: &str, dirty: bool) -> Value {
    let mut file = json!({
        "filePath": path, "cursorPosition": { "line": 1, "col": 1 }, "scrollTop": 0.0,
        "viewMode": "code", "isDirty": dirty, "lastModified": 1000.0
    });
    if dirty {
        file["draftContent"] = json!("# draft\n");
    }
    file
}

/// One pane with `a.md` (unsaved), `b.md` and the Git tab, as a device saved it.
fn seeded() -> Runtime<S> {
    let record = json!({
        "_version": 1,
        "data": {
            "openFiles": [open_file("/p/a.md", true), open_file("/p/b.md", false)],
            "activeFilePath": "/p/b.md",
            "expandedDirs": [],
            "activeTabId": "edit-/p/b.md",
            "paneLayout": {
                "type": "leaf", "id": "left",
                "tabs": [
                    { "type": "editor", "filePath": "/p/a.md" },
                    { "type": "editor", "filePath": "/p/b.md" },
                    { "type": "git", "id": "git-/p", "cwd": "/p" }
                ],
                "activeTabId": "edit-/p/b.md"
            },
            "activePaneId": "left"
        }
    });
    let mut ws = Workspace::new(Box::new(Counter(0)));
    let hydrated = hydrate(Some(record), Some("/p"), &[], None, ws.ids(), &mut Vec::new());
    ws.activate("p", Some(hydrated));
    let mut rt = Runtime::new(S { ws }, view, (1000.0, 700.0), Env::new(true, 1.0));
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<S>) {
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

fn rect(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("no element keyed {key}"))
        .rect
}

fn present(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// Press `from`, move to `to` in steps with a frame between each, release there.
fn drag(rt: &mut Runtime<S>, from: (f32, f32), to: (f32, f32), before_release: impl FnOnce(&Runtime<S>)) {
    rt.pointer_down(from.0, from.1);
    rt.frame(0.016);
    for step in 1..=8 {
        let k = step as f32 / 8.0;
        rt.pointer_move(from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
        rt.frame(0.016);
    }
    before_release(rt);
    rt.pointer_up(to.0, to.1);
    settle(rt);
}

fn tap(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn record(rt: &Runtime<S>) -> Value {
    serde_json::to_value(rt.state.ws.record().expect("a visible project")).unwrap()
}

#[test]
fn a_tab_dragged_to_a_pane_edge_splits_there_and_the_centre_takes_a_tab() {
    let mut rt = seeded();
    let from = rt.find("tab-edit-/p/b.md").expect("b.md tab");
    let [x, y, w, h] = rect(&rt, "pane-content-left");
    let edge = (x + w * 0.9, y + h * 0.5);
    drag(&mut rt, from, edge, |rt| {
        assert!(present(rt, "drop-zone-left"), "the right zone lights up under the pointer");
        assert_eq!(
            rt.state.ws.drag.as_ref().and_then(|d| d.target.clone()),
            Some(DropTarget::Zone {
                pane: "left".into(),
                position: model::workspace::Position::Right
            })
        );
    });
    assert!(rt.state.ws.drag.is_none());
    assert!(!present(&rt, "drop-zones-left"), "the zones leave with the drag");

    let written = record(&rt);
    assert_eq!(
        written["paneLayout"],
        json!({
            "type": "split", "id": "id-3", "direction": "horizontal", "sizes": [50.0, 50.0],
            "children": [
                { "type": "leaf", "id": "left", "activeTabId": "git-/p", "tabs": [
                    { "type": "editor", "filePath": "/p/a.md" },
                    { "type": "git", "id": "git-/p", "cwd": "/p" }
                ] },
                { "type": "leaf", "id": "id-2", "activeTabId": "edit-/p/b.md", "tabs": [
                    { "type": "editor", "filePath": "/p/b.md" }
                ] }
            ]
        })
    );
    assert_eq!(written["activePaneId"], json!("id-2"));
    assert_eq!(written["activeTabId"], json!("edit-/p/b.md"));
    assert_eq!(written["activeFilePath"], json!("/p/b.md"));

    // Another pane's tab bar takes no tab, as `WorkspaceTabBar` lands only chat drags there.
    let before = record(&rt);
    let from = rt.find("tab-git-/p").expect("git tab");
    let [bx, by, bw, bh] = rect(&rt, "tab-bar-id-2");
    drag(&mut rt, from, (bx + bw * 0.6, by + bh * 0.5), |_| {});
    assert_eq!(record(&rt)["paneLayout"], before["paneLayout"]);

    // The new pane's centre zone takes it.
    let from = rt.find("tab-git-/p").expect("git tab");
    let [cx, cy, cw, ch] = rect(&rt, "pane-content-id-2");
    drag(&mut rt, from, (cx + cw * 0.5, cy + ch * 0.5), |_| {});
    let written = record(&rt);
    assert_eq!(
        written["paneLayout"]["children"],
        json!([
            { "type": "leaf", "id": "left", "activeTabId": "edit-/p/a.md", "tabs": [
                { "type": "editor", "filePath": "/p/a.md" }
            ] },
            { "type": "leaf", "id": "id-2", "activeTabId": "git-/p", "tabs": [
                { "type": "editor", "filePath": "/p/b.md" },
                { "type": "git", "id": "git-/p", "cwd": "/p" }
            ] }
        ])
    );
    assert_eq!(written["activeFilePath"], Value::Null, "the Git tab is active now");
}

#[test]
fn a_press_without_travel_selects_the_tab_and_its_pane() {
    let mut rt = seeded();
    tap(&mut rt, "tab-git-/p");
    let written = record(&rt);
    assert_eq!(written["activeTabId"], json!("git-/p"));
    assert_eq!(written["paneLayout"]["type"], json!("leaf"), "no split");
}

#[test]
fn closing_an_unsaved_tab_asks_and_discard_closes_it() {
    let mut rt = seeded();
    assert!(present(&rt, "tab-dirty-edit-/p/a.md"), "the unsaved file shows its dot");
    tap(&mut rt, "tab-close-edit-/p/a.md");
    assert_eq!(rt.state.ws.confirm_close.as_deref(), Some("/p/a.md"));
    assert!(present(&rt, "unsaved-changes-dialog"));
    assert_eq!(record(&rt)["openFiles"].as_array().map(Vec::len), Some(2), "nothing closed yet");

    tap(&mut rt, "secondary");
    assert!(!present(&rt, "unsaved-changes-dialog"));
    let written = record(&rt);
    assert_eq!(
        written["paneLayout"]["tabs"],
        json!([
            { "type": "editor", "filePath": "/p/b.md" },
            { "type": "git", "id": "git-/p", "cwd": "/p" }
        ])
    );
    assert_eq!(written["openFiles"], json!([open_file("/p/b.md", false)]));
}

#[test]
fn saving_from_the_dialog_asks_the_app_to_write_the_draft() {
    let mut rt = seeded();
    tap(&mut rt, "tab-close-edit-/p/a.md");
    tap(&mut rt, "confirm");
    assert_eq!(
        rt.state.ws.requests,
        vec![Request::SaveAndClose {
            path: "/p/a.md".into(),
            content: "# draft\n".into()
        }]
    );
    assert_eq!(record(&rt)["openFiles"].as_array().map(Vec::len), Some(2), "kept until saved");
    rt.state.ws.saved("/p/a.md");
    assert_eq!(record(&rt)["openFiles"], json!([open_file("/p/b.md", false)]));
}

#[test]
fn a_split_handle_resizes_its_neighbours_and_commits_on_release() {
    let mut rt = seeded();
    let from = rt.find("tab-edit-/p/b.md").expect("b.md tab");
    let [x, y, w, h] = rect(&rt, "pane-content-left");
    drag(&mut rt, from, (x + w * 0.5, y + h * 0.95), |_| {});
    assert_eq!(record(&rt)["paneLayout"]["direction"], json!("vertical"));

    let [_, _, _, group] = rect(&rt, "split-id-3");
    let (hx, hy) = rt.find("resize-id-3-0").expect("the handle");
    drag(&mut rt, (hx, hy), (hx, hy + group * 0.2), |rt| {
        assert!(rt.state.ws.sizing.is_some(), "sizes follow the drag before release");
    });
    let sizes = record(&rt)["paneLayout"]["sizes"].clone();
    let top = sizes[0].as_f64().unwrap();
    assert!((top - 70.0).abs() < 0.5, "top share {top}");
    assert!((top + sizes[1].as_f64().unwrap() - 100.0).abs() < 1e-9);
}

#[test]
fn shares_keep_ten_percent_on_each_side() {
    assert_eq!(resized(&[50.0, 50.0], 0, -900.0, 1000.0), vec![10.0, 90.0]);
    assert_eq!(resized(&[50.0, 50.0], 0, 100.0, 1000.0), vec![60.0, 40.0]);
    assert_eq!(resized(&[5.0, 5.0, 90.0], 0, 100.0, 1000.0), vec![5.0, 5.0, 90.0]);
    assert_eq!(resized(&[4.0, 12.0, 84.0], 0, -500.0, 1000.0), vec![8.0, 8.0, 84.0]);
}

#[test]
fn the_record_round_trips_unchanged_when_nothing_moves() {
    let rt = seeded();
    let written = record(&rt);
    let again = hydrate(
        Some(persistence::stored(written.clone())),
        Some("/p"),
        &[],
        None,
        &mut Counter(100),
        &mut Vec::new(),
    );
    assert_eq!(
        serde_json::to_value(persistence::record(&again.tree, &again.files)).unwrap(),
        written
    );
}
