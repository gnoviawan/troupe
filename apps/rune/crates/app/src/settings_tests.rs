use client::ws::Command;
use rune::Runtime;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::settings_view::{APP, RESET_ALL};
use crate::tests::{present, runtime_at, tap};

fn desktop() -> Runtime<State> {
    runtime_at((1280.0, 800.0))
}

/// The `settings/app` records written, oldest first.
fn written(rt: &Runtime<State>) -> Vec<Value> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call {
                kind: WsRequestType::StoreWrite,
                payload,
                ..
            } if payload["key"] == "shared/lww/settings/app" => Some(payload["value"]["data"].clone()),
            _ => None,
        })
        .collect()
}

fn open(rt: &mut Runtime<State>) {
    tap(rt, "rail-settings");
    assert_eq!(rt.state.overlays.top(), Some(APP));
    assert!(present(rt, "settings-page-preferences"));
}

#[test]
fn the_zoom_stepper_writes_the_whole_record_and_zooms_the_window() {
    let mut rt = desktop();
    open(&mut rt);
    tap(&mut rt, "ui-zoom-inc");
    let records = written(&rt);
    let last = records.last().expect("a write");
    assert_eq!(last["uiZoomLevel"], json!(1.1));
    assert_eq!(
        last["notifyOnTerminalIdle"],
        json!(true),
        "the whole record, as useUpdateAppSetting writes it"
    );
    assert!((rt.state.zoom.factor() - 1.1).abs() < 1e-6);
}

#[test]
fn a_page_switch_and_a_switch_row_write_their_setting() {
    let mut rt = desktop();
    open(&mut rt);
    tap(&mut rt, "notify-terminal-idle-switch");
    assert_eq!(written(&rt).last().expect("a write")["notifyOnTerminalIdle"], json!(false));
    write_reply(&mut rt, json!({ "revision": 4, "merged": false }));
    tap(&mut rt, "page-terminal");
    assert!(present(&rt, "settings-page-terminal"));
    tap(&mut rt, "terminal-font-size-inc");
    assert_eq!(written(&rt).last().expect("a write")["terminalFontSize"], json!(15.0));
}

#[test]
fn reset_waits_for_the_confirm_and_keeps_the_profiles() {
    let mut rt = desktop();
    rt.update_if(|s, _| {
        s.settings_step(model::settings::Event::UpdateSettings(
            serde_json::from_value(json!({ "colorTheme": "nord", "terminalProfiles": [{ "id": "p" }] })).expect("a map"),
        ));
        true
    });
    write_reply(&mut rt, json!({ "revision": 4, "merged": false }));
    open(&mut rt);
    tap(&mut rt, "page-about");
    let before = written(&rt).len();
    tap(&mut rt, "reset-settings");
    assert_eq!(rt.state.overlays.top(), Some(RESET_ALL));
    assert_eq!(written(&rt).len(), before, "nothing resets before the confirm");
    tap(&mut rt, "confirm");
    assert!(!rt.state.overlays.is_open(RESET_ALL));
    let last = written(&rt).last().cloned().expect("the reset's write");
    assert_eq!(last["colorTheme"], json!("termul"));
    assert_eq!(last["terminalProfiles"], json!([{ "id": "p" }]));
}

#[test]
fn an_idle_window_pick_posts_to_the_host_route() {
    let mut rt = desktop();
    rt.update_if(|s, _| {
        s.settings_step(model::settings::Event::SetAgentIdleWindow(json!(3600)));
        true
    });
    assert_eq!(rt.state.settings.get("agentIdleWindowSecs"), &json!(3600));
    assert_eq!(written(&rt).last().expect("a write")["agentIdleWindowSecs"], json!(3600));
}

/// The `baseRevision` of each `settings/app` write, oldest first.
fn bases(rt: &Runtime<State>) -> Vec<Value> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: WsRequestType::StoreWrite, payload, .. } if payload["key"] == "shared/lww/settings/app" => {
                Some(payload["baseRevision"].clone())
            }
            _ => None,
        })
        .collect()
}

/// Answers the newest `settings/app` write with `reply`.
fn write_reply(rt: &mut Runtime<State>, reply: Value) {
    let call = rt
        .state
        .sync
        .sent
        .iter()
        .rev()
        .find_map(|c| match c {
            Command::Call { call, kind: WsRequestType::StoreWrite, payload, .. } if payload["key"] == "shared/lww/settings/app" => Some(*call),
            _ => None,
        })
        .expect("a settings write");
    rt.update_if(move |s, _| {
        s.apply(client::ws::Output::Reply { call, result: Ok(Some(reply.clone())) });
        true
    });
    rt.frame(0.016);
}

#[test]
fn quick_edits_write_one_at_a_time_and_the_last_one_lands() {
    let mut rt = desktop();
    open(&mut rt);
    tap(&mut rt, "page-terminal");
    tap(&mut rt, "terminal-font-size-inc");
    tap(&mut rt, "terminal-font-size-inc");
    tap(&mut rt, "terminal-font-size-inc");
    assert_eq!(written(&rt).len(), 1, "the later edits wait for the first write's reply");
    write_reply(&mut rt, json!({ "revision": 4, "merged": false, "value": { "_version": 1, "data": {} } }));
    let records = written(&rt);
    assert_eq!(records.len(), 2, "the two waiting edits go as one write");
    assert_eq!(records[1]["terminalFontSize"], json!(17.0));
    assert_eq!(bases(&rt)[1], json!(4), "over the revision the first write made");
    assert_eq!(rt.state.settings.get("terminalFontSize"), &json!(17.0));
}

#[test]
fn a_merged_reply_shows_what_the_host_kept() {
    let mut rt = desktop();
    open(&mut rt);
    tap(&mut rt, "ui-zoom-inc");
    let mut kept = written(&rt).last().cloned().expect("a write");
    kept["colorTheme"] = json!("nord");
    write_reply(&mut rt, json!({ "revision": 9, "merged": true, "value": { "_version": 1, "data": kept } }));
    assert_eq!(rt.state.settings.get("colorTheme"), &json!("nord"), "another device's theme");
    assert_eq!(rt.state.settings.get("uiZoomLevel"), &json!(1.1), "our zoom");
    assert_eq!(written(&rt).len(), 1, "adopting the merge writes nothing back");
}
