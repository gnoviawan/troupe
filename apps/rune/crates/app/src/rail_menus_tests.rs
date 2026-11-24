use client::ws::Command;
use rune::{Key, KeyEvent, Mods, Runtime};
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::rail::*;
use crate::tests::{present, runtime_at, tap};

fn desktop() -> Runtime<State> {
    runtime_at((1280.0, 800.0))
}

fn calls(rt: &Runtime<State>, kind: WsRequestType) -> Vec<Value> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call {
                kind: k, payload, ..
            } if *k == kind => Some(payload.clone()),
            _ => None,
        })
        .collect()
}

fn secondary(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.016);
    rt.frame(0.032);
}

/// A system back press, as `poll` handles it.
fn back(rt: &mut Runtime<State>) -> bool {
    let closed = rt.update_if(|s, _| s.close_top());
    rt.frame(0.016);
    closed
}

fn typed(rt: &mut Runtime<State>, text: &str) {
    for ch in text.chars() {
        rt.key(KeyEvent {
            key: Key::Text(ch.to_string()),
            mods: Mods::default(),
        });
    }
    rt.frame(0.016);
}

fn enter(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    rt.frame(0.032);
}

#[test]
fn the_project_menu_recolours_and_deletes_on_the_server() {
    let mut rt = desktop();
    secondary(&mut rt, "project-item-p-b");
    assert_eq!(rt.state.overlays.top(), Some(PROJECT_MENU));
    tap(&mut rt, "project-menu-item2");
    assert_eq!(rt.state.overlays.top(), Some(COLOUR));
    tap(&mut rt, "swatch-red");
    assert!(!rt.state.overlays.is_open(COLOUR));
    let updates = calls(&rt, WsRequestType::UpdateProject);
    assert_eq!(updates, [json!({ "projectId": "p-b", "color": "red" })]);

    secondary(&mut rt, "project-item-p-c");
    tap(&mut rt, "project-menu-item9");
    assert_eq!(rt.state.overlays.top(), Some(DELETE_PROJECT));
    tap(&mut rt, "confirm");
    assert_eq!(rt.state.overlays.top(), None);
    assert_eq!(
        calls(&rt, WsRequestType::RemoveProject),
        [json!({ "projectId": "p-c" })]
    );
}

#[test]
fn rename_turns_the_row_into_a_field_and_return_saves_it() {
    let mut rt = desktop();
    secondary(&mut rt, "project-item-p-b");
    tap(&mut rt, "project-menu-item1");
    assert!(present(&rt, RENAME_FIELD_KEY));
    typed(&mut rt, "omega");
    enter(&mut rt);
    assert!(rt.state.rail.rename.is_none());
    assert_eq!(
        calls(&rt, WsRequestType::UpdateProject),
        [json!({ "projectId": "p-b", "name": "omega" })]
    );
}

const RENAME_FIELD_KEY: &str = crate::rail_forms::RENAME_FIELD;

#[test]
fn the_add_menu_creates_a_project_and_a_group() {
    let mut rt = desktop();
    tap(&mut rt, "rail-add-project");
    assert_eq!(rt.state.overlays.top(), Some(ADD_MENU));
    tap(&mut rt, "add-project-menu-item0");
    assert_eq!(rt.state.overlays.top(), Some(NEW_PROJECT));
    typed(&mut rt, "/w/omega");
    enter(&mut rt);
    assert_eq!(rt.state.overlays.top(), None);
    let added = calls(&rt, WsRequestType::AddProject);
    assert_eq!(added.len(), 1);
    assert_eq!(
        (added[0]["name"].clone(), added[0]["path"].clone()),
        (json!("omega"), json!("/w/omega"))
    );

    tap(&mut rt, "rail-add-project");
    tap(&mut rt, "add-project-menu-item1");
    assert_eq!(rt.state.overlays.top(), Some(NEW_GROUP));
    typed(&mut rt, "Work");
    tap(&mut rt, "create");
    let groups: Vec<&str> = rt
        .state
        .projects
        .store
        .groups
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    assert_eq!(groups, ["Work"]);
    let prefs = calls(&rt, WsRequestType::StoreWrite)
        .into_iter()
        .filter(|p| {
            p["key"]
                .as_str()
                .is_some_and(|k| k.ends_with("/project-prefs"))
        })
        .count();
    assert!(prefs >= 1, "the group layout was saved");
    let group = rt.state.projects.store.groups[0].id.clone();
    assert!(present(&rt, &format!("rail-group-{group}")));
}

