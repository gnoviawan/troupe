use rune::{Key, KeyEvent, Mods};
use serde_json::json;

use super::*;

fn parse(line: &str) -> Result<Command, String> {
    serde_json::from_str(line).map_err(|e| e.to_string())
}

#[test]
fn parses_every_command() {
    assert_eq!(parse(r#"{"cmd":"tree"}"#), Ok(Command::Tree));
    assert_eq!(parse(r#"{"cmd":"a11y"}"#), Ok(Command::A11y));
    assert_eq!(
        parse(r#"{"cmd":"find","key":"press"}"#),
        Ok(Command::Find(Target::Key {
            key: "press".into()
        }))
    );
    assert_eq!(
        parse(r#"{"cmd":"find","text":"Pressed 1"}"#),
        Ok(Command::Find(Target::Text {
            text: "Pressed 1".into()
        }))
    );
    assert_eq!(
        parse(r#"{"cmd":"wait","text":"ready","timeout_ms":250}"#),
        Ok(Command::Wait {
            target: Target::Text {
                text: "ready".into()
            },
            timeout_ms: 250
        })
    );
    assert_eq!(
        parse(r#"{"cmd":"wait","key":"press"}"#),
        Ok(Command::Wait {
            target: Target::Key {
                key: "press".into()
            },
            timeout_ms: 5000
        })
    );
    assert_eq!(
        parse(r#"{"cmd":"click","key":"press"}"#),
        Ok(Command::Click(ClickAt::Key {
            key: "press".into()
        }))
    );
    assert_eq!(
        parse(r#"{"cmd":"click","x":10,"y":20.5}"#),
        Ok(Command::Click(ClickAt::Point { x: 10.0, y: 20.5 }))
    );
    assert_eq!(
        parse(r#"{"cmd":"type","text":"hi"}"#),
        Ok(Command::Type { text: "hi".into() })
    );
    assert_eq!(
        parse(r#"{"cmd":"key","name":"a","mods":["ctrl","shift"]}"#),
        Ok(Command::Key {
            name: "a".into(),
            mods: vec![Modifier::Ctrl, Modifier::Shift]
        })
    );
    assert_eq!(
        parse(r#"{"cmd":"key","name":"Enter"}"#),
        Ok(Command::Key {
            name: "Enter".into(),
            mods: vec![]
        })
    );
    assert_eq!(
        parse(r#"{"cmd":"snap","path":"/tmp/a.png"}"#),
        Ok(Command::Snap {
            path: "/tmp/a.png".into()
        })
    );
}

#[test]
fn rejects_unknown_and_incomplete_commands() {
    assert!(
        parse(r#"{"cmd":"explode"}"#)
            .unwrap_err()
            .contains("unknown variant `explode`")
    );
    assert!(parse(r#"{"cmd":"find"}"#).is_err());
    assert!(parse(r#"{"cmd":"click","x":1}"#).is_err());
    assert!(parse(r#"{"cmd":"key","name":"a","mods":["hyper"]}"#).is_err());
}

fn element(key: &str, text: Option<&str>, label: Option<&str>) -> Element {
    Element {
        key: Some(key.into()),
        path: format!("root/{key}"),
        role: "text",
        label: label.map(Into::into),
        text: text.map(Into::into),
        rect: [0.0, 0.0, 10.0, 10.0],
        focused: false,
    }
}

#[test]
fn find_matches_key_or_text_or_label() {
    let els = [
        element("title", Some("Hello"), None),
        element("press", None, Some("Press me")),
    ];
    assert_eq!(
        find(
            &els,
            &Target::Key {
                key: "press".into()
            }
        )
        .unwrap()["element"]["path"],
        "root/press"
    );
    assert_eq!(
        find(
            &els,
            &Target::Text {
                text: "Hello".into()
            }
        )
        .unwrap()["element"]["key"],
        "title"
    );
    assert_eq!(
        find(
            &els,
            &Target::Text {
                text: "Press me".into()
            }
        )
        .unwrap()["element"]["key"],
        "press"
    );
    assert_eq!(
        find(
            &els,
            &Target::Text {
                text: "press".into()
            }
        ),
        Err("not found".into())
    );
}

#[test]
fn reply_json_marks_ok_and_errors() {
    assert_eq!(
        reply_json(Ok(json!({ "at": [1.0, 2.0] }))),
        json!({ "at": [1.0, 2.0], "ok": true })
    );
    assert_eq!(
        reply_json(Err("timeout".into())),
        json!({ "ok": false, "error": "timeout" })
    );
}

#[test]
fn key_event_maps_names_and_modifiers() {
    assert_eq!(
        key_event("Enter", &[]),
        Ok(KeyEvent {
            key: Key::Enter,
            mods: Mods::default()
        })
    );
    assert_eq!(
        key_event("x", &[]),
        Ok(KeyEvent {
            key: Key::Text("x".into()),
            mods: Mods::default()
        })
    );
    assert_eq!(
        key_event("Space", &[]),
        Ok(KeyEvent {
            key: Key::Text(" ".into()),
            mods: Mods::default()
        })
    );
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    assert_eq!(
        key_event("Tab", &[Modifier::Shift]),
        Ok(KeyEvent {
            key: Key::Tab,
            mods: shift
        })
    );
    assert_eq!(key_event("Nope", &[]), Err("unknown key Nope".into()));
}

#[test]
fn primary_modifier_with_a_character_is_a_command_key() {
    let ev = key_event("A", &[Modifier::Cmd]).unwrap();
    assert_eq!(ev.key, Key::Command('a'));
    assert!(ev.mods.cmd);
    let ctrl = key_event("c", &[Modifier::Ctrl]).unwrap();
    assert!(ctrl.mods.ctrl);
    if cfg!(target_os = "macos") {
        assert_eq!(ctrl.key, Key::Text("c".into()));
    } else {
        assert_eq!(ctrl.key, Key::Command('c'));
    }
}
