//! Unit tests for `ui::browser`: Chromium non-interleaved JPEG decoding and
//! Rune-to-CDP input mapping.

use base64::Engine;
use rune::{Ime, Key, KeyEvent, Mods, WheelEvent};
use serde_json::json;

use super::{decode_base64_jpeg, decode_jpeg};
use super::input::{
    ime_event, key_event, pointer_event, should_bubble_shortcut, touch_event, wheel_event,
};

/// Real headless Chromium screencast frame: 4 horizontal 200px bands (white, red, green, blue).
const CHROMIUM_BANDS: &[u8] = include_bytes!("testdata/frame-colors.jpg");

fn sample_rgb(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]]
}

#[test]
fn decodes_chromium_non_interleaved_jpeg_bands_accurately() {
    let frame = decode_jpeg(CHROMIUM_BANDS).expect("decode chromium jpeg");
    assert_eq!(frame.width, 1280);
    assert_eq!(frame.height, 800);
    assert_eq!(frame.rgba.len(), 1280 * 800 * 4);

    let white = sample_rgb(&frame.rgba, frame.width, 640, 100);
    let red = sample_rgb(&frame.rgba, frame.width, 640, 300);
    let green = sample_rgb(&frame.rgba, frame.width, 640, 500);
    let blue = sample_rgb(&frame.rgba, frame.width, 640, 700);

    assert!(white[0] > 230 && white[1] > 230 && white[2] > 230 && white[3] == 255, "white={white:?}");
    assert!(red[0] > 210 && red[1] < 40 && red[2] < 40 && red[3] == 255, "red={red:?}");
    assert!(green[0] < 40 && green[1] > 180 && green[2] < 40 && green[3] == 255, "green={green:?}");
    assert!(blue[0] < 40 && blue[1] < 40 && blue[2] > 210 && blue[3] == 255, "blue={blue:?}");
}

#[test]
fn base64_wrapper_round_trips_and_rejects_garbage() {
    let b64 = base64::engine::general_purpose::STANDARD.encode(CHROMIUM_BANDS);
    let frame = decode_base64_jpeg(&b64).expect("decode base64");
    assert_eq!((frame.width, frame.height), (1280, 800));
    assert!(decode_base64_jpeg("not-valid-base64!!!").is_none());
    assert!(decode_jpeg(b"not a jpeg").is_none());
}

#[test]
fn maps_text_and_named_keys_to_pane_input() {
    let ev = KeyEvent {
        key: Key::Text("a".into()),
        mods: Mods::default(),
    };
    let down = key_event(true, &ev).expect("down");
    assert_eq!(
        down,
        json!({
            "kind": "key",
            "action": "down",
            "key": "a",
            "code": "KeyA",
            "keyCode": 65,
            "modifiers": 0,
            "text": "a",
        })
    );
    let up = key_event(false, &ev).expect("up");
    assert_eq!(up["action"], "up");
    assert!(up.get("text").is_none());

    let enter = KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    };
    let enter_down = key_event(true, &enter).expect("enter");
    assert_eq!(enter_down["key"], "Enter");
    assert_eq!(enter_down["keyCode"], 13);
    assert_eq!(enter_down["text"], "\r");
}

#[test]
fn strips_text_when_ctrl_modifier_is_held() {
    let ev = KeyEvent {
        key: Key::Command('a'),
        mods: Mods {
            ctrl: true,
            ..Default::default()
        },
    };
    let down = key_event(true, &ev).expect("ctrl+a");
    assert_eq!(down["modifiers"], 2);
    assert!(down.get("text").is_none());
}

#[test]
fn maps_wheel_pointer_touch_and_ime_utf16_offsets() {
    let wheel = wheel_event(
        &WheelEvent::new(10.0, -120.0).at(100.0, 200.0),
        0,
    );
    assert_eq!(wheel["kind"], "wheel");
    assert_eq!(wheel["deltaX"], -10.0);
    assert_eq!(wheel["deltaY"], 120.0);

    let ptr = pointer_event("down", 12.0, 34.0, "left", 1, 1, 0);
    assert_eq!(ptr["kind"], "pointer");
    assert_eq!(ptr["clickCount"], 1);

    let touch_start = touch_event("start", 50.0, 60.0, 1, 0);
    assert_eq!(touch_start["points"].as_array().map(Vec::len), Some(1));
    let touch_end = touch_event("end", 50.0, 60.0, 1, 0);
    assert_eq!(touch_end["points"].as_array().map(Vec::len), Some(0));

    // "日本" is 6 UTF-8 bytes and 2 UTF-16 code units.
    let preedit = ime_event(&Ime::Preedit("日本".into(), Some((0, 6)), Vec::new())).expect("preedit");
    assert_eq!(preedit["kind"], "ime");
    assert_eq!(preedit["selectionStart"], 0);
    assert_eq!(preedit["selectionEnd"], 2);

    let commit = ime_event(&Ime::Commit("東京".into())).expect("commit");
    assert_eq!(commit, json!({ "kind": "text", "text": "東京" }));
}

#[test]
fn bubbles_app_level_shortcuts_out_of_browser_pane() {
    assert!(should_bubble_shortcut("commandPalette"));
    assert!(should_bubble_shortcut("settings"));
    assert!(should_bubble_shortcut("newBrowserTab"));
    assert!(!should_bubble_shortcut("closeTab"));
}
