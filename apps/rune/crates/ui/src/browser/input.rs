//! Maps Rune pointer, wheel, key, IME, and touch events to the server's
//! `browser_tab_input` `PaneInput` JSON payloads (`src-tauri/src/browser_host/input.rs`).

use rune::{Ime, Key, KeyEvent, KeyPlatform, Mods, PointerButton, WheelEvent};
use serde_json::{Value, json};

/// CDP modifier bitmask: Alt=1, Ctrl=2, Meta=4, Shift=8.
#[must_use]
pub fn cdp_modifiers(m: Mods) -> u32 {
    let mac = KeyPlatform::current() == KeyPlatform::Mac;
    let ctrl = m.ctrl || (!mac && m.cmd);
    let meta = m.logo || (mac && m.cmd);
    (u32::from(m.alt))
        | (u32::from(ctrl) << 1)
        | (u32::from(meta) << 2)
        | (u32::from(m.shift) << 3)
}

/// Maps a Rune pointer button to the `PaneInput::Pointer` `(button, buttons_mask)` pair.
#[must_use]
pub fn pointer_button(button: PointerButton) -> (&'static str, u32) {
    match button {
        PointerButton::Primary => ("left", 1),
        PointerButton::Secondary => ("right", 2),
        PointerButton::Middle => ("middle", 4),
        _ => ("none", 0),
    }
}

/// Builds a `PaneInput::Pointer` JSON object.
#[must_use]
pub fn pointer_event(
    action: &str,
    x: f32,
    y: f32,
    button: &str,
    buttons: u32,
    click_count: u32,
    modifiers: u32,
) -> Value {
    json!({
        "kind": "pointer",
        "action": action,
        "x": f64::from(x),
        "y": f64::from(y),
        "button": button,
        "buttons": buttons,
        "clickCount": click_count,
        "modifiers": modifiers,
    })
}

/// Builds a `PaneInput::Wheel` JSON object (negating Rune's scroll delta to match Chromium).
#[must_use]
pub fn wheel_event(e: &WheelEvent, modifiers: u32) -> Value {
    json!({
        "kind": "wheel",
        "x": f64::from(e.x),
        "y": f64::from(e.y),
        "deltaX": f64::from(-e.dx),
        "deltaY": f64::from(-e.dy),
        "modifiers": modifiers,
    })
}

/// Builds a `PaneInput::Touch` JSON object for mobile/phone touch interaction.
#[must_use]
pub fn touch_event(action: &str, x: f32, y: f32, id: u32, modifiers: u32) -> Value {
    let points = if action == "end" || action == "cancel" {
        json!([])
    } else {
        json!([{ "x": f64::from(x), "y": f64::from(y), "id": id }])
    };
    json!({
        "kind": "touch",
        "action": action,
        "points": points,
        "modifiers": modifiers,
    })
}

/// Maps a named Rune key to `(key, code, windowsVirtualKeyCode, text)`.
#[must_use]
pub fn named_key(key: &Key) -> Option<(&'static str, &'static str, u32, &'static str)> {
    Some(match key {
        Key::Enter => ("Enter", "Enter", 13, "\r"),
        Key::Backspace => ("Backspace", "Backspace", 8, ""),
        Key::Delete => ("Delete", "Delete", 46, ""),
        Key::Tab => ("Tab", "Tab", 9, ""),
        Key::Escape => ("Escape", "Escape", 27, ""),
        Key::Left => ("ArrowLeft", "ArrowLeft", 37, ""),
        Key::Up => ("ArrowUp", "ArrowUp", 38, ""),
        Key::Right => ("ArrowRight", "ArrowRight", 39, ""),
        Key::Down => ("ArrowDown", "ArrowDown", 40, ""),
        Key::Home => ("Home", "Home", 36, ""),
        Key::End => ("End", "End", 35, ""),
        Key::PageUp => ("PageUp", "PageUp", 33, ""),
        Key::PageDown => ("PageDown", "PageDown", 34, ""),
        Key::Function(n) => {
            let (name, vk) = match n {
                1 => ("F1", 112),
                2 => ("F2", 113),
                3 => ("F3", 114),
                4 => ("F4", 115),
                5 => ("F5", 116),
                6 => ("F6", 117),
                7 => ("F7", 118),
                8 => ("F8", 119),
                9 => ("F9", 120),
                10 => ("F10", 121),
                11 => ("F11", 122),
                12 => ("F12", 123),
                _ => return None,
            };
            (name, name, vk, "")
        }
        _ => return None,
    })
}

