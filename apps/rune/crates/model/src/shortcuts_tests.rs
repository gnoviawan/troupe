use super::*;

fn key(key: &str, ctrl: bool, alt: bool) -> KeyEvent {
    KeyEvent {
        key: key.into(),
        code: String::new(),
        ctrl_key: ctrl,
        alt_key: alt,
        ..KeyEvent::default()
    }
}

#[test]
fn web_defaults_fire_by_their_alt_binding() {
    let state = State::new(Platform::default());
    assert_eq!(find_match(&state, &key("t", false, true)).map(|s| s.id.as_str()), Some("newTerminal"));
    assert_eq!(find_match(&state, &key("b", true, false)).map(|s| s.id.as_str()), Some("sidebarToggle"));
    assert!(find_match(&state, &key("t", true, false)).is_none(), "ctrl+t is the browser's on the web");
}

#[test]
fn a_rebound_shortcut_fires_on_its_new_keys_only() {
    let mut state = State::new(Platform::default());
    step(&mut state, Event::UpdateShortcut("sidebarToggle".into(), "alt+j".into()));
    assert_eq!(find_match(&state, &key("j", false, true)).map(|s| s.id.as_str()), Some("sidebarToggle"));
    assert!(find_match(&state, &key("b", true, false)).is_none());
}

#[test]
fn a_saved_record_merges_over_the_defaults() {
    let saved = serde_json::json!({
        "sidebarToggle": { "id": "sidebarToggle", "customKey": "alt+j" },
        "gone": { "id": "gone", "customKey": "alt+g" }
    });
    let merged = Shortcuts::merged(&saved, Platform::default());
    assert_eq!(merged.0.len(), 25);
    assert_eq!(merged.get("sidebarToggle").unwrap().active_key(), "alt+j");
    assert_eq!(merged.get("zoomIn").unwrap().active_key(), "alt+=");
    assert!(merged.get("gone").is_none());
}
