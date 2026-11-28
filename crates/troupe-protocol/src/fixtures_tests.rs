//! Every recorded frame parses into its contract type and serializes back to
//! the same JSON value. A field the type drops or renames fails here.

use std::collections::BTreeMap;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::terminal::*;
use crate::ws::*;

/// One hand-written fixture: a frame the recorder cannot produce live.
#[derive(Deserialize)]
struct Handwritten {
    file: String,
    name: String,
    #[serde(rename = "_handwritten")]
    handwritten: bool,
    source: String,
    frame: Value,
}

fn read(file: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn handwritten() -> Vec<Handwritten> {
    serde_json::from_value(read("handwritten.json")).expect("handwritten.json entries")
}

/// The live frames of `file` plus its hand-written ones, by name.
fn fixtures(file: &str) -> BTreeMap<String, Value> {
    let mut frames: BTreeMap<String, Value> =
        serde_json::from_value(read(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    for entry in handwritten().into_iter().filter(|entry| entry.file == file) {
        assert!(
            frames.insert(entry.name.clone(), entry.frame).is_none(),
            "{file} {} is both live and hand-written",
            entry.name
        );
    }
    frames
}

/// Parse `value` as `T`, serialize it back and require the same JSON value.
fn round_trip<T: DeserializeOwned + Serialize>(label: &str, value: &Value) -> T {
    let parsed: T = serde_json::from_value(value.clone()).unwrap_or_else(|e| {
        panic!(
            "{label} does not parse as {}: {e}",
            std::any::type_name::<T>()
        )
    });
    let back = serde_json::to_value(&parsed).expect("serialize");
    assert_eq!(
        &back,
        value,
        "{label} changed on the round trip through {}",
        std::any::type_name::<T>()
    );
    parsed
}

fn payload<'a>(label: &str, frame: &'a Value) -> &'a Value {
    frame
        .get("payload")
        .unwrap_or_else(|| panic!("{label} has no payload"))
}

#[test]
fn every_ws_request_round_trips() {
    let frames = fixtures("ws-request.json");
    for kind in WsRequestType::ALL {
        assert!(
            frames.contains_key(kind.as_str()),
            "no ws-request fixture for {}",
            kind.as_str()
        );
    }
    for (name, frame) in &frames {
        let label = format!("ws-request {name}");
        let request: WsRequest = round_trip(&label, frame);
        assert_eq!(request.kind.as_str(), name);
        let body = &request.payload;
        match request.kind {
            WsRequestType::Authenticate => drop(round_trip::<AcpAuthenticatePayload>(&label, body)),
            WsRequestType::StoreRead => drop(round_trip::<StoreReadPayload>(&label, body)),
            WsRequestType::StoreWrite => drop(round_trip::<StoreWritePayload>(&label, body)),
            WsRequestType::StoreDelete => drop(round_trip::<StoreDeletePayload>(&label, body)),
            WsRequestType::AddProject => drop(round_trip::<AddProjectPayload>(&label, body)),
            WsRequestType::UpdateProject => drop(round_trip::<UpdateProjectPayload>(&label, body)),
            WsRequestType::RemoveProject => drop(round_trip::<RemoveProjectPayload>(&label, body)),
            WsRequestType::DeleteSession => drop(round_trip::<DeleteSessionPayload>(&label, body)),
            WsRequestType::RecordAgentSwitch => {
                drop(round_trip::<RecordAgentSwitchPayload>(&label, body));
            }
            WsRequestType::MarkChatRead => drop(round_trip::<ChatReadMark>(&label, body)),
            WsRequestType::CanvasReply => drop(round_trip::<WsCanvasReplyPayload>(&label, body)),
            WsRequestType::SshSessionRespondHostKey => {
                drop(round_trip::<SshHostKeyAnswer>(&label, body));
            }
            _ => {}
        }
    }
}

/// The typed reply payload of a request, where the contract names one. A
/// fixture named `<request>.<case>` is a second reply to the same request.
fn check_reply_payload(name: &str, label: &str, body: &Value) {
    match name {
        "authenticate" => drop(round_trip::<AcpAuthenticateReply>(label, body)),
        "subscribe" | "open_persisted_session" => {
            drop(round_trip::<SessionSubscribeReply>(label, body));
        }
        "recover_session_snapshot" => drop(round_trip::<SessionSnapshotEvent>(label, body)),
        "get_session_cursor" => drop(round_trip::<SessionCursorReply>(label, body)),
        "list_persisted_sessions" => drop(round_trip::<Vec<PersistedSessionSummary>>(label, body)),
        "list_agents" => drop(round_trip::<Vec<WsAgentSummary>>(label, body)),
        "store_read" => drop(round_trip::<StoreReadReply>(label, body)),
        "store_write" => drop(round_trip::<StoreDeviceWriteReply>(label, body)),
        "store_delete" => drop(round_trip::<StoreDeleteReply>(label, body)),
        "watch" => drop(round_trip::<WsWatchReply>(label, body)),
        "mark_chat_read" => drop(round_trip::<ChatReadMarkReply>(label, body)),
        "canvas_mount" => drop(round_trip::<WsCanvasMountReply>(label, body)),
        _ => {}
    }
}

#[test]
fn every_ws_reply_round_trips() {
    let frames = fixtures("ws-reply.json");
    for kind in WsRequestType::ALL {
        assert!(
            frames.contains_key(kind.as_str()),
            "no ws-reply fixture for {}",
            kind.as_str()
        );
    }
    for (name, frame) in &frames {
        let label = format!("ws-reply {name}");
        round_trip::<WsServerFrame>(&label, frame);
        match round_trip::<WsReply>(&label, frame) {
            WsReply::Ok(reply) => {
                if let Some(body) = &reply.payload {
                    let request = name.split('.').next().unwrap_or(name);
                    check_reply_payload(request, &label, body);
                }
            }
            WsReply::Err(reply) => {
                let code = &reply.err.code;
                let known = WsErrorCode::ALL.iter().any(|known| known.as_str() == code);
                let domain = code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_');
                assert!(known || domain, "{label}: unknown error code {code}");
            }
        }
    }
}

/// Event types that never travel as a `/ws` event: the two delivery frames,
/// and the agent-level facts the `agents` watch carries.
fn carried_elsewhere(kind: WsEventType) -> Option<(&'static str, &'static str)> {
    match kind {
        WsEventType::Gap => Some(("ws-frame.json", "gap")),
        WsEventType::Heartbeat => Some(("ws-frame.json", "heartbeat")),
        WsEventType::AgentSpawned => Some(("ws-watch.json", "agents.delta.agent_spawned")),
        WsEventType::AgentDisconnected => {
            Some(("ws-watch.json", "agents.delta.agent_disconnected"))
        }
        _ => None,
    }
}

#[test]
fn every_ws_event_round_trips() {
    let frames = fixtures("ws-event.json");
    for kind in WsEventType::ALL {
        match carried_elsewhere(*kind) {
            Some((file, name)) => assert!(
                fixtures(file).contains_key(name),
                "no {file} fixture {name} for event {}",
                kind.as_str()
            ),
            None => assert!(
                frames.contains_key(kind.as_str()),
                "no ws-event fixture for {}",
                kind.as_str()
            ),
        }
    }
    for (name, frame) in &frames {
        let label = format!("ws-event {name}");
        round_trip::<WsServerFrame>(&label, frame);
        let event: WsEvent = round_trip(&label, frame);
        assert_eq!(event.kind.as_str(), name);
        if event.kind == WsEventType::UserPrompt {
            round_trip::<UserPromptEvent>(&label, &event.payload);
        }
    }
}

#[test]
fn every_ws_frame_round_trips() {
    for (name, frame) in &fixtures("ws-frame.json") {
        let label = format!("ws-frame {name}");
        round_trip::<WsServerFrame>(&label, frame);
        match name.as_str() {
            "events" => {
                let batch: WsEventBatchFrame = round_trip(&label, frame);
                assert!(batch.events.len() > 1, "{label} holds one event");
            }
            "gap" => drop(round_trip::<WsGapFrame>(&label, frame)),
            "heartbeat" => drop(round_trip::<WsHeartbeatFrame>(&label, frame)),
            "canvas_call" => drop(round_trip::<WsCanvasCallFrame>(&label, frame)),
            "canvas_lease_lost" => drop(round_trip::<WsCanvasLeaseLostFrame>(&label, frame)),
            other => panic!("unexpected ws-frame fixture {other}"),
        }
    }
}

fn check_watch_body(name: &str, label: &str, body: &Value) {
    match name {
        "chat_activity.state" => drop(round_trip::<ChatActivityWatchState>(label, body)),
        "chat_activity.delta" => drop(round_trip::<ChatActivityWatchDelta>(label, body)),
        "chat_history.delta" => drop(round_trip::<ChatHistoryWatchDelta>(label, body)),
        "fs.state" => drop(round_trip::<FsWatchState>(label, body)),
        "fs.delta" => drop(round_trip::<FsWatchDelta>(label, body)),
        "store.state" | "store.delta" => drop(round_trip::<StoreWatchState>(label, body)),
        "terminals.state" | "terminals.delta" => {
            drop(round_trip::<TerminalsWatchState>(label, body));
        }
        agents if agents.starts_with("agents.delta.") => {
            drop(round_trip::<AgentsWatchDelta>(label, body));
        }
        "ssh.state" => drop(round_trip::<SshWatchState>(label, body)),
        ssh if ssh.starts_with("ssh.delta.") => {
            let delta: SshWatchDelta = round_trip(label, body);
            assert_eq!(
                body["event"].as_str(),
                ssh.strip_prefix("ssh.delta."),
                "{label} names its event"
            );
            drop(delta);
        }
        _ => {}
    }
}

#[test]
fn every_watch_frame_round_trips() {
    let frames = fixtures("ws-watch.json");
    for watch in WsWatchName::ALL {
        let watch = watch.as_str();
        assert!(
            frames.contains_key(&format!("{watch}.state")),
            "no {watch} state fixture"
        );
        assert!(
            frames
                .keys()
                .any(|name| name.starts_with(&format!("{watch}.delta"))),
            "no {watch} delta fixture"
        );
    }
    for (name, frame) in &frames {
        let label = format!("ws-watch {name}");
        round_trip::<WsServerFrame>(&label, frame);
        let body = match round_trip::<WsWatchFrame>(&label, frame) {
            WsWatchFrame::State(state) => {
                assert!(name.ends_with(".state"), "{label} is a state frame");
                state.state
            }
            WsWatchFrame::Delta(delta) => {
                assert!(name.contains(".delta"), "{label} is a delta frame");
                delta.delta
            }
        };
        check_watch_body(name, &label, &body);
    }
}

#[test]
fn every_terminal_request_round_trips() {
    let frames = fixtures("terminal-request.json");
    for kind in WebTerminalRequestType::ALL {
        assert!(
            frames.contains_key(kind.as_str()),
            "no terminal-request fixture for {}",
            kind.as_str()
        );
    }
    for (name, frame) in &frames {
        let label = format!("terminal-request {name}");
        let request: WebTerminalRequest = round_trip(&label, frame);
        assert_eq!(request.kind.as_str(), name);
        if request.kind == WebTerminalRequestType::Spawn {
            round_trip::<TerminalSpawnOptions>(&label, &request.payload);
        }
    }
}

#[test]
fn every_terminal_reply_round_trips() {
    let frames = fixtures("terminal-reply.json");
    for kind in WebTerminalRequestType::ALL {
        assert!(
            frames.contains_key(kind.as_str()),
            "no terminal-reply fixture for {}",
            kind.as_str()
        );
    }
    for (name, frame) in &frames {
        let label = format!("terminal-reply {name}");
        round_trip::<WebTerminalFrame>(&label, frame);
        let WebTerminalReply::Ok(reply) = round_trip::<WebTerminalReply>(&label, frame) else {
            continue;
        };
        match name.as_str() {
            "spawn" => drop(round_trip::<SpawnedTerminal>(&label, &reply.data)),
            "attach" => drop(round_trip::<TerminalAttachResult>(&label, &reply.data)),
            "rotate_claim" => drop(round_trip::<RotatedClaim>(&label, &reply.data)),
            "get_git_status" => drop(round_trip::<Option<GitStatus>>(&label, &reply.data)),
            _ => {}
        }
    }
}

#[test]
fn every_terminal_frame_round_trips() {
    let frames = fixtures("terminal-frame.json");
    for name in [
        "data",
        "replay",
        "gap",
        "event.session_lifecycle",
        "event.exit",
        "event.cwd_changed",
        "event.git_branch_changed",
        "event.git_status_changed",
        "event.exit_code_changed",
    ] {
        assert!(
            frames.contains_key(name),
            "no terminal-frame fixture {name}"
        );
    }
    for (name, frame) in &frames {
        let label = format!("terminal-frame {name}");
        round_trip::<WebTerminalFrame>(&label, frame);
        match name.as_str() {
            "data" => drop(round_trip::<WebTerminalDataFrame>(&label, frame)),
            "replay" => drop(round_trip::<WebTerminalReplayFrame>(&label, frame)),
            "gap" => drop(round_trip::<WebTerminalGapFrame>(&label, frame)),
            _ => {
                let event: WebTerminalEventFrame = round_trip(&label, frame);
                let kind = payload(&label, frame)["type"].as_str().unwrap_or_default();
                assert_eq!(name.strip_prefix("event."), Some(kind));
                drop(event);
            }
        }
    }
}

#[test]
fn handwritten_fixtures_name_their_source() {
    let entries = handwritten();
    assert!(!entries.is_empty());
    for entry in entries {
        assert!(
            entry.handwritten,
            "{} {} is not marked _handwritten",
            entry.file, entry.name
        );
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(entry.source.split_whitespace().next().unwrap_or_default())
                .exists(),
            "{} {}: source {} does not name a file",
            entry.file,
            entry.name,
            entry.source
        );
    }
}

#[test]
fn a_read_mark_reply_with_a_new_field_still_parses_and_a_request_does_not() {
    let body = serde_json::json!({
        "sessionId": "s1",
        "seq": 7,
        "logEpoch": "e1",
        "unreadCount": 0
    });
    let reply: ChatReadMarkReply = serde_json::from_value(body.clone()).expect("lenient reply");
    assert_eq!(
        reply,
        ChatReadMarkReply {
            session_id: "s1".into(),
            seq: 7,
            log_epoch: Some("e1".into()),
        }
    );
    assert!(serde_json::from_value::<ChatReadMark>(body).is_err());
}
