//! The `/ws` machine through loss, gap and replay, with no network: the test
//! plays the host by feeding frames and reading what the machine sends.

use rune_core::machine::Attempt;
use serde_json::{Value, json};
use troupe_protocol::ws::{WsEvent, WsWatchName};

use crate::cursor::Cursor;
use crate::link::Status;
use crate::machine::{Config, Effect, Event, Machine, SocketEvent};
use crate::ws::{Command, Output, WsClient};

pub(crate) fn config() -> Config {
    Config {
        base_url: "http://127.0.0.1:8190".into(),
        token: Some("t0ken".into()),
        device_id: "dev-1".into(),
        client_instance_id: "tab-1".into(),
        seed: 7,
    }
}

struct Host {
    client: WsClient,
    now: f64,
    socket: Option<Attempt>,
    sent: Vec<Value>,
}

impl Host {
    fn new() -> Self {
        Self {
            client: WsClient::new(config()),
            now: 0.0,
            socket: None,
            sent: Vec::new(),
        }
    }

    fn step(&mut self, event: Event<Command>) -> Vec<Output> {
        let mut out = Vec::new();
        for effect in self.client.step(event, self.now) {
            match effect {
                Effect::Open { socket, url } => {
                    assert_eq!(url, "ws://127.0.0.1:8190/ws");
                    self.socket = Some(socket);
                }
                Effect::Send { socket, text } => {
                    assert_eq!(Some(socket), self.socket, "sent on the current socket");
                    self.sent.push(serde_json::from_str(&text).unwrap());
                }
                Effect::Close { .. } | Effect::WakeAt(_) => {}
                Effect::Output(o) => out.push(o),
            }
        }
        out
    }

    fn command(&mut self, command: Command) -> Vec<Output> {
        self.step(Event::Command(command))
    }

    fn frame(&mut self, frame: Value) -> Vec<Output> {
        let socket = self.socket.unwrap();
        self.step(Event::Socket(SocketEvent::Text(socket, frame.to_string())))
    }

    /// The last request of `kind` the machine sent.
    fn request(&self, kind: &str) -> Value {
        self.sent
            .iter()
            .rev()
            .find(|r| r["type"] == kind)
            .cloned()
            .unwrap_or_else(|| panic!("no {kind} sent"))
    }

    fn requests(&self, kind: &str) -> Vec<Value> {
        self.sent
            .iter()
            .filter(|r| r["type"] == kind)
            .cloned()
            .collect()
    }

    fn reply(&mut self, kind: &str, payload: Value) -> Vec<Output> {
        let id = self.request(kind)["id"].clone();
        self.frame(json!({ "id": id, "ok": true, "payload": payload }))
    }

    fn refuse(&mut self, kind: &str, code: &str) -> Vec<Output> {
        let id = self.request(kind)["id"].clone();
        self.frame(json!({ "id": id, "ok": false, "err": { "code": code, "message": code } }))
    }

    /// Open the socket and authenticate.
    fn connect(&mut self) -> Vec<Output> {
        let mut out = self.step(Event::Start);
        out.extend(self.step(Event::Socket(SocketEvent::Opened(self.socket.unwrap()))));
        out.extend(self.reply(
            "authenticate",
            json!({ "historyMode": "server", "protocolVersion": 7 }),
        ));
        out
    }

    /// The live socket dies; the backoff runs out and a new one authenticates.
    fn reconnect(&mut self) -> Vec<Output> {
        let socket = self.socket.unwrap();
        let mut out = self.step(Event::Socket(SocketEvent::Closed {
            socket,
            code: Some(1006),
        }));
        self.now += 10.0;
        out.extend(self.step(Event::Tick));
        assert_ne!(self.socket, Some(socket), "a new socket opened");
        out.extend(self.step(Event::Socket(SocketEvent::Opened(self.socket.unwrap()))));
        out.extend(self.reply("authenticate", json!({ "historyMode": "server" })));
        out
    }