#[test]
fn the_chat_menu_copies_the_reference_and_deletes_after_confirming() {
    let mut rt = desktop();
    secondary(&mut rt, "rail-chat-chat-1");
    assert_eq!(rt.state.overlays.top(), Some(CHAT_MENU));
    let reference = crate::rail_menus::chat_reference(&rt.state, "chat-1");
    assert_eq!(
        reference,
        "Troupe chat: How does the web client decide\nChat ID: chat-1\nProject: alpha (ID p-a)\nRead it with the Troupe read_chat tool."
    );
    tap(&mut rt, "chat-menu-item10");
    assert_eq!(rt.state.overlays.top(), Some(DELETE_CHAT));
    tap(&mut rt, "confirm");
    assert_eq!(
        calls(&rt, WsRequestType::DeleteSession),
        [json!({ "sessionId": "chat-1" })]
    );
    assert!(!present(&rt, "rail-chat-chat-1"));
}

#[test]
fn the_theme_picker_previews_on_arrows_and_saves_on_return() {
    let mut rt = desktop();
    tap(&mut rt, "rail-color-themes");
    assert_eq!(rt.state.overlays.top(), Some(crate::theme_picker::KEY));
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.preview, Some("termul-light"));
    assert!(!rt.state.tokens().dark, "the window paints the preview");
    enter(&mut rt);
    assert_eq!(
        (rt.state.theme, rt.state.dark, rt.state.preview),
        ("termul", false, None)
    );
    let saved: Vec<Value> = calls(&rt, WsRequestType::StoreWrite)
        .into_iter()
        .filter(|p| p["key"] == "shared/lww/settings/app")
        .collect();
    assert_eq!(saved.len(), 1);
    assert_eq!(
        saved[0]["value"]["data"],
        json!({ "colorTheme": "termul", "appearanceMode": "light" })
    );
}

#[test]
fn escape_puts_the_saved_theme_back() {
    let mut rt = desktop();
    tap(&mut rt, "rail-color-themes");
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.preview, Some("termul-light"));
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!((rt.state.preview, rt.state.dark), (None, true));
    assert!(!rt.state.overlays.is_open(crate::theme_picker::KEY));
}

#[test]
fn on_a_phone_back_closes_one_overlay_per_press() {
    let mut rt = runtime_at((390.0, 844.0));
    assert!(present(&rt, "phone-header"));
    tap(&mut rt, "phone-more-button");
    assert!(present(&rt, "phone-more-sheet"));
    assert!(back(&mut rt));
    tap(&mut rt, "phone-menu");
    assert!(present(&rt, crate::DRAWER));
    tap(&mut rt, "actions");
    assert_eq!(
        rt.state.overlays.keys(),
        [crate::DRAWER, crate::phone::RECENTS]
    );
    assert!(back(&mut rt));
    assert_eq!(rt.state.overlays.keys(), [crate::DRAWER]);
    assert!(back(&mut rt));
    assert!(rt.state.overlays.keys().is_empty());
    assert!(!back(&mut rt));
}

#[test]
fn the_phone_project_sheet_switches_projects() {
    let mut rt = runtime_at((390.0, 844.0));
    tap(&mut rt, "phone-project-line");
    assert!(present(&rt, "phone-projects-sheet"));
    tap(&mut rt, "sheet-project-p-b");
    assert_eq!(
        rt.state.active_project().map(|p| p.name.as_str()),
        Some("beta")
    );
    assert!(rt.state.overlays.keys().is_empty());
}
