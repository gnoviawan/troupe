//! The host's `/terminal/ws` parser and writers against the recorded
//! `troupe-protocol` fixtures.

use crate::web::ws::protocol_fixture_tests::{assert_wire, fixtures};

use super::*;

#[test]
fn every_terminal_request_parses_as_the_host_reads_it() {
    for (name, frame, _) in fixtures("terminal-request.json") {
        let request: Request =
            serde_json::from_value(frame.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut read = json!({
            "id": request.id,
            "type": request.type_,
            "payload": request.payload,
        });
        if let Some(command_id) = &request.command_id {
            read["commandId"] = json!(command_id);
        }
        assert_eq!(read, frame, "{name}");
    }
}

fn text_field(frame: &Value, key: &str) -> String {
    frame[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} is a string"))
        .to_string()
}

fn seq_field(frame: &Value, key: &str) -> u64 {
    frame[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} is a number"))
}

#[tokio::test]
async fn every_terminal_reply_and_frame_is_written_as_recorded() {
    let (tx, mut rx) = mpsc::channel(1);
    let mut written = Vec::new();
    let frames = fixtures("terminal-reply.json")
        .into_iter()
        .chain(fixtures("terminal-frame.json"));
    for (name, frame, recorded) in frames {
        let sent = match frame.get("type").and_then(Value::as_str) {
            None if frame["success"] == true => {
                let reply = WebTerminalOkReply {
                    id: text_field(&frame, "id"),
                    success: True,
                    data: frame["data"].clone(),
                };
                send_json(&tx, reply).await
            }
            None => {
                let code = text_field(&frame, "code");
                let error = text_field(&frame, "error");
                send_error(&tx, &text_field(&frame, "id"), &code, error).await
            }
            Some("data") => {
                let data = serde_json::from_value(frame["data"].clone()).expect("output bytes");
                let chunk = WebTerminalDataFrame {
                    kind: DataTag,
                    terminal_id: text_field(&frame, "terminalId"),
                    seq: seq_field(&frame, "seq"),
                    data,
                };
                send_json(&tx, chunk).await
            }
            Some("gap") => {
                let gap = WebTerminalGapFrame {
                    kind: TerminalGapTag,
                    terminal_id: text_field(&frame, "terminalId"),
                    last_seq: seq_field(&frame, "lastSeq"),
                };
                send_json(&tx, gap).await
            }
            // The `replay` and `event` frames carry tracker and PTY types.
            Some(_) => continue,
        };
        sent.unwrap_or_else(|e| panic!("{name}: {e}"));
        let Some(Message::Text(text)) = rx.recv().await else {
            panic!("{name}: no text frame");
        };
        assert_wire(&name, Some(text.as_str().to_owned()), &frame, recorded);
        written.push(name);
    }
    for kind in ["data", "gap"] {
        assert!(written.iter().any(|name| name == kind), "no {kind} frame");
    }
    assert!(written.len() > 2, "no reply was written");
}
