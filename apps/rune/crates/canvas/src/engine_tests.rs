//! In-process `Engine` tests: exercises FigCraft's host protocol against `sample.figcraft`
//! without needing a GPU or window.

use base64::Engine as _;
use serde_json::{Value, json};

use crate::{ENGINE_VERSION, Engine, PIN_TAG};

const SAMPLE: &[u8] = include_bytes!("../../../fixtures/sample.figcraft");

fn boot(mount_seq: u64) -> Engine {
    let mut engine = Engine::new(mount_seq);
    engine.set_visible(true);
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    };
    engine.frame_unpainted(raw());
    assert!(!engine.ready(), "first frame only installs fonts");
    engine.frame_unpainted(raw());
    assert!(engine.ready(), "second frame reports ready");
    let out = engine.take_outbox();
    assert!(out.iter().any(|m| m.get("method").and_then(Value::as_str) == Some("event.ready")), "got {out:?}");
    engine
}

fn call(engine: &mut Engine, id: u64, mount_seq: u64, method: &str, params: Value) -> Value {
    engine.receive(&json!({
        "id": id,
        "method": method,
        "params": params.to_string(),
        "mountSeq": mount_seq,
    }));
    let out = engine.take_outbox();
    out.into_iter().find(|m| m.get("id").and_then(Value::as_u64) == Some(id)).expect("reply for request")
}

#[test]
fn open_edit_snapshot_and_mark_saved_round_trip() {
    let mount_seq = 11;
    let mut engine = boot(mount_seq);

    let desc = call(&mut engine, 1, mount_seq, "host.describe", json!({}));
    assert_eq!(desc["ok"], true);
    let desc_val: Value = serde_json::from_str(desc["result"].as_str().unwrap()).unwrap();
    assert_eq!(desc_val["tag"], PIN_TAG);
    assert_eq!(desc_val["engineVersion"], ENGINE_VERSION);

    let b64 = base64::engine::general_purpose::STANDARD.encode(SAMPLE);
    let opened = call(&mut engine, 2, mount_seq, "host.openDocument", json!({ "bytes": b64 }));
    assert_eq!(opened["ok"], true, "open failed: {opened}");

    let inspected = call(&mut engine, 3, mount_seq, "document.inspect", json!({}));
    assert_eq!(inspected["ok"], true);
    assert!(inspected["result"].as_str().unwrap().contains("Troupe on Rune"));

    // Simulate an agent creating a new rectangle on the canvas.
    engine.receive(&json!({
        "id": 4,
        "method": "engine.execute",
        "params": json!({
            "command": "node.create",
            "params": {
                "type": "rectangle",
                "name": "Agent Card",
                "x": 60,
                "y": 290,
                "width": 180,
                "height": 64,
                "props": { "fill": "#FF5500", "cornerRadius": 8 }
            }
        }).to_string(),
        "mountSeq": mount_seq,
    }));
    let out = engine.take_outbox();
    let reply = out.iter().find(|m| m.get("id").and_then(Value::as_u64) == Some(4)).expect("create reply");
    assert_eq!(reply["ok"], true, "create failed: {reply}");
    let dirty_event = out
        .iter()
        .find(|m| m.get("method").and_then(Value::as_str) == Some("event.dirtyChanged"))
        .expect("dirtyChanged emitted after node.create");
    let dirty_params: Value = serde_json::from_str(dirty_event["params"].as_str().unwrap()).unwrap();
    assert_eq!(dirty_params["dirty"], true);

    // Snapshot and verify the saved .figcraft bytes parse back with the new node.
    let snap = call(&mut engine, 5, mount_seq, "host.snapshot", json!({}));
    assert_eq!(snap["ok"], true);
    let snap_val: Value = serde_json::from_str(snap["result"].as_str().unwrap()).unwrap();
    let version = snap_val["contentVersion"].as_u64().unwrap();
    let bytes = base64::engine::general_purpose::STANDARD.decode(snap_val["bytes"].as_str().unwrap()).unwrap();
    let loaded = figcraft_format::load_bytes_with_versions(&bytes).expect("snapshot decodes");
    assert!(loaded.doc.nodes().any(|n| n.name == "Agent Card"), "new node persisted in snapshot");

    let marked = call(&mut engine, 6, mount_seq, "host.markSaved", json!({ "contentVersion": version }));
    assert_eq!(marked["ok"], true);
    let marked_val: Value = serde_json::from_str(marked["result"].as_str().unwrap()).unwrap();
    assert_eq!(marked_val["dirty"], false);
}

#[test]
fn host_theme_locale_file_and_events_poll_succeed() {
    let mount_seq = 5;
    let mut engine = boot(mount_seq);

    let theme = call(
        &mut engine,
        1,
        mount_seq,
        "host.theme",
        json!({
            "tokens": {
                "mode": "dark",
                "background": "#08090a",
                "card": "#111214",
                "foreground": "#f7f8f8",
                "mutedForeground": "#8a8f98",
                "border": "#23252a",
                "primary": "#5e6ad2",
                "primaryFill": "#5e6ad2",
                "destructive": "#e5484d"
            }
        }),
    );
    assert_eq!(theme["ok"], true, "{theme}");

    let locale = call(&mut engine, 2, mount_seq, "host.locale", json!({ "bcp47": "en-US" }));
    assert_eq!(locale["ok"], true, "{locale}");

    let file = call(&mut engine, 3, mount_seq, "host.file", json!({ "name": "sample", "path": "sample.figcraft", "location": "Alpha" }));
    assert_eq!(file["ok"], true, "{file}");

    let events = call(&mut engine, 4, mount_seq, "events.poll", json!({ "after": 0, "topics": ["comments"], "limit": 10 }));
    assert_eq!(events["ok"], true, "{events}");
}

#[test]
fn ui_call_returns_canvas_hidden_when_off_screen() {
    let mount_seq = 9;
    let mut engine = boot(mount_seq);
    engine.set_visible(false);

    let reply = call(&mut engine, 1, mount_seq, "ui.inspect", json!({}));
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["code"], "CANVAS_HIDDEN");
}

#[test]
fn malformed_document_is_answered_without_unwinding() {
    let mut engine = boot(17);
    let reply = call(&mut engine, 99, 17, "host.openDocument", json!({
        "bytes": base64::engine::general_purpose::STANDARD.encode(b"not a figcraft archive")
    }));
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["code"], "ENGINE_ERROR");
}
