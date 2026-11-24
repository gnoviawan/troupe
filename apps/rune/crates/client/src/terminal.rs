//! The `/terminal/ws` machine: attach to terminals with their claims, replay
//! the output a client has not seen, and keep each terminal's byte stream
//! contiguous across reconnects.
//!
//! A terminal is attached with the claim its `spawn` (or `rotate_claim`,
//! `list_preserved`) issued and the cursor `(logEpoch, lastSeq)` of the bytes
//! already shown. Every authenticated socket re-attaches every terminal from
//! its cursor; the host's `replay` frame carries only the unseen chunks, or
//! `reset` when the cursor belongs to another log.

use std::collections::BTreeMap;

use rune_core::machine::{Attempt, Effects};
use serde_json::{Value, json};
use troupe_protocol::terminal::{
    TerminalAttachResult, TerminalStateSnapshot, WebTerminalErrorCode, WebTerminalEventPayload,
    WebTerminalFrame, WebTerminalReply, WebTerminalRequest, WebTerminalRequestType,
};
use troupe_protocol::version::WS_PROTOCOL_VERSION;
use troupe_protocol::ws::AcpAuthenticatePayload;

use crate::ledger::{Carrier, Ledger, is_in_progress_receipt};
use crate::link::{LinkEffect, LinkEvent, LinkMachine, Liveness, Status};
use crate::machine::{CallId, Config, Effect, Event, Machine, SocketEvent};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Attach with `claim`, showing nothing yet (`last_seq` 0) or resuming a
    /// stream the app already shows.
    Attach {
        terminal_id: String,
        claim: String,
        log_epoch: Option<String>,
        last_seq: u64,
    },
    Detach {
        terminal_id: String,
    },
    /// Send one request. Ledgered kinds: `spawn`, `write`, `kill`,
    /// `rotate_claim`, `revoke_claim`.
    Call {
        call: CallId,
        kind: WebTerminalRequestType,
        payload: Value,
        ledgered: bool,
    },
}

/// A failed `/terminal/ws` request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Status(Status),
    /// The attach landed; `replayed` holds the chunks the app had not shown.
    /// With `reset` the app clears what it shows first.
    Replay {
        terminal_id: String,
        reset: bool,
        gap: bool,
        bytes: Vec<u8>,
        snapshot: TerminalStateSnapshot,
    },
    Attached(TerminalAttachResult),
    /// Live output, contiguous with everything before it.
    Data {
        terminal_id: String,
        seq: u64,
        bytes: Vec<u8>,
    },
    Event(WebTerminalEventPayload),
    /// The host refused the claim (a restarted host, a revoked claim). The
    /// machine forgets the terminal.
    Lost {
        terminal_id: String,
        error: TermError,
    },
    Reply {
        call: CallId,
        result: Result<Value, TermError>,
    },
}

type Fx = Effects<Effect<Output>>;

#[derive(Clone, Debug, PartialEq)]
enum Sync {
    Offline,
    Attaching,
    Live,
}

#[derive(Clone, Debug, PartialEq)]
struct Terminal {
    claim: String,
    log_epoch: Option<String>,
    last_seq: u64,
    sync: Sync,
}

#[derive(Clone, Debug, PartialEq)]
enum Purpose {
    Authenticate,
    Attach(String),
    Detach,
    Call(CallId),
}

pub struct TerminalClient {
    config: Config,
    link: LinkMachine,
    next_request: u64,
    inflight: BTreeMap<String, Purpose>,
    terminals: BTreeMap<String, Terminal>,
    ledger: Ledger<WebTerminalRequestType>,
}

impl Machine for TerminalClient {
    type Command = Command;
    type Output = Output;
    const PATH: &'static str = "/terminal/ws";

