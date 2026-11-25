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
    tap(&mut rt, "rail-search");
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