    fn follow(&mut self) {
        self.connect();
        self.command(Command::Subscribe {
            session_id: "s1".into(),
            cursor: Cursor::default(),
        });
        self.frame(batch(&[1, 2, 3]));
        self.reply("subscribe", subscribed(3));
    }
}

fn chunk(seq: u64) -> Value {
    json!({ "sid": "s1", "seq": seq, "type": "message_chunk", "payload": { "content": { "type": "text", "text": format!("w{seq} ") } } })
}

fn batch(seqs: &[u64]) -> Value {
    json!({ "type": "events", "events": seqs.iter().map(|s| chunk(*s)).collect::<Vec<_>>() })
}

fn subscribed(head: u64) -> Value {
    json!({ "sessionId": "s1", "replayed": head, "logEpoch": "e1", "headSeq": head })
}

/// The seqs of the session events among `out`.
fn seqs(out: &[Output]) -> Vec<u64> {
    out.iter()
        .filter_map(|o| match o {
            Output::Event(WsEvent {
                sid: Some(_), seq, ..
            }) => Some(*seq),
            _ => None,
        })
        .collect()
}

#[test]
fn replay_that_overtakes_the_subscribe_reply_is_applied_after_it() {
    let mut host = Host::new();
    host.connect();
    host.command(Command::Subscribe {
        session_id: "s1".into(),
        cursor: Cursor::default(),
    });
    assert_eq!(
        host.request("subscribe")["payload"],
        json!({ "sessionId": "s1", "lastSeq": 0 })
    );

    assert_eq!(host.frame(batch(&[1, 2, 3])), vec![]);
    let out = host.reply("subscribe", subscribed(3));

    assert!(matches!(&out[0], Output::Subscribed(r) if r.head_seq == 3));
    assert_eq!(seqs(&out), vec![1, 2, 3]);
}

#[test]
fn a_lost_socket_resubscribes_from_the_cursor_and_skips_what_was_shown() {
    let mut host = Host::new();
    host.follow();
    host.frame(batch(&[4]));

    let out = host.reconnect();
    assert!(out.contains(&Output::Status(Status::Live)));
    assert_eq!(
        host.request("subscribe")["payload"],
        json!({ "sessionId": "s1", "lastSeq": 4, "logEpoch": "e1" })
    );

    // The host replays from its own frontier, overlapping what was shown.
    host.frame(batch(&[3, 4, 5, 6]));
    let mut shown = seqs(&host.reply("subscribe", subscribed(6)));
    shown.extend(seqs(&host.frame(batch(&[7]))));
    assert_eq!(shown, vec![5, 6, 7]);
}

#[test]
fn a_skipped_seq_installs_a_snapshot_and_then_applies_what_followed() {
    let mut host = Host::new();
    host.follow();

    assert_eq!(seqs(&host.frame(batch(&[5]))), Vec::<u64>::new());
    assert_eq!(
        host.request("recover_session_snapshot")["payload"],
        json!({ "sessionId": "s1" })
    );
    assert_eq!(seqs(&host.frame(batch(&[6]))), Vec::<u64>::new());

    let snapshot = json!({
        "sessionId": "s1", "watermark": 5, "headSeq": 5, "logEpoch": "e1",
        "events": [chunk(1), chunk(2), chunk(3), chunk(4), chunk(5)]
    });
    let out = host.reply("recover_session_snapshot", snapshot);

    assert!(matches!(&out[0], Output::Snapshot(s) if s.watermark == 5 && s.events.len() == 5));
    assert_eq!(seqs(&out), vec![6]);
    assert_eq!(host.client.cursor("s1").map(|c| c.last_seq), Some(6));
}

#[test]
fn a_gap_frame_installs_a_snapshot() {
    let mut host = Host::new();
    host.follow();

    host.frame(json!({ "type": "gap", "sid": "s1" }));

    assert_eq!(host.requests("recover_session_snapshot").len(), 1);
}

