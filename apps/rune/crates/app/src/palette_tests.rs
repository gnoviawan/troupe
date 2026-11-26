use client::ws::Command;
use rune::{Key, KeyEvent, Mods, Runtime};
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use super::{KEY, rows};
use crate::State;
use crate::tests::{present, read_reply, runtime_at, tap};

fn desktop() -> Runtime<State> {
    runtime_at((1280.0, 800.0))
}

/// The values written under `key`, oldest first, with the shared revision they went out over.
fn writes(rt: &Runtime<State>, key: &str) -> Vec<(Value, Value)> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: WsRequestType::StoreWrite, payload, .. } if payload["key"].as_str().is_some_and(|k| k.ends_with(key)) => {
                Some((payload["value"]["data"].clone(), payload["baseRevision"].clone()))
            }
            _ => None,
        })
        .collect()
}

fn press(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(0.016);
    rt.frame(0.032);
}

fn typed(rt: &mut Runtime<State>, text: &str) {
    for ch in text.chars() {
        rt.key(KeyEvent { key: Key::Text(ch.to_string()), mods: Mods::default() });
    }
    rt.frame(0.016);
    rt.frame(0.032);
}

#[test]
fn search_opens_the_palette_and_return_runs_the_best_match() {
    let mut rt = desktop();
    let (_, recent) = open_palette(&mut rt);
    answer(&mut rt, recent, read_value(json!([]), 0));
    assert_eq!(rt.state.overlays.top(), Some(KEY));
    assert!(present(&rt, "cmd-new-terminal"));
    typed(&mut rt, "change color theme");
    assert!(!present(&rt, "cmd-new-terminal"), "a row that does not match leaves");
    assert!(present(&rt, "cmd-change-color-theme"));
    press(&mut rt, Key::Enter);
    assert!(!rt.state.overlays.is_open(KEY));
    assert!(rt.state.overlays.is_open(crate::theme_picker::KEY));
    assert_eq!(writes(&rt, "settings/recent-commands"), vec![(json!(["change-color-theme"]), Value::Null)]);
}

#[test]
fn down_then_return_runs_the_second_row() {
    let mut rt = desktop();
    tap(&mut rt, "rail-search");
    typed(&mut rt, "App Pref");
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Enter);
    assert!(rt.state.overlays.is_open(crate::settings_view::APP), "Open Canvas ranks first, App Preferences second");
}

#[test]
fn the_pinned_list_shows_on_top_and_a_pin_writes_it_over_the_read_revision() {
    let mut rt = desktop();
    tap(&mut rt, "rail-search");
    let recent = rt.state.sync.last_call();
    rt.update_if(|s, _| {
        s.apply(read_reply(recent - 1, json!(["open-app-preferences"])));
        s.apply(read_reply(recent, json!(["new-terminal"])));
        true
    });
    rt.frame(0.016);
    assert!(present(&rt, "cmd-pinned:open-app-preferences"));
    assert!(present(&rt, "cmd-recent:new-terminal"));
    rt.update_if(|s, _| {
        s.toggle_pinned("report-bug");
        true
    });
    assert_eq!(
        writes(&rt, "settings/pinned-commands"),
        vec![(json!(["open-app-preferences", "report-bug"]), json!(3))]
    );
}

#[test]
fn the_phone_palette_offers_new_project_and_hides_the_canvas_and_shortcut_menu() {
    let rt = desktop();
    let ids = |phone: bool| rows(&rt.state, phone).into_iter().map(|r| r.id).collect::<Vec<_>>();
    let desktop = ids(false);
    let phone = ids(true);
    assert!(desktop.contains(&"open-canvas".to_owned()) && desktop.contains(&"open-shortcut-menu".to_owned()));
    assert!(!desktop.contains(&"new-project".to_owned()));
    assert!(phone.contains(&"new-project".to_owned()));
    assert!(!phone.contains(&"open-canvas".to_owned()) && !phone.contains(&"open-shortcut-menu".to_owned()));
}

