//! The host's `/ws` parsers and writers against the recorded `troupe-protocol`
//! fixtures: every request parses the way its handler reads it, and every
//! frame the host writes comes out as it was recorded.

use serde::de::DeserializeOwned;
use troupe_protocol::ws::{
    AcpAuthenticateReply, ChatReadMark, DeleteSessionPayload, RecordAgentSwitchPayload,
    StoreDeletePayload, StoreReadPayload, StoreWritePayload,
};

use super::*;

fn read(file: &str) -> &'static str {
    match file {
        "ws-request.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/ws-request.json")
        }
        "ws-reply.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/ws-reply.json")
        }
        "ws-event.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/ws-event.json")
        }
        "ws-frame.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/ws-frame.json")
        }
        "terminal-request.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/terminal-request.json")
        }
        "terminal-reply.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/terminal-reply.json")
        }
        "terminal-frame.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/terminal-frame.json")
        }
        "handwritten.json" => {
            include_str!("../../../../crates/troupe-protocol/fixtures/handwritten.json")
        }
        other => panic!("no fixture file {other}"),
    }
}

/// `(name, frame, recorded)` for every fixture of `file`: the frames recorded
/// from a live host, then the hand-written ones.
pub(crate) fn fixtures(file: &str) -> Vec<(String, Value, bool)> {
    let recorded: serde_json::Map<String, Value> =
        serde_json::from_str(read(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    let handwritten: Vec<Value> =
        serde_json::from_str(read("handwritten.json")).expect("handwritten.json");
    let handwritten = handwritten
        .into_iter()
        .filter(|entry| entry["file"] == file)
        .map(|entry| {
            let name = entry["name"].as_str().expect("fixture name").to_string();
            (name, entry["frame"].clone(), false)
        });
    let all: Vec<_> = recorded
        .into_iter()
        .map(|(name, frame)| (name, frame, true))
        .chain(handwritten)
        .collect();
    assert!(!all.is_empty(), "{file} holds no fixture");
    all
}

/// A recorded frame must come out byte for byte, key order included; a
/// hand-written one must be the same JSON value.
pub(crate) fn assert_wire(name: &str, text: Option<String>, frame: &Value, recorded: bool) {
    let text = text.unwrap_or_else(|| panic!("{name}: the host wrote nothing"));
    if recorded {
        assert_eq!(text, frame.to_string(), "{name}");
    } else {
        let written: Value = serde_json::from_str(&text).expect("the host writes JSON");
        assert_eq!(&written, frame, "{name}");
    }
}

fn parses<T: DeserializeOwned>(payload: &Value) -> Result<(), serde_json::Error> {
    serde_json::from_value::<T>(payload.clone()).map(drop)
}

#[test]
fn every_ws_request_parses_as_its_handler_reads_it() {
    for (name, frame, _) in fixtures("ws-request.json") {
        let request: WsRequest =
            serde_json::from_value(frame.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut read = json!({
            "id": request.id,
            "type": request.type_,
            "payload": request.payload,
        });
        if let Some(command_id) = &request.command_id {
            read["command_id"] = json!(command_id);
        }
        assert_eq!(read, frame, "{name}");
        let typed = match request.type_.as_str() {
            "mark_chat_read" => parses::<ChatReadMark>(&request.payload),
            "store_read" => parses::<StoreReadPayload>(&request.payload),
            "store_write" => parses::<StoreWritePayload>(&request.payload),
            "store_delete" => parses::<StoreDeletePayload>(&request.payload),
            "delete_session" => parses::<DeleteSessionPayload>(&request.payload),
            "record_agent_switch" => parses::<RecordAgentSwitchPayload>(&request.payload),
            _ => Ok(()),
        };
        if let Err(error) = typed {
            panic!("{name}: the handler refuses the recorded payload: {error}");
        }
    }
}

#[test]
fn a_typed_payload_with_an_unknown_field_is_refused() {
    let error = parses::<StoreReadPayload>(&json!({ "key": "device/d/k", "extra": 1 }))
        .expect_err("an unknown field is refused");
    assert_eq!(error.to_string(), "unknown field `extra`, expected `key`");
}

/// The reply the handlers build for a recorded reply frame.
fn host_reply(name: &str, frame: &Value) -> WsReply {
    let id = frame["id"].as_str().expect("reply id").to_string();
    let mut reply = if name == "authenticate" {
        let payload: AcpAuthenticateReply =
            serde_json::from_value(frame["payload"].clone()).expect("authenticate reply");
        ok_with_payload(id, &payload)
    } else if frame["ok"] == true {
        WsReply::ok(id, frame.get("payload").cloned())
    } else {
        let err = &frame["err"];
        let mut reply = WsReply::err_with_code(
            id,
            err["code"].as_str().expect("err.code"),
            err["message"].as_str().expect("err.message"),
        );
        if let (Some(details), Some(host_err)) = (err.get("details"), reply.err.as_mut()) {
            host_err.details = Some(details.clone());
        }
        reply
    };
    reply.visible_at = frame.get("visible_at").cloned();
    reply
}

#[test]
fn every_ws_reply_is_written_as_recorded() {
    for (name, frame, recorded) in fixtures("ws-reply.json") {
        let reply = host_reply(&name, &frame);
        assert_wire(&name, serde_json::to_string(&reply).ok(), &frame, recorded);
    }
}

fn sequenced(frame: &Value) -> SequencedEvent {
    SequencedEvent::new(
        frame["sid"].as_str().map(str::to_string),
        frame["seq"].as_u64().expect("event seq"),
        frame["type"].as_str().expect("event type"),
        frame["payload"].clone(),
    )
}

#[test]
fn every_ws_event_and_frame_is_written_as_recorded() {
    for (name, frame, recorded) in fixtures("ws-event.json") {
        let text = events_text(std::slice::from_ref(&sequenced(&frame)));
        assert_wire(&name, text, &frame, recorded);
    }
    let mut written = Vec::new();
    for (name, frame, recorded) in fixtures("ws-frame.json") {
        let text = match name.as_str() {
            "events" => {
                let events = frame["events"].as_array().expect("batched events");
                events_text(&events.iter().map(sequenced).collect::<Vec<_>>())
            }
            "heartbeat" => heartbeat_text(),
            "gap" => gap_text(frame["sid"].as_str().expect("gap sid").to_string()),
            // `acp::host_mcp::canvas` writes the canvas frames.
            _ => continue,
        };
        assert_wire(&name, text, &frame, recorded);
        written.push(name);
    }
    written.sort();
    assert_eq!(written, ["events", "gap", "heartbeat"]);
}
