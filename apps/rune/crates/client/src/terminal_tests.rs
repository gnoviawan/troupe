//! The `/terminal/ws` machine with no network: the test plays the host.

use rune_core::machine::Attempt;
use serde_json::{Value, json};

use crate::machine::{Effect, Event, Machine, SocketEvent};
use crate::reconnect_tests::config;
use crate::terminal::{Command, Output, TerminalClient};

struct Host {
    client: TerminalClient,
    now: f64,
    socket: Option<Attempt>,
    sent: Vec<Value>,
    closed: Vec<Attempt>,
}

impl Host {
    fn new() -> Self {
        Self {
            client: TerminalClient::new(config()),
            now: 0.0,
            socket: None,
            sent: Vec::new(),
            closed: Vec::new(),
        }
    }

    fn step(&mut self, event: Event<Command>) -> Vec<Output> {
        let mut out = Vec::new();
        for effect in self.client.step(event, self.now) {
            match effect {
                Effect::Open { socket, url } => {
                    assert_eq!(url, "ws://127.0.0.1:8190/terminal/ws");
                    self.socket = Some(socket);
                }
                Effect::Send { text, .. } => self.sent.push(serde_json::from_str(&text).unwrap()),
                Effect::Close { socket } => self.closed.push(socket),
                Effect::WakeAt(_) => {}
                Effect::Output(o) => out.push(o),
            }
        }
        out
    }

    fn frame(&mut self, frame: Value) -> Vec<Output> {
        let socket = self.socket.unwrap();
        self.step(Event::Socket(SocketEvent::Text(socket, frame.to_string())))
    }

    fn request(&self, kind: &str) -> Value {
        self.sent
            .iter()
            .rev()
            .find(|r| r["type"] == kind)
            .cloned()
            .unwrap_or_else(|| panic!("no {kind} sent"))
    }

    fn connect(&mut self) {
        self.step(Event::Start);
        self.step(Event::Socket(SocketEvent::Opened(self.socket.unwrap())));
        let id = self.request("authenticate")["id"].clone();
        self.frame(json!({ "id": id, "success": true, "data": {} }));
    }

    fn attach(&mut self) -> Value {
        self.step(Event::Command(Command::Attach {
            terminal_id: "term-1".into(),
            claim: "claim-1".into(),
            log_epoch: None,
            last_seq: 0,
        }));
        self.request("attach")["id"].clone()
    }
}

fn data(seq: u64) -> Value {
    json!({ "type": "data", "terminalId": "term-1", "seq": seq, "data": [104, 105] })
}

#[test]
fn a_late_refusal_for_a_replaced_attach_leaves_the_new_one_alone() {
    let mut host = Host::new();
    host.connect();
    let first = host.attach();
    host.step(Event::Command(Command::Detach {
        terminal_id: "term-1".into(),
    }));
    host.attach();

    let late =
        host.frame(json!({ "id": first, "success": false, "error": "no", "code": "FORBIDDEN" }));
    assert_eq!(late, vec![]);

    assert_eq!(
        host.frame(data(1)),
        vec![Output::Data {
            terminal_id: "term-1".into(),
            seq: 1,
            bytes: b"hi".to_vec()
        }]
    );
}

#[test]
fn a_refusal_for_the_current_attach_loses_the_terminal() {
    let mut host = Host::new();
    host.connect();
    let id = host.attach();

    let out = host.frame(json!({ "id": id, "success": false, "error": "no", "code": "FORBIDDEN" }));

    assert!(matches!(&out[..], [Output::Lost { terminal_id, .. }] if terminal_id == "term-1"));
}

#[test]
fn an_idle_terminal_socket_is_not_replaced() {
    let mut host = Host::new();
    host.connect();
    host.attach();

    let mut out = Vec::new();
    for _ in 0..120 {
        host.now += 1.0;
        out.extend(host.step(Event::Tick));
    }

    assert_eq!(out, vec![]);
    assert_eq!(host.closed, vec![]);
}

#[test]
fn a_call_handle_already_in_use_is_refused() {
    let mut host = Host::new();
    host.connect();
    let call = || {
        Event::Command(Command::Call {
            call: 2,
            kind: troupe_protocol::terminal::WebTerminalRequestType::GetCwd,
            payload: json!({ "terminalId": "term-1" }),
            ledgered: false,
        })
    };
    host.step(call());

    let out = host.step(call());

    assert!(
        matches!(&out[..], [Output::Reply { call: 2, result: Err(e) }] if e.code == "DUPLICATE_CALL"),
        "{out:?}"
    );
}