#[test]
fn a_stale_cursor_installs_a_snapshot() {
    let mut host = Host::new();
    host.follow();
    host.reconnect();

    host.refuse("subscribe", "stale");

    assert_eq!(
        host.request("recover_session_snapshot")["payload"],
        json!({ "sessionId": "s1" })
    );
}

#[test]
fn a_new_log_epoch_under_the_cursor_installs_a_snapshot() {
    let mut host = Host::new();
    host.follow();
    host.reconnect();

    let out = host.reply(
        "subscribe",
        json!({ "sessionId": "s1", "replayed": 0, "logEpoch": "e2", "headSeq": 9 }),
    );

    assert_eq!(out, vec![]);
    assert_eq!(host.requests("recover_session_snapshot").len(), 1);
}

#[test]
fn a_ledgered_command_is_resent_with_its_command_id_and_answered_once() {
    let mut host = Host::new();
    host.connect();
    let payload = json!({ "agentId": "a1", "sessionId": "s1", "text": "hi" });
    host.command(Command::Call {
        call: 9,
        kind: troupe_protocol::ws::WsRequestType::SendPrompt,
        payload: payload.clone(),
        ledgered: true,
    });
    let first = host.request("send_prompt");
    assert_eq!(first["command_id"], "tab-1-w7-1");

    let out = host.reconnect();
    assert!(!out.iter().any(|o| matches!(o, Output::Reply { .. })));
    let again = host.request("send_prompt");
    assert_eq!(again["command_id"], "tab-1-w7-1");
    assert_eq!(again["payload"], payload);
    assert_ne!(again["id"], first["id"]);

    let out = host.reply("send_prompt", json!({ "stopReason": "end_turn" }));
    assert_eq!(
        out,
        vec![Output::Reply {
            call: 9,
            result: Ok(Some(json!({ "stopReason": "end_turn" })))
        }]
    );
}

#[test]
fn a_prompt_with_a_turn_id_is_resent_under_that_id_and_its_receipt_answers() {
    let mut host = Host::new();
    host.connect();
    let payload = json!({ "agentId": "a1", "sessionId": "s1", "turnId": "turn-42", "content": [] });
    host.command(Command::Call {
        call: 5,
        kind: troupe_protocol::ws::WsRequestType::SendPrompt,
        payload,
        ledgered: true,
    });
    assert_eq!(host.request("send_prompt")["command_id"], "turn-42");

    let out = host.reconnect();
    assert!(!out.iter().any(|o| matches!(o, Output::Reply { .. })));
    assert_eq!(host.request("send_prompt")["command_id"], "turn-42");

    let out = host.reply("send_prompt", json!({ "receipt": "in_progress" }));
    assert_eq!(
        out,
        vec![Output::Reply {
            call: 5,
            result: Ok(Some(json!({ "receipt": "in_progress" })))
        }]
    );
}