/// Opens the palette; returns the call ids of the pinned read and the recent read.
fn open_palette(rt: &mut Runtime<State>) -> (u64, u64) {
    tap(rt, "rail-search");
    let recent = rt.state.sync.last_call();
    (recent - 1, recent)
}

fn answer(rt: &mut Runtime<State>, call: u64, reply: Value) {
    rt.update_if(move |s, _| {
        s.apply(client::ws::Output::Reply { call, result: Ok(Some(reply.clone())) });
        true
    });
    rt.frame(0.016);
}

/// The call id of the newest write to `key`.
fn last_write(rt: &Runtime<State>, key: &str) -> u64 {
    rt.state
        .sync
        .sent
        .iter()
        .rev()
        .find_map(|c| match c {
            Command::Call { call, kind: WsRequestType::StoreWrite, payload, .. } if payload["key"].as_str().is_some_and(|k| k.ends_with(key)) => Some(*call),
            _ => None,
        })
        .expect("a write")
}

#[test]
fn a_pin_the_host_merged_keeps_the_other_devices_pins() {
    let mut rt = desktop();
    let (pins, recent) = open_palette(&mut rt);
    answer(&mut rt, pins, read_value(json!([]), 5));
    answer(&mut rt, recent, read_value(json!([]), 0));
    rt.update_if(|s, _| {
        s.toggle_pinned("y");
        true
    });
    let call = last_write(&rt, "settings/pinned-commands");
    answer(&mut rt, call, json!({ "revision": 7, "merged": true, "value": { "_version": 1, "data": ["x", "y"] } }));
    assert_eq!(rt.state.palette.lists.pinned_command_ids, vec!["x".to_owned(), "y".to_owned()]);
    rt.update_if(|s, _| {
        s.toggle_pinned("z");
        true
    });
    assert_eq!(
        writes(&rt, "settings/pinned-commands").last().cloned(),
        Some((json!(["x", "y", "z"]), json!(7))),
        "the next pin keeps x, over the merged revision"
    );
}

#[test]
fn a_pin_another_device_saves_shows_without_a_reload() {
    let mut rt = desktop();
    let (pins, recent) = open_palette(&mut rt);
    answer(&mut rt, pins, read_value(json!(["a"]), 2));
    answer(&mut rt, recent, read_value(json!([]), 0));
    assert!(rt.state.sync.sent.iter().any(|c| matches!(
        c,
        Command::Watch { watch: troupe_protocol::ws::WsWatchName::Store, scope } if scope == "shared/lww/settings/pinned-commands"
    )));
    rt.update_if(|s, _| {
        s.apply(client::ws::Output::WatchDelta {
            watch: troupe_protocol::ws::WsWatchName::Store,
            scope: "shared/lww/settings/pinned-commands".into(),
            delta: json!({ "exists": true, "value": { "_version": 1, "data": ["a", "b"] } }),
        });
        true
    });
    assert_eq!(rt.state.palette.lists.pinned_command_ids, vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
fn runs_and_pins_before_the_reads_land_on_top_of_what_was_saved() {
    let mut rt = desktop();
    let (pins, recent) = open_palette(&mut rt);
    rt.update_if(|s, _| {
        s.toggle_pinned("report-bug");
        s.run_command("change-color-theme");
        true
    });
    assert!(writes(&rt, "settings/recent-commands").is_empty(), "no write before the read");
    assert!(writes(&rt, "settings/pinned-commands").is_empty(), "no write before the read");
    answer(&mut rt, recent, read_value(json!(["new-terminal", "open-canvas"]), 4));
    answer(&mut rt, pins, read_value(json!(["open-app-preferences"]), 6));
    assert_eq!(
        writes(&rt, "settings/recent-commands"),
        vec![(json!(["change-color-theme", "new-terminal", "open-canvas"]), Value::Null)]
    );
    assert_eq!(
        writes(&rt, "settings/pinned-commands"),
        vec![(json!(["open-app-preferences", "report-bug"]), json!(6))]
    );
}

/// A store read's reply: `value` stored at `revision` (0: the key is empty).
fn read_value(value: Value, revision: u64) -> Value {
    json!({ "value": { "_version": 1, "data": value }, "revision": revision, "clientInstanceId": null })
}