    fn step(&mut self, event: Event<Command>, now: f64) -> Fx {
        let mut fx = Effects::none();
        match event {
            Event::Start => self.link_step(LinkEvent::Start, now, &mut fx),
            Event::Stop => self.link_step(LinkEvent::Stop, now, &mut fx),
            Event::Tick => {
                self.link_step(LinkEvent::Tick, now, &mut fx);
                self.flush_calls(now, &mut fx);
            }
            Event::Socket(SocketEvent::Opened(s)) => {
                self.link_step(LinkEvent::Opened(s), now, &mut fx)
            }
            Event::Socket(SocketEvent::Closed { socket, .. }) => {
                self.link_step(LinkEvent::Closed(socket), now, &mut fx)
            }
            Event::Socket(SocketEvent::Text(s, text)) => {
                if self.link.socket() == Some(s) {
                    self.link_step(LinkEvent::Heard(s), now, &mut fx);
                    self.frame(&text, now, &mut fx);
                }
            }
            Event::Command(command) => self.command(command, now, &mut fx),
        }
        fx
    }
}

impl TerminalClient {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            link: LinkMachine::new(config.seed ^ 0x7465_726d, Liveness::CloseOnly),
            ledger: Ledger::new(&config, Carrier::Terminal),
            config,
            next_request: 0,
            inflight: BTreeMap::new(),
            terminals: BTreeMap::new(),
        }
    }

    /// The claim held for a terminal (it changes on `rotate_claim`).
    #[must_use]
    pub fn claim(&self, terminal_id: &str) -> Option<&str> {
        self.terminals.get(terminal_id).map(|t| t.claim.as_str())
    }

    fn link_step(&mut self, event: LinkEvent, now: f64, fx: &mut Fx) {
        for effect in self.link.step(event, now) {
            match effect {
                LinkEffect::Open(socket) => fx.push(Effect::Open {
                    socket,
                    url: self.config.socket_url(Self::PATH),
                }),
                LinkEffect::Close(socket) => fx.push(Effect::Close { socket }),
                LinkEffect::Authenticate(socket) => {
                    let payload = AcpAuthenticatePayload {
                        token: self.config.token.clone(),
                        protocol_version: WS_PROTOCOL_VERSION,
                        device_id: self.config.device_id.clone(),
                        client_instance_id: self.config.client_instance_id.clone(),
                    };
                    let payload = serde_json::to_value(payload).unwrap_or(Value::Null);
                    self.send(
                        socket,
                        Purpose::Authenticate,
                        WebTerminalRequestType::Authenticate,
                        payload,
                        None,
                        fx,
                    );
                }
                // `Liveness::CloseOnly` never probes: the protocol has no ping.
                LinkEffect::Ping(_) => {}
                LinkEffect::WakeAt(due) => fx.push(Effect::WakeAt(due)),
                LinkEffect::Up(socket) => {
                    let ids: Vec<String> = self.terminals.keys().cloned().collect();
                    for id in ids {
                        self.attach(socket, &id, fx);
                    }
                    self.flush_calls(now, fx);
                }
                LinkEffect::Down => {
                    self.inflight.clear();
                    for t in self.terminals.values_mut() {
                        t.sync = Sync::Offline;
                    }
                    for (call, c) in self.ledger.down() {
                        let message = format!("the socket closed before {:?} was answered", c.kind);
                        let error = TermError {
                            code: "NETWORK_ERROR".into(),
                            message,
                        };
                        fx.push(Effect::Output(Output::Reply {
                            call,
                            result: Err(error),
                        }));
                    }
                }
                LinkEffect::Status(status) => fx.push(Effect::Output(Output::Status(status))),
            }
        }
    }

    fn send(
        &mut self,
        socket: Attempt,
        purpose: Purpose,
        kind: WebTerminalRequestType,
        payload: Value,
        command_id: Option<String>,
        fx: &mut Fx,
    ) -> String {
        self.next_request += 1;
        let id = format!("t{}", self.next_request);
        let request = WebTerminalRequest {
            id: id.clone(),
            kind,
            payload,
            command_id,
        };
        match serde_json::to_string(&request) {
            Ok(text) => {
                self.inflight.insert(id.clone(), purpose);
                fx.push(Effect::Send { socket, text });
            }
            Err(e) => log::error!("[client] could not encode terminal {kind:?}: {e}"),
        }
        id
    }

    fn command(&mut self, command: Command, now: f64, fx: &mut Fx) {
        match command {
            Command::Attach {
                terminal_id,
                claim,
                log_epoch,
                last_seq,
            } => {
                let terminal = Terminal {
                    claim,
                    log_epoch,
                    last_seq,
                    sync: Sync::Offline,
                };
                self.terminals.insert(terminal_id.clone(), terminal);
                if let Some(socket) = self.link.live() {
                    self.attach(socket, &terminal_id, fx);
                }
            }
            Command::Detach { terminal_id } => {
                self.forget_attach(&terminal_id);
                if self.terminals.remove(&terminal_id).is_some()
                    && let Some(socket) = self.link.live()
                {
                    let payload = json!({ "terminalId": terminal_id });
                    self.send(
                        socket,
                        Purpose::Detach,
                        WebTerminalRequestType::Detach,
                        payload,
                        None,
                        fx,
                    );
                }
            }
            Command::Call {
                call,
                kind,
                payload,
                ledgered,
            } => {
                if !self.ledger.add(call, kind, payload, ledgered) {
                    log::error!("[client] call handle {call} is already in use");
                    let error = TermError {
                        code: "DUPLICATE_CALL".into(),
                        message: format!("call handle {call} is already in use"),
                    };
                    let result = Err(error);
                    return fx.push(Effect::Output(Output::Reply { call, result }));
                }
                self.flush_calls(now, fx);
            }
        }
    }

    fn flush_calls(&mut self, now: f64, fx: &mut Fx) {
        let Some(socket) = self.link.live() else {
            return;
        };
        for call in self.ledger.due(now) {
            let Some(c) = self.ledger.get(call) else {
                continue;
            };
            let (kind, payload, command_id) = (c.kind, c.payload.clone(), c.command_id.clone());
            let request = self.send(socket, Purpose::Call(call), kind, payload, command_id, fx);
            self.ledger.mark_sent(call, request);
        }
    }

    fn attach(&mut self, socket: Attempt, terminal_id: &str, fx: &mut Fx) {
        let Some(t) = self.terminals.get_mut(terminal_id) else {
            return;
        };
        t.sync = Sync::Attaching;
        let mut payload =
            json!({ "terminalId": terminal_id, "claim": t.claim, "lastSeq": t.last_seq });
        if t.last_seq > 0
            && let Some(epoch) = &t.log_epoch
        {
            payload["logEpoch"] = json!(epoch);
        }
        self.forget_attach(terminal_id);
        let purpose = Purpose::Attach(terminal_id.to_owned());
        self.send(
            socket,
            purpose,
            WebTerminalRequestType::Attach,
            payload,
            None,
            fx,
        );
    }

    /// A newer attach or a detach replaces any attach still in flight, so its
    /// late reply is ignored.
    fn forget_attach(&mut self, terminal_id: &str) {
        self.inflight
            .retain(|_, p| !matches!(p, Purpose::Attach(id) if id == terminal_id));
    }

    fn frame(&mut self, text: &str, now: f64, fx: &mut Fx) {
        let frame = match serde_json::from_str::<WebTerminalFrame>(text) {
            Ok(frame) => frame,
            Err(e) => {
                log::warn!(
                    "[client] unreadable /terminal/ws frame ({} bytes): {e}",
                    text.len()
                );
                return;
            }
        };
        match frame {
            WebTerminalFrame::Reply(reply) => self.reply(reply, now, fx),
            WebTerminalFrame::Replay(replay) => {
                let Some(t) = self.terminals.get_mut(&replay.terminal_id) else {
                    return;
                };
                let floor = if replay.reset { 0 } else { t.last_seq };
                let bytes: Vec<u8> = replay
                    .chunks
                    .iter()
                    .filter(|c| c.seq > floor)
                    .flat_map(|c| c.data.iter().copied())
                    .collect();
                t.log_epoch = Some(replay.log_epoch);
                t.last_seq = replay.latest_seq;
                if replay.gap {
                    log::info!("[client] terminal {} replay has a gap", replay.terminal_id);
                }
                fx.push(Effect::Output(Output::Replay {
                    terminal_id: replay.terminal_id,
                    reset: replay.reset,
                    gap: replay.gap,
                    bytes,
                    snapshot: replay.snapshot,
                }));
            }
            WebTerminalFrame::Data(data) => {
                let Some(t) = self.terminals.get_mut(&data.terminal_id) else {
                    return;
                };
                if data.seq <= t.last_seq {
                    return;
                }
                if data.seq != t.last_seq + 1 {
                    log::info!(
                        "[client] terminal {} seq {} after {}; re-attaching",
                        data.terminal_id,
                        data.seq,
                        t.last_seq
                    );
                    if let Some(socket) = self.link.live() {
                        self.attach(socket, &data.terminal_id, fx);
                    }
                    return;
                }
                t.last_seq = data.seq;
                fx.push(Effect::Output(Output::Data {
                    terminal_id: data.terminal_id,
                    seq: data.seq,
                    bytes: data.data,
                }));
            }
            WebTerminalFrame::Gap(gap) => {
                log::info!(
                    "[client] terminal {} lost output after {}; re-attaching",
                    gap.terminal_id,
                    gap.last_seq
                );
                if let Some(socket) = self.link.live()
                    && self.terminals.contains_key(&gap.terminal_id)
                {
                    self.attach(socket, &gap.terminal_id, fx);
                }
            }
            WebTerminalFrame::Event(event) => fx.push(Effect::Output(Output::Event(event.payload))),
        }
    }

    fn reply(&mut self, reply: WebTerminalReply, now: f64, fx: &mut Fx) {
        let (id, result) = match reply {
            WebTerminalReply::Ok(ok) => (ok.id, Ok(ok.data)),
            WebTerminalReply::Err(err) => (
                err.id,
                Err(TermError {
                    code: err.code,
                    message: err.error,
                }),
            ),
        };
        let Some(purpose) = self.inflight.remove(&id) else {
            return;
        };
        let Some(socket) = self.link.socket() else {
            return;
        };
        match purpose {
            Purpose::Authenticate => match result {
                Ok(_) => self.link_step(LinkEvent::AuthOk(socket), now, fx),
                Err(e) => {
                    let fatal = [
                        WebTerminalErrorCode::Unauthorized,
                        WebTerminalErrorCode::ProtocolVersionMismatch,
                        WebTerminalErrorCode::InvalidClientIdentity,
                    ]
                    .iter()
                    .any(|c| c.as_str() == e.code);
                    let refused = LinkEvent::AuthRefused {
                        socket,
                        code: e.code,
                        message: e.message,
                        fatal,
                    };
                    self.link_step(refused, now, fx);
                }
            },
            Purpose::Detach => {}
            Purpose::Attach(terminal_id) => match result {
                Ok(data) => {
                    let Some(t) = self.terminals.get_mut(&terminal_id) else {
                        return;
                    };
                    t.sync = Sync::Live;
                    match serde_json::from_value::<TerminalAttachResult>(data) {
                        Ok(attached) => fx.push(Effect::Output(Output::Attached(attached))),
                        Err(e) => {
                            log::warn!("[client] unreadable attach reply for {terminal_id}: {e}")
                        }
                    }
                }
                Err(error) => {
                    log::warn!("[client] attach {terminal_id} refused: {}", error.code);
                    self.terminals.remove(&terminal_id);
                    fx.push(Effect::Output(Output::Lost { terminal_id, error }));
                }
            },
            Purpose::Call(call) => {
                let ledgered = self
                    .ledger
                    .get(call)
                    .is_some_and(|c| c.command_id.is_some());
                if ledgered && matches!(&result, Ok(data) if is_in_progress_receipt(Some(data))) {
                    let retry_at = self.ledger.mark_in_progress(call, now);
                    fx.push(Effect::WakeAt(retry_at));
                    return;
                }
                let Some(c) = self.ledger.take(call) else {
                    return;
                };
                if c.kind == WebTerminalRequestType::RotateClaim
                    && let Ok(data) = &result
                {
                    self.adopt_claim(&c.payload, data);
                }
                fx.push(Effect::Output(Output::Reply { call, result }));
            }
        }
    }

    /// A rotated claim replaces the one presented on the next re-attach.
    fn adopt_claim(&mut self, request: &Value, data: &Value) {
        let terminal_id = request.get("terminalId").and_then(Value::as_str);
        let claim = data.get("claim").and_then(Value::as_str);
        if let (Some(terminal_id), Some(claim)) = (terminal_id, claim)
            && let Some(t) = self.terminals.get_mut(terminal_id)
        {
            t.claim = claim.to_owned();
        }
    }
}
