//! `troupe-probe` against a scripted host on a loopback socket.

use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::Message;

/// Serves one socket: answers each request with `answer(request)`, sending
/// every frame it returns, until the probe hangs up.
fn host(answer: impl Fn(&Value) -> Vec<Value> + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut ws = tungstenite::accept(stream).unwrap();
        while let Ok(message) = ws.read() {
            let Message::Text(text) = message else {
                continue;
            };
            let request: Value = serde_json::from_str(&text).unwrap();
            for frame in answer(&request) {
                if ws.send(Message::text(frame.to_string())).is_err() {
                    return;
                }
            }
        }
    });
    url
}

/// Runs the probe, killing it if it outlives `limit`; `None` when it had to.
fn probe(args: &[&str], limit: Duration) -> Option<Output> {
    let (out, exited) = run(args, limit);
    exited.then_some(out)
}

/// Runs the probe for at most `limit`; the flag says whether it exited itself.
fn run(args: &[&str], limit: Duration) -> (Output, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_troupe-probe"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while started.elapsed() < limit {
        if child.try_wait().unwrap().is_some() {
            return (child.wait_with_output().unwrap(), true);
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    (child.wait_with_output().unwrap(), false)
}

fn ws_refuse_token(request: &Value) -> Vec<Value> {
    vec![
        json!({ "id": request["id"], "ok": false, "err": { "code": "unauthorized", "message": "bad token" } }),
    ]
}

#[test]
fn chat_follow_exits_with_the_error_when_the_token_is_refused() {
    let url = host(ws_refuse_token);

    let out = probe(
        &["--url", &url, "--token", "nope", "chat", "s1", "--follow"],
        Duration::from_secs(10),
    )
    .expect("the probe exited");

    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unauthorized"), "{stderr}");
}

#[test]
fn terminal_attach_exits_with_the_error_when_the_token_is_refused() {
    let url = host(|request| {
        vec![
            json!({ "id": request["id"], "success": false, "error": "bad token", "code": "UNAUTHORIZED" }),
        ]
    });

    let out = probe(
        &[
            "--url", &url, "--token", "nope", "terminal", "t1", "--attach", "--claim", "c1",
        ],
        Duration::from_secs(10),
    )
    .expect("the probe exited");

    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("UNAUTHORIZED"), "{stderr}");
}

fn chunk(seq: u64) -> Value {
    json!({ "sid": "s1", "seq": seq, "type": "message_chunk", "payload": { "content": { "type": "text", "text": "w" } } })
}

#[test]
fn chat_without_follow_exits_once_the_replay_is_printed() {
    let url = host(|request| match request["type"].as_str() {
        Some("authenticate") => {
            vec![json!({ "id": request["id"], "ok": true, "payload": { "historyMode": "server" } })]
        }
        Some("subscribe") => vec![
            json!({ "id": request["id"], "ok": true, "payload": { "sessionId": "s1", "replayed": 2, "logEpoch": "e1", "headSeq": 3 } }),
            json!({ "type": "events", "events": [chunk(1), chunk(3)] }),
        ],
        _ => vec![],
    });

    let out = probe(&["--url", &url, "chat", "s1"], Duration::from_secs(10))
        .expect("the probe exited before its idle timeout");

    assert_eq!(out.status.code(), Some(0));
    let seqs: Vec<u64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["seq"]
                .as_u64()
                .unwrap()
        })
        .collect();
    assert_eq!(seqs, vec![1, 3]);
}

#[test]
fn chat_refuses_an_unknown_option() {
    let out = probe(
        &["--url", "http://127.0.0.1:9", "chat", "s1", "--folow"],
        Duration::from_secs(10),
    )
    .expect("the probe exited");

    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--folow"));
}

#[test]
fn a_terminal_replay_with_a_gap_is_reported_on_stderr() {
    let url = host(|request| match request["type"].as_str() {
        Some("authenticate") => vec![json!({ "id": request["id"], "success": true, "data": {} })],
        Some("attach") => vec![json!({
            "type": "replay", "terminalId": "t1", "chunks": [{ "seq": 7, "data": [104, 105] }],
            "gap": true, "reset": true, "logEpoch": "e1", "latestSeq": 7,
            "snapshot": { "exited": false }
        })],
        _ => vec![],
    });

    let (out, _) = run(
        &["--url", &url, "terminal", "t1", "--attach", "--claim", "c1"],
        Duration::from_secs(3),
    );

    assert_eq!(out.stdout, b"hi");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("t1 replay lost output"), "{stderr}");
}
