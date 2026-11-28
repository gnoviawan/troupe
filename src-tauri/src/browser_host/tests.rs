#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{json, Value};

use super::input::{InputRefusal, PaneInput};
use super::ProfileKey;

fn cdp(event: Value) -> Result<(&'static str, Value), InputRefusal> {
    let input: PaneInput = serde_json::from_value(event).expect("a valid pane event");
    input
        .to_cdp()
        .map(|(method, params)| (method.as_str(), params))
}

#[test]
fn pane_pointer_wheel_and_touch_events_become_cdp_input() {
    assert_eq!(
        cdp(json!({ "kind": "pointer", "action": "down", "x": 10.5, "y": 20.0 })).unwrap(),
        (
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed", "x": 10.5, "y": 20.0, "button": "left", "buttons": 0,
                "clickCount": 1, "modifiers": 0,
            })
        )
    );
    assert_eq!(
        cdp(json!({ "kind": "wheel", "x": 1.0, "y": 2.0, "deltaX": 0.0, "deltaY": 120.0 }))
            .unwrap(),
        (
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseWheel", "x": 1.0, "y": 2.0, "deltaX": 0.0, "deltaY": 120.0, "modifiers": 0 })
        )
    );
    assert_eq!(
        cdp(json!({ "kind": "touch", "action": "start", "points": [{ "x": 5.0, "y": 6.0, "id": 1 }] })).unwrap(),
        (
            "Input.dispatchTouchEvent",
            json!({ "type": "touchStart", "touchPoints": [{ "x": 5.0, "y": 6.0, "id": 1 }], "modifiers": 0 })
        )
    );
}

#[test]
fn pane_keys_text_and_ime_become_cdp_input() {
    assert_eq!(
        cdp(json!({ "kind": "key", "action": "down", "key": "a", "code": "KeyA", "text": "a", "keyCode": 65 }))
            .unwrap(),
        (
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyDown", "key": "a", "code": "KeyA", "windowsVirtualKeyCode": 65,
                "modifiers": 0, "text": "a", "unmodifiedText": "a",
            })
        )
    );
    assert_eq!(
        cdp(json!({ "kind": "key", "action": "down", "key": "Enter", "keyCode": 13 }))
            .unwrap()
            .1["type"],
        json!("rawKeyDown")
    );
    assert_eq!(
        cdp(json!({ "kind": "text", "text": "café" })).unwrap(),
        ("Input.insertText", json!({ "text": "café" }))
    );
    assert_eq!(
        cdp(json!({ "kind": "ime", "text": "caf", "selectionStart": 3, "selectionEnd": 3 }))
            .unwrap(),
        (
            "Input.imeSetComposition",
            json!({ "text": "caf", "selectionStart": 3, "selectionEnd": 3 })
        )
    );
}

#[test]
fn oversized_or_malformed_pane_input_is_refused() {
    assert_eq!(
        cdp(json!({ "kind": "text", "text": "x".repeat(5000) })),
        Err(InputRefusal::TextTooLong)
    );
    let points: Vec<Value> = (0..11)
        .map(|i| json!({ "x": 1.0, "y": 1.0, "id": i }))
        .collect();
    assert_eq!(
        cdp(json!({ "kind": "touch", "action": "move", "points": points })),
        Err(InputRefusal::TooManyTouchPoints)
    );
    assert!(serde_json::from_value::<PaneInput>(json!({ "kind": "script", "code": "1" })).is_err());
    assert!(
        serde_json::from_value::<PaneInput>(json!({ "kind": "text", "text": "a", "extra": true }))
            .is_err(),
        "unknown fields are refused"
    );
}

#[test]
fn every_project_gets_its_own_safe_profile_folder() {
    let a = ProfileKey::Project("proj-1".into()).dir_name();
    let b = ProfileKey::Project("proj/1".into()).dir_name();
    let traversal = ProfileKey::Project("../../etc".into()).dir_name();
    assert!(a.starts_with("project-proj-1-"));
    assert_ne!(a, b, "ids that differ only in unsafe characters stay apart");
    assert!(!traversal.contains('/') && !traversal.contains(".."));
    assert_eq!(ProfileKey::Agent.dir_name(), "agent");
}