#[test]
fn an_unledgered_request_on_a_lost_socket_fails_with_closed() {
    let mut host = Host::new();
    host.connect();
    host.command(Command::Call {
        call: 3,
        kind: troupe_protocol::ws::WsRequestType::ListPersistedSessions,
        payload: json!({}),
        ledgered: false,
    });

    let out = host.reconnect();

    let failed: Vec<(u64, String)> = out
        .iter()
        .filter_map(|o| match o {
            Output::Reply {
                call,
                result: Err(e),
            } => Some((*call, e.code.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(failed, vec![(3, "closed".to_owned())]);
    assert_eq!(host.requests("list_persisted_sessions").len(), 1);
}

#[test]
fn a_command_the_host_still_runs_is_asked_again_after_a_second() {
    let mut host = Host::new();
    host.connect();
    host.command(Command::Call {
        call: 4,
        kind: troupe_protocol::ws::WsRequestType::CreateSession,
        payload: json!({ "agentId": "a1" }),
        ledgered: true,
    });

    assert_eq!(
        host.reply("create_session", json!({ "receipt": "in_progress" })),
        vec![]
    );
    host.now += 1.0;
    host.step(Event::Tick);

    let resends = host.requests("create_session");
    assert_eq!(resends.len(), 2);
    assert_eq!(resends[1]["command_id"], resends[0]["command_id"]);
}

#[test]
fn a_resent_prompt_complete_for_a_seen_turn_is_not_shown_twice() {
    let mut host = Host::new();
    host.follow();
    let done = |seq: u64| json!({ "sid": "s1", "seq": seq, "type": "prompt_complete", "payload": { "turnId": "t1" } });

    let first = host.frame(done(4));
    let second = host.frame(done(5));

    assert_eq!(seqs(&first), vec![4]);
    assert_eq!(seqs(&second), Vec::<u64>::new());
    assert_eq!(seqs(&host.frame(batch(&[6]))), vec![6]);
}

#[test]
fn a_skipped_watch_revision_rewatches_and_a_reconnect_rewatches_every_watch() {
    let mut host = Host::new();
    host.connect();
    host.command(Command::Watch {
        watch: WsWatchName::Projects,
        scope: String::new(),
    });
    let frame = |n: u64, body: Value| {
        let mut f = json!({ "type": "watch", "watch": "projects", "scope": "", "revision": { "hostEpoch": "h1", "n": n } });
        f.as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        f
    };

    let state = host.frame(frame(
        1,
        json!({ "reset": true, "state": { "projects": [] } }),
    ));
    assert_eq!(
        state,
        vec![Output::WatchState {
            watch: WsWatchName::Projects,
            scope: String::new(),
            state: json!({ "projects": [] })
        }]
    );
    let delta = host.frame(frame(2, json!({ "delta": { "projects": ["a"] } })));
    assert_eq!(
        delta,
        vec![Output::WatchDelta {
            watch: WsWatchName::Projects,
            scope: String::new(),
            delta: json!({ "projects": ["a"] })
        }]
    );

    assert_eq!(
        host.frame(frame(4, json!({ "delta": { "projects": ["b"] } }))),
        vec![]
    );
    assert_eq!(host.requests("watch").len(), 2);

    host.reconnect();
    assert_eq!(host.requests("watch").len(), 3);
    assert_eq!(
        host.request("watch")["payload"],
        json!({ "watch": "projects", "scope": "" })
    );
}

#[test]
fn a_refused_token_halts_and_never_reconnects() {
    let mut host = Host::new();
    host.step(Event::Start);
    host.step(Event::Socket(SocketEvent::Opened(host.socket.unwrap())));
    assert_eq!(
        host.request("authenticate")["payload"],
        json!({ "token": "t0ken", "protocolVersion": 7, "deviceId": "dev-1", "clientInstanceId": "tab-1" })
    );

    let out = host.refuse("authenticate", "unauthorized");
    assert_eq!(
        out,
        vec![Output::Status(Status::Halted {
            code: "unauthorized".into(),
            message: "unauthorized".into()
        })]
    );

    let before = host.socket;
    host.now += 60.0;
    host.step(Event::Tick);
    assert_eq!(host.socket, before);
}

#[test]
fn frames_from_a_replaced_socket_are_ignored() {
    let mut host = Host::new();
    host.follow();
    let old = host.socket.unwrap();
    host.reconnect();

    let out = host.step(Event::Socket(SocketEvent::Text(
        old,
        batch(&[4]).to_string(),
    )));

    assert_eq!(out, vec![]);
    assert_eq!(host.client.cursor("s1").map(|c| c.last_seq), Some(3));
}

#[test]
fn a_late_refusal_for_a_replaced_subscribe_leaves_the_new_one_alone() {
    let mut host = Host::new();
    host.connect();
    let subscribe = || Command::Subscribe {
        session_id: "s1".into(),
        cursor: Cursor::default(),
    };
    host.command(subscribe());
    let first = host.request("subscribe")["id"].clone();
    host.command(Command::Unsubscribe {
        session_id: "s1".into(),
    });
    host.command(subscribe());

    let late = host.frame(
        json!({ "id": first, "ok": false, "err": { "code": "not_found", "message": "gone" } }),
    );
    assert_eq!(late, vec![]);

    let out = host.reply("subscribe", subscribed(3));
    assert!(matches!(&out[0], Output::Subscribed(r) if r.head_seq == 3));
    assert_eq!(seqs(&host.frame(batch(&[1, 2, 3, 4]))), vec![1, 2, 3, 4]);
}

fn watch_frame(n: u64, body: Value) -> Value {
    let mut f = json!({ "type": "watch", "watch": "projects", "scope": "", "revision": { "hostEpoch": "h1", "n": n } });
    f.as_object_mut()
        .unwrap()
        .extend(body.as_object().unwrap().clone());
    f
}

fn watch_projects() -> Command {
    Command::Watch {
        watch: WsWatchName::Projects,
        scope: String::new(),
    }
}

#[test]
fn a_late_refusal_for_a_replaced_watch_is_not_reported() {
    let mut host = Host::new();
    host.connect();
    host.command(watch_projects());
    let first = host.request("watch")["id"].clone();
    host.command(Command::Unwatch {
        watch: WsWatchName::Projects,
        scope: String::new(),
    });
    host.command(watch_projects());
    host.frame(watch_frame(
        1,
        json!({ "reset": true, "state": { "projects": [] } }),
    ));

    let late = host.frame(
        json!({ "id": first, "ok": false, "err": { "code": "rate_limited", "message": "busy" } }),
    );
    assert_eq!(late, vec![]);

    let delta = host.frame(watch_frame(2, json!({ "delta": { "projects": ["a"] } })));
    assert_eq!(
        delta,
        vec![Output::WatchDelta {
            watch: WsWatchName::Projects,
            scope: String::new(),
            delta: json!({ "projects": ["a"] })
        }]
    );
}

#[test]
fn a_watch_refused_for_a_passing_reason_is_asked_again_and_a_final_refusal_is_not() {
    let mut host = Host::new();
    host.connect();
    host.command(watch_projects());

    let out = host.refuse("watch", "rate_limited");
    assert!(
        matches!(&out[..], [Output::WatchRefused { error, .. }] if error.code == "rate_limited")
    );
    host.now += 0.5;
    host.step(Event::Tick);
    assert_eq!(
        host.requests("watch").len(),
        1,
        "not before the retry delay"
    );
    host.now += 0.5;
    host.step(Event::Tick);
    assert_eq!(host.requests("watch").len(), 2);

    host.refuse("watch", "unsupported");
    host.now += 60.0;
    host.step(Event::Tick);
    assert_eq!(host.requests("watch").len(), 2);
}

#[test]
fn a_watch_retry_gives_up_after_a_few_tries() {
    let mut host = Host::new();
    host.connect();
    host.command(watch_projects());

    for _ in 0..10 {
        host.refuse("watch", "rate_limited");
        host.now += 60.0;
        host.step(Event::Tick);
    }

    assert_eq!(
        host.requests("watch").len(),
        1 + crate::ws::WATCH_RETRIES as usize
    );
}

#[test]
fn an_authenticate_reply_after_the_handshake_budget_is_not_reported() {
    let mut host = Host::new();
    host.step(Event::Start);
    host.step(Event::Socket(SocketEvent::Opened(host.socket.unwrap())));
    host.now = crate::link::HANDSHAKE_S + 1.0;

    let out = host.reply("authenticate", json!({ "historyMode": "server" }));

    assert!(
        !out.iter().any(|o| matches!(o, Output::Authenticated(_))),
        "{out:?}"
    );
}

#[test]
fn a_turn_the_snapshot_completed_is_not_completed_again() {
    let mut host = Host::new();
    host.follow();
    host.frame(batch(&[5]));
    let done = |seq: u64| json!({ "sid": "s1", "seq": seq, "type": "prompt_complete", "payload": { "turnId": "t1" } });
    let snapshot = json!({
        "sessionId": "s1", "watermark": 5, "headSeq": 5, "logEpoch": "e1",
        "events": [chunk(1), chunk(2), chunk(3), done(4), chunk(5)]
    });
    host.reply("recover_session_snapshot", snapshot);

    assert_eq!(seqs(&host.frame(done(6))), Vec::<u64>::new());
}

#[test]
fn a_call_handle_already_in_use_is_refused_and_the_first_call_kept() {
    let mut host = Host::new();
    host.connect();
    let call = |text: &str| Command::Call {
        call: 5,
        kind: troupe_protocol::ws::WsRequestType::ListPersistedSessions,
        payload: json!({ "text": text }),
        ledgered: false,
    };
    host.command(call("first"));

    let out = host.command(call("second"));

    assert!(
        matches!(&out[..], [Output::Reply { call: 5, result: Err(e) }] if e.code == "duplicate_call"),
        "{out:?}"
    );
    assert_eq!(host.requests("list_persisted_sessions").len(), 1);
    assert_eq!(
        host.reply("list_persisted_sessions", json!({ "sessions": [] })),
        vec![Output::Reply {
            call: 5,
            result: Ok(Some(json!({ "sessions": [] })))
        }]
    );
}

#[test]
fn raw_call_forwards_type_replies_and_fails_with_closed_on_drop() {
    let mut host = Host::new();
    host.connect();
    host.command(Command::RawCall {
        call: 11,
        kind: "browser_tab_open".into(),
        payload: json!({ "projectId": "p1", "url": "http://127.0.0.1:8199/" }),
    });
    assert_eq!(
        host.request("browser_tab_open")["payload"],
        json!({ "projectId": "p1", "url": "http://127.0.0.1:8199/" })
    );
    assert_eq!(
        host.reply("browser_tab_open", json!({ "tabId": "tab-1" })),
        vec![Output::Reply {
            call: 11,
            result: Ok(Some(json!({ "tabId": "tab-1" })))
        }]
    );

    host.command(Command::RawCall {
        call: 12,
        kind: "browser_tab_navigate".into(),
        payload: json!({ "tabId": "tab-1", "url": "https://example.com/" }),
    });
    let out = host.reconnect();
    assert!(out.iter().any(|o| matches!(
        o,
        Output::Reply {
            call: 12,
            result: Err(e)
        } if e.code == "closed"
    )));
}

#[test]
fn raw_events_emit_standalone_and_inside_session_batches_without_breaking_cursor() {
    let mut host = Host::new();
    host.follow();

    let standalone = host.frame(json!({
        "type": "browser_stop_changed",
        "payload": { "sessionId": "s1", "stopped": true }
    }));
    assert_eq!(
        standalone,
        vec![Output::RawEvent {
            sid: None,
            seq: 0,
            kind: "browser_stop_changed".into(),
            payload: json!({ "sessionId": "s1", "stopped": true })
        }]
    );

    let mixed = host.frame(json!({
        "type": "events",
        "events": [
            { "sid": "s1", "seq": 4, "type": "browser_agent_tab", "payload": { "action": "opened", "tabId": "t1" } },
            chunk(5)
        ]
    }));
    assert!(mixed.iter().any(|o| matches!(
        o,
        Output::RawEvent { sid: Some(sid), seq: 4, kind, .. }
            if sid == "s1" && kind == "browser_agent_tab"
    )));
    assert_eq!(seqs(&mixed), vec![5]);
    assert_eq!(host.client.cursor("s1").map(|c| c.last_seq), Some(5));
}