fn char_code_vk(c: char) -> (String, u32) {
    let up = c.to_ascii_uppercase();
    if up.is_ascii_alphabetic() {
        return (format!("Key{up}"), up as u32);
    }
    if up.is_ascii_digit() {
        return (format!("Digit{up}"), up as u32);
    }
    match c {
        ' ' => ("Space".into(), 32),
        '-' | '_' => ("Minus".into(), 189),
        '=' | '+' => ("Equal".into(), 187),
        '[' | '{' => ("BracketLeft".into(), 219),
        ']' | '}' => ("BracketRight".into(), 221),
        '\\' | '|' => ("Backslash".into(), 220),
        ';' | ':' => ("Semicolon".into(), 186),
        '\'' | '"' => ("Quote".into(), 222),
        ',' | '<' => ("Comma".into(), 188),
        '.' | '>' => ("Period".into(), 190),
        '/' | '?' => ("Slash".into(), 191),
        '`' | '~' => ("Backquote".into(), 192),
        _ => (String::new(), 0),
    }
}

/// Builds a `PaneInput::Key` JSON object for `down` or `up`, or `None` for unmapped keys.
#[must_use]
pub fn key_event(down: bool, ev: &KeyEvent) -> Option<Value> {
    let modifiers = cdp_modifiers(ev.mods);
    let (key, code, key_code, text): (String, String, u32, String) = match &ev.key {
        Key::Text(s) => {
            let (code, vk) = s.chars().next().map_or((String::new(), 0), char_code_vk);
            (s.clone(), code, vk, s.clone())
        }
        Key::Command(c) => {
            let (code, vk) = char_code_vk(*c);
            (c.to_string(), code, vk, String::new())
        }
        other => {
            let (k, c, vk, t) = named_key(other)?;
            (k.to_owned(), c.to_owned(), vk, t.to_owned())
        }
    };
    let has_ctrl_or_meta = (modifiers & 0b0110) != 0;
    let action = if down { "down" } else { "up" };
    let mut payload = json!({
        "kind": "key",
        "action": action,
        "key": key,
        "code": code,
        "keyCode": key_code,
        "modifiers": modifiers,
    });
    if down && !text.is_empty() && !has_ctrl_or_meta {
        payload["text"] = json!(text);
    }
    Some(payload)
}

/// Builds a `PaneInput::Ime` or `PaneInput::Text` JSON object from a Rune `Ime` event.
#[must_use]
pub fn ime_event(ime: &Ime) -> Option<Value> {
    match ime {
        Ime::Preedit(text, cursor, _) => {
            let u16_at = |byte_idx: usize| {
                text.get(..byte_idx)
                    .map_or(0, |prefix| prefix.encode_utf16().count()) as u32
            };
            let (start, end) = cursor
                .map(|(a, b)| (u16_at(a), u16_at(b)))
                .unwrap_or((0, 0));
            Some(json!({
                "kind": "ime",
                "text": text,
                "selectionStart": start,
                "selectionEnd": end,
            }))
        }
        Ime::Commit(text) => Some(json!({
            "kind": "text",
            "text": text,
        })),
        _ => None,
    }
}

/// App-level shortcuts that must bubble out of a focused browser pane to the app shell.
#[must_use]
pub fn should_bubble_shortcut(shortcut_id: &str) -> bool {
    matches!(
        shortcut_id,
        "commandPalette"
            | "commandPaletteAlt"
            | "settings"
            | "colorThemePicker"
            | "sidebarToggle"
            | "toggleFileExplorer"
            | "newBrowserTab"
    )
}
