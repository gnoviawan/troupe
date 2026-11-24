use client::ws::Command;
use model::shortcuts;
use rune::{Key, KeyEvent, Mods, Tx};
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::tests::{loaded, read_reply};

/// The Rune key event the desktop build gets for a binding such as `ctrl+shift+p`. Off macOS
/// Ctrl also sets `cmd`. On macOS a `ctrl` binding fires from Cmd (Control alone belongs to the
/// shell), so the press holds Cmd. A letter with Cmd held arrives as `Key::Command`.
fn press(binding: &str) -> KeyEvent {
    let mac = super::platform().mac;
    let mut mods = Mods::default();
    let mut name = "";
    for part in binding.split('+') {
        match part {
            "ctrl" => {
                mods.ctrl = !mac;
                mods.cmd = true;
            }
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            other => name = other,
        }
    }
    let key = match name {
        "pagedown" => Key::PageDown,
        "pageup" => Key::PageUp,
        "enter" => Key::Enter,
        "delete" => Key::Delete,
        "f2" => Key::Function(2),
        n if n.len() == 1 && mods.cmd && n.chars().all(|c| c.is_ascii_alphabetic()) => {
            Key::Command(n.chars().next().unwrap_or('a'))
        }
        n => Key::Text(n.to_owned()),
    };
    KeyEvent { key, mods }
}

fn key(s: &mut State, ev: &KeyEvent) -> bool {
    let mut tx = Tx::default();
    super::on_key(s, ev, &mut tx)
}

fn shortcut_writes(s: &State) -> Vec<Value> {
    s.sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call {
                kind: WsRequestType::StoreWrite,
                payload,
                ..
            } if payload["key"] == "shared/lww/settings/keyboard-shortcuts" => {
                Some(payload.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn every_default_shortcut_fires_its_action_once() {
    let mut s = loaded();
    let defaults = shortcuts::defaults(s.shortcuts.platform).0;
    assert_eq!(defaults.len(), 25);
    for (id, shortcut) in &defaults {
        if shortcut.default_key.is_empty() {
            continue;
        }
        s.overlays = ui::overlay::Overlays::default();
        s.keys.fired.clear();
        assert!(
            key(&mut s, &press(&shortcut.default_key)),
            "{id} ({}) was not claimed",
            shortcut.default_key
        );
        assert_eq!(
            s.keys.fired,
            [id.as_str()],
            "{} fired the wrong action",
            shortcut.default_key
        );
    }
}

#[test]
fn the_sidebar_shortcut_collapses_the_rail_and_the_theme_shortcut_opens_the_picker() {
    let mut s = loaded();
    assert!(s.layout.record.rail_expanded);
    key(&mut s, &press("ctrl+b"));
    assert!(!s.layout.record.rail_expanded);
    key(&mut s, &press("ctrl+alt+b"));
    assert!(s.layout.record.side_panel_visible);
    let theme = shortcuts::defaults(s.shortcuts.platform)
        .0
        .into_iter()
        .find(|(id, _)| id == "colorThemePicker")
        .map(|(_, sc)| sc.default_key);
    key(&mut s, &press(&theme.unwrap_or_default()));
    assert_eq!(s.overlays.top(), Some(crate::theme_picker::KEY));
    key(
        &mut s,
        &KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        },
    );
    assert_eq!(s.overlays.top(), None);
}

#[test]
fn a_rebinding_is_saved_and_a_fresh_client_reads_it_back() {
    let mut s = loaded();
    s.keys.recording = Some("sidebarToggle".to_owned());
    key(&mut s, &press("ctrl+shift+y"));
    assert_eq!(s.keys.recording, None);
    let saved = shortcut_writes(&s);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0]["baseRevision"], json!(0));
    let data = saved[0]["value"]["data"].clone();
    // macOS records the Cmd chord as `cmd`, which then fires from Cmd alone.
    let chord = if super::platform().mac {
        "cmd+shift+y"
    } else {
        "ctrl+shift+y"
    };
    assert_eq!(data["sidebarToggle"]["customKey"], json!(chord));

    let mut fresh = crate::tests::connected();
    fresh.apply(read_reply(5, data));
    assert_eq!(
        fresh
            .shortcuts
            .shortcuts
            .get("sidebarToggle")
            .map(|sc| sc.active_key().to_owned()),
        Some(chord.to_owned())
    );
    fresh.keys.fired.clear();
    key(&mut fresh, &press("ctrl+shift+y"));
    assert_eq!(fresh.keys.fired, ["sidebarToggle"]);
    key(&mut fresh, &press("ctrl+b"));
    assert_eq!(
        fresh.keys.fired,
        ["sidebarToggle"],
        "the old binding no longer fires"
    );
}

#[test]
fn a_chord_another_shortcut_holds_asks_before_it_rebinds() {
    let mut s = loaded();
    s.keys.recording = Some("sidebarToggle".to_owned());
    key(&mut s, &press("f2"));
    let conflict = s.keys.conflict.clone();
    assert_eq!(
        conflict,
        Some((
            "sidebarToggle".to_owned(),
            "f2".to_owned(),
            "Rename File".to_owned()
        ))
    );
    assert!(shortcut_writes(&s).is_empty());
    key(
        &mut s,
        &KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        },
    );
    assert_eq!(
        (s.keys.recording.as_deref(), s.keys.conflict.as_ref()),
        (None, None)
    );
}

#[test]
fn a_rail_collapsed_on_load_expands_from_its_shortcut() {
    let mut s = crate::tests::connected();
    s.apply(read_reply(
        1,
        json!({ "version": 3, "railExpanded": false }),
    ));
    for call in 2..=5 {
        s.apply(read_reply(call, Value::Null));
    }
    s.apply(crate::tests::projects_frame());
    assert!(!s.layout.record.rail_expanded);
    key(&mut s, &press("ctrl+b"));
    assert!(s.layout.record.rail_expanded);
}
