//! The `/ws` machine: one multiplexed socket for requests, session log events
//! and watches.
//!
//! Sessions, watches and queued requests are what the app wants; they outlive
//! any one socket. On every authenticated socket the machine resubscribes each
//! session from its cursor, rewatches each watch and resends each ledgered
//! request. Session events are judged against the cursor so the app sees each
//! seq once and in order; a hole installs a snapshot instead of waiting.

use std::collections::{BTreeMap, BTreeSet};

use rune_core::machine::{Attempt, Effects};
use serde_json::{Value, json};
use troupe_protocol::version::WS_PROTOCOL_VERSION;
use troupe_protocol::ws::{
    AcpAuthenticatePayload, AcpAuthenticateReply, HistoryMode, SessionSnapshotEvent,
    SessionSubscribeReply, WsError, WsErrorCode, WsEvent, WsEventType, WsReply, WsRequest,
    WsRequestType, WsServerFrame, WsWatchName, WsWatchRevision,
};

use crate::cursor::{Cursor, Held, HeldItem, Verdict};
use crate::ledger::{Carrier, Ledger, is_in_progress_receipt};
use crate::link::{LinkEffect, LinkEvent, LinkMachine, Liveness, Status};
use crate::machine::{CallId, Config, Effect, Event, Machine, SocketEvent};

#[path = "ws_watch.rs"]
mod watch;
pub use watch::{WATCH_RETRIES, WATCH_RETRY_S};

/// A request from the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Follow a session from `cursor`; `Cursor::default()` replays the whole log.
    Subscribe {
        session_id: String,
        cursor: Cursor,
    },
    /// Stop judging and forwarding a session's events.
    Unsubscribe {
        session_id: String,
    },
    Watch {
        watch: WsWatchName,
        scope: String,
    },
    Unwatch {
        watch: WsWatchName,
        scope: String,
    },
    /// Send one request. A `ledgered` one carries a `commandId` and survives a
    /// reconnect; use it for every mutating request the host ledgers. A ledgered
    /// `send_prompt` with a `turnId` carries that as its `commandId`, as the host
    /// requires.
    Call {
        call: CallId,
        kind: WsRequestType,
        payload: Value,
        ledgered: bool,
    },
}

/// What the machine hands the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Status(Status),
    Authenticated(AcpAuthenticateReply),
    /// A session event in seq order, each seq once, or an agent-level event
    /// (`sid` null, `seq` 0).
    Event(WsEvent),
    /// A subscribe landed; replayed events follow as [`Output::Event`].
    Subscribed(SessionSubscribeReply),
    /// The stream had a hole: replace everything held for the session.
    Snapshot(SessionSnapshotEvent),
    /// A live-only host lost events it cannot replay: the session restarts
    /// from live events with what it already showed.
    Degraded {
        session_id: String,
    },
    /// The host refused the session; the machine stopped following it.
    SessionLost {
        session_id: String,
        error: WsError,
    },
    WatchState {
        watch: WsWatchName,
        scope: String,
        state: Value,
    },
    WatchDelta {
        watch: WsWatchName,
        scope: String,
        delta: Value,
    },
    WatchRefused {
        watch: WsWatchName,
        scope: String,
        error: WsError,
    },
    Reply {
        call: CallId,
        result: Result<Option<Value>, WsError>,
    },
    /// A frame this machine does not own (canvas calls).
    Unhandled(Box<WsServerFrame>),
}

type Fx = Effects<Effect<Output>>;
type WatchKey = (WsWatchName, String);

/// Where one session's subscription stands on the current socket.
#[derive(Clone, Debug, PartialEq)]
enum Sync {
    /// No live socket; the next one resubscribes from the cursor.
    Offline,
    Subscribing(Held),
    /// A `recover_session_snapshot` is in flight.
    Recovering(Held),
    Live,
}

#[derive(Clone, Debug, PartialEq)]
struct Session {
    cursor: Cursor,
    sync: Sync,
    /// Turn ids whose `prompt_complete` was applied (the host may resend it).
    turns: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum WatchSync {
    Offline,
    /// A request is in flight after `tries` passing refusals.
    Requesting {
        tries: u32,
    },
    Live(WsWatchRevision),
    /// Refused for a passing reason; ask again at `due`.
    Retrying {
        tries: u32,
        due: f64,
    },
    /// The host answered `unsupported` or `not_found`; asking again is pointless.
    Refused,
}

/// Why a request id is in flight on the current socket.
#[derive(Clone, Debug, PartialEq)]
enum Purpose {
    Authenticate,
    Ping,
    Subscribe(String),
    Recover(String),
    Watch(WatchKey),
    Unwatch,
    Call(CallId),
}

pub struct WsClient {
    config: Config,
    link: LinkMachine,
    next_request: u64,
    history: Option<HistoryMode>,
    inflight: BTreeMap<String, Purpose>,
    sessions: BTreeMap<String, Session>,
    watches: BTreeMap<WatchKey, WatchSync>,
    ledger: Ledger<WsRequestType>,
}

impl Machine for WsClient {
    type Command = Command;
    type Output = Output;
    const PATH: &'static str = "/ws";

    fn step(&mut self, event: Event<Command>, now: f64) -> Fx {
        let mut fx = Effects::none();
        match event {
            Event::Start => self.link_step(LinkEvent::Start, now, &mut fx),
            Event::Stop => self.link_step(LinkEvent::Stop, now, &mut fx),
            Event::Tick => {
                self.link_step(LinkEvent::Tick, now, &mut fx);
                self.flush_calls(now, &mut fx);
                self.retry_watches(now, &mut fx);
            }
            Event::Socket(SocketEvent::Opened(s)) => {
                self.link_step(LinkEvent::Opened(s), now, &mut fx)
            }
            Event::Socket(SocketEvent::Closed { socket, code }) => {
                if self.link.socket() == Some(socket) {
                    log::info!("[client] /ws closed (code {code:?})");
                }
                self.link_step(LinkEvent::Closed(socket), now, &mut fx);
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

impl WsClient {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            link: LinkMachine::new(config.seed, Liveness::Heartbeat),
            ledger: Ledger::new(&config, Carrier::Ws),
            config,
            next_request: 0,
            history: None,
            inflight: BTreeMap::new(),
            sessions: BTreeMap::new(),
            watches: BTreeMap::new(),
        }
    }

    /// The session's cursor, for a caller that persists it.
    #[must_use]
    pub fn cursor(&self, session_id: &str) -> Option<&Cursor> {
        self.sessions.get(session_id).map(|s| &s.cursor)
    }

    #[must_use]
    pub fn link(&self) -> &LinkMachine {
        &self.link
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
                        host_local_token: None,
                    };
                    let payload = serde_json::to_value(payload).unwrap_or(Value::Null);
                    self.send(
                        socket,
                        Purpose::Authenticate,
                        WsRequestType::Authenticate,
                        payload,
                        None,
                        fx,
                    );
                }
                LinkEffect::Ping(socket) => {
                    self.send(
                        socket,
                        Purpose::Ping,
                        WsRequestType::Ping,
                        json!({}),
                        None,
                        fx,
                    );
                }
                LinkEffect::WakeAt(due) => fx.push(Effect::WakeAt(due)),
                LinkEffect::Up(socket) => self.up(socket, now, fx),
                LinkEffect::Down => self.down(fx),
                LinkEffect::Status(status) => fx.push(Effect::Output(Output::Status(status))),
            }
        }
    }

    fn send(
        &mut self,
        socket: Attempt,
        purpose: Purpose,
        kind: WsRequestType,
        payload: Value,
        command_id: Option<String>,
        fx: &mut Fx,
    ) -> String {
        self.next_request += 1;
        let id = format!("r{}", self.next_request);
        let request = WsRequest {
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
            Err(e) => log::error!("[client] could not encode {kind:?}: {e}"),
        }
        id
    }

    fn command(&mut self, command: Command, now: f64, fx: &mut Fx) {
        match command {
            Command::Subscribe { session_id, cursor } => {
                self.sessions.insert(
                    session_id.clone(),
                    Session {
                        cursor,
                        sync: Sync::Offline,
                        turns: BTreeSet::new(),
                    },
                );
                if let Some(socket) = self.link.live() {
                    self.subscribe(socket, &session_id, fx);
                }
            }
            Command::Unsubscribe { session_id } => {
                self.forget_session(&session_id);
                self.sessions.remove(&session_id);
            }
            Command::Watch { watch, scope } => {
                let key = (watch, scope);
                self.watches.insert(key.clone(), WatchSync::Offline);
                if let Some(socket) = self.link.live() {
                    self.request_watch(socket, key, 0, fx);
                }
            }
            Command::Unwatch { watch, scope } => {
                let key = (watch, scope.clone());
                self.forget_watch(&key);
                self.watches.remove(&key);
                if let Some(socket) = self.link.live() {
                    let payload = json!({ "watch": watch, "scope": scope });
                    self.send(
                        socket,
                        Purpose::Unwatch,
                        WsRequestType::Unwatch,
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
                let added = match prompt_command_id(kind, &payload, ledgered) {
                    Some(turn_id) => self.ledger.add_as(call, kind, payload, turn_id),
                    None => self.ledger.add(call, kind, payload, ledgered),
                };
                if !added {
                    log::error!("[client] call handle {call} is already in use");
                    let error = WsError {
                        code: "duplicate_call".into(),
                        message: format!("call handle {call} is already in use"),
                        details: None,
                    };
                    let result = Err(error);
                    return fx.push(Effect::Output(Output::Reply { call, result }));
                }
                self.flush_calls(now, fx);
            }
        }
    }

    /// Put every due request on the live socket.
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

    fn up(&mut self, socket: Attempt, now: f64, fx: &mut Fx) {
        let sessions: Vec<String> = self.sessions.keys().cloned().collect();
        for session_id in sessions {
            self.subscribe(socket, &session_id, fx);
        }
        let watches: Vec<WatchKey> = self
            .watches
            .iter()
            .filter(|(_, sync)| **sync != WatchSync::Refused)
            .map(|(key, _)| key.clone())
            .collect();
        for key in watches {
            self.request_watch(socket, key, 0, fx);
        }
        self.flush_calls(now, fx);
    }

    /// A newer subscribe or an unsubscribe replaces any subscribe or recover
    /// still in flight for the session, so its late reply is ignored.
    fn forget_session(&mut self, session_id: &str) {
        self.inflight.retain(
            |_, p| !matches!(p, Purpose::Subscribe(id) | Purpose::Recover(id) if id == session_id),
        );
    }

    fn down(&mut self, fx: &mut Fx) {
        self.inflight.clear();
        for session in self.sessions.values_mut() {
            session.sync = Sync::Offline;
        }
        for sync in self.watches.values_mut() {
            if *sync != WatchSync::Refused {
                *sync = WatchSync::Offline;
            }
        }
        for (call, c) in self.ledger.down() {
            let error = WsError {
                code: "closed".into(),
                message: format!("the socket closed before {:?} was answered", c.kind),
                details: None,
            };
            fx.push(Effect::Output(Output::Reply {
                call,
                result: Err(error),
            }));
        }
    }

    fn frame(&mut self, text: &str, now: f64, fx: &mut Fx) {
        let frame = match serde_json::from_str::<WsServerFrame>(text) {
            Ok(frame) => frame,
            Err(e) => {
                log::warn!("[client] unreadable /ws frame ({} bytes): {e}", text.len());
                return;
            }
        };
        match frame {
            WsServerFrame::Reply(reply) => self.reply(reply, now, fx),
            WsServerFrame::Batch(batch) => {
                for event in batch.events {
                    self.event(event, fx);
                }
            }
            WsServerFrame::Event(event) => self.event(event, fx),
            WsServerFrame::Gap(gap) => self.session_item(&gap.sid, HeldItem::Gap, fx),
            WsServerFrame::Heartbeat(_) => {}
            WsServerFrame::Watch(frame) => self.watch_frame(frame, fx),
            other @ (WsServerFrame::CanvasCall(_) | WsServerFrame::CanvasLeaseLost(_)) => {
                fx.push(Effect::Output(Output::Unhandled(Box::new(other))));
            }
        }
    }

    fn reply(&mut self, reply: WsReply, now: f64, fx: &mut Fx) {
        let (id, result) = match reply {
            WsReply::Ok(ok) => (ok.id, Ok(ok.payload)),
            WsReply::Err(err) => (err.id, Err(err.err)),
        };
        let Some(purpose) = self.inflight.remove(&id) else {
            return;
        };
        let Some(socket) = self.link.socket() else {
            return;
        };
        match purpose {
            Purpose::Authenticate => match result {
                Ok(payload) => {
                    let reply = payload
                        .and_then(|p| serde_json::from_value::<AcpAuthenticateReply>(p).ok())
                        .unwrap_or(AcpAuthenticateReply {
                            history_mode: None,
                            runtime_policy: None,
                            protocol_version: None,
                        });
                    // The link may refuse a reply past the handshake budget;
                    // report it only once the socket went live. `Up`
                    // resubscribes with the negotiated history mode.
                    let before = std::mem::replace(&mut self.history, reply.history_mode);
                    let mut up = Effects::none();
                    self.link_step(LinkEvent::AuthOk(socket), now, &mut up);
                    if self.link.live() == Some(socket) {
                        fx.push(Effect::Output(Output::Authenticated(reply)));
                    } else {
                        self.history = before;
                    }
                    fx.extend(up);
                }
                Err(e) => {
                    let fatal = [
                        WsErrorCode::Unauthorized,
                        WsErrorCode::ProtocolVersionMismatch,
                        WsErrorCode::InvalidClientIdentity,
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
            Purpose::Ping | Purpose::Unwatch => {}
            Purpose::Subscribe(session_id) => self.subscribed(socket, session_id, result, fx),
            Purpose::Recover(session_id) => self.recovered(session_id, result, fx),
            Purpose::Watch(key) => {
                if let Err(error) = result {
                    self.watch_refused(key, error, now, fx);
                }
            }
            Purpose::Call(call) => {
                let still_running =
                    self.ledger.get(call).is_some_and(|c| {
                        c.command_id.is_some() && c.kind != WsRequestType::SendPrompt
                    }) && matches!(&result, Ok(p) if is_in_progress_receipt(p.as_ref()));
                if still_running {
                    let retry_at = self.ledger.mark_in_progress(call, now);
                    fx.push(Effect::WakeAt(retry_at));
                } else if self.ledger.take(call).is_some() {
                    fx.push(Effect::Output(Output::Reply { call, result }));
                }
            }
        }
    }

    fn subscribe(&mut self, socket: Attempt, session_id: &str, fx: &mut Fx) {
        self.forget_session(session_id);
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        let cursor = &session.cursor;
        if cursor.last_seq > 0 && cursor.log_epoch.is_none() {
            // A cursor without the epoch it was issued under cannot be checked.
            session.cursor = Cursor::default();
            self.recover(socket, session_id, Held::default(), fx);
            return;
        }
        let mut payload = json!({ "sessionId": session_id, "lastSeq": cursor.last_seq });
        if let Some(epoch) = &cursor.log_epoch {
            payload["logEpoch"] = json!(epoch);
        }
        session.sync = Sync::Subscribing(Held::default());
        self.send(
            socket,
            Purpose::Subscribe(session_id.to_owned()),
            WsRequestType::Subscribe,
            payload,
            None,
            fx,
        );
    }

    /// Install a snapshot, or on a live-only host restart from live events.
    fn recover(&mut self, socket: Attempt, session_id: &str, held: Held, fx: &mut Fx) {
        self.forget_session(session_id);
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        session.turns.clear();
        if self.history == Some(HistoryMode::LiveOnly) {
            session.cursor = Cursor::default();
            session.sync = Sync::Subscribing(held);
            let payload = json!({ "sessionId": session_id });
            self.send(
                socket,
                Purpose::Subscribe(session_id.to_owned()),
                WsRequestType::Subscribe,
                payload,
                None,
                fx,
            );
            fx.push(Effect::Output(Output::Degraded {
                session_id: session_id.to_owned(),
            }));
            return;
        }
        session.sync = Sync::Recovering(held);
        let payload = json!({ "sessionId": session_id });
        let purpose = Purpose::Recover(session_id.to_owned());
        self.send(
            socket,
            purpose,
            WsRequestType::RecoverSessionSnapshot,
            payload,
            None,
            fx,
        );
    }

    fn subscribed(
        &mut self,
        socket: Attempt,
        session_id: String,
        result: Result<Option<Value>, WsError>,
        fx: &mut Fx,
    ) {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return;
        };
        let held = match std::mem::replace(&mut session.sync, Sync::Offline) {
            Sync::Subscribing(held) => held,
            other => {
                session.sync = other;
                return;
            }
        };
        let payload = match result {
            Err(e) if e.code == WsErrorCode::Stale.as_str() => {
                log::info!("[client] session {session_id} cursor is stale; installing a snapshot");
                return self.recover(socket, &session_id, held, fx);
            }
            Err(error) => return self.lose(session_id, error, fx),
            Ok(payload) => payload,
        };
        let Some(reply) =
            payload.and_then(|p| serde_json::from_value::<SessionSubscribeReply>(p).ok())
        else {
            let error = WsError {
                code: "bad_reply".into(),
                message: "unreadable subscribe reply".into(),
                details: None,
            };
            return self.lose(session_id, error, fx);
        };
        let cursor = &mut session.cursor;
        if cursor.last_seq > 0
            && cursor
                .log_epoch
                .as_ref()
                .is_some_and(|e| *e != reply.log_epoch)
        {
            log::info!(
                "[client] session {session_id} log epoch changed under the cursor; installing a snapshot"
            );
            *cursor = Cursor::default();
            return self.recover(socket, &session_id, held, fx);
        }
        cursor.log_epoch = Some(reply.log_epoch.clone());
        cursor.head_seq = reply.head_seq;
        session.sync = Sync::Live;
        fx.push(Effect::Output(Output::Subscribed(reply)));
        self.release(&session_id, held, fx);
    }

    fn recovered(
        &mut self,
        session_id: String,
        result: Result<Option<Value>, WsError>,
        fx: &mut Fx,
    ) {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return;
        };
        let held = match std::mem::replace(&mut session.sync, Sync::Offline) {
            Sync::Recovering(held) => held,
            other => {
                session.sync = other;
                return;
            }
        };
        let snapshot = match result {
            Ok(payload) => {
                payload.and_then(|p| serde_json::from_value::<SessionSnapshotEvent>(p).ok())
            }
            Err(error) => return self.lose(session_id, error, fx),
        };
        let Some(snapshot) = snapshot else {
            let error = WsError {
                code: "bad_reply".into(),
                message: "unreadable snapshot".into(),
                details: None,
            };
            return self.lose(session_id, error, fx);
        };
        session.cursor = Cursor {
            log_epoch: Some(snapshot.log_epoch.clone()),
            last_seq: snapshot.watermark,
            head_seq: snapshot.head_seq,
        };
        session.turns = snapshot
            .events
            .iter()
            .filter(|e| e.kind == WsEventType::PromptComplete)
            .filter_map(|e| e.payload.get("turnId").and_then(Value::as_str))
            .map(str::to_owned)
            .collect();
        session.sync = Sync::Live;
        fx.push(Effect::Output(Output::Snapshot(snapshot)));
        self.release(&session_id, held, fx);
    }

    fn lose(&mut self, session_id: String, error: WsError, fx: &mut Fx) {
        log::warn!("[client] session {session_id} refused: {}", error.code);
        self.sessions.remove(&session_id);
        fx.push(Effect::Output(Output::SessionLost { session_id, error }));
    }

    /// Judge what a sync held, in arrival order, against the cursor its reply set.
    fn release(&mut self, session_id: &str, held: Held, fx: &mut Fx) {
        if held.lost {
            self.session_item(session_id, HeldItem::Gap, fx);
        }
        for item in held.items {
            self.session_item(session_id, item, fx);
        }
    }

    fn event(&mut self, event: WsEvent, fx: &mut Fx) {
        match event.sid.clone() {
            Some(sid) if event.seq > 0 => self.session_item(&sid, HeldItem::Event(event), fx),
            _ => fx.push(Effect::Output(Output::Event(event))),
        }
    }

    /// Route one event or gap of a session through its sync state.
    fn session_item(&mut self, session_id: &str, item: HeldItem, fx: &mut Fx) {
        let Some(socket) = self.link.live() else {
            return;
        };
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        match (&mut session.sync, item) {
            (Sync::Subscribing(held) | Sync::Recovering(held), item) => held.push(item),
            (Sync::Offline, _) => {}
            (Sync::Live, HeldItem::Gap) => {
                log::info!("[client] session {session_id} gap frame; installing a snapshot");
                self.recover(socket, session_id, Held::default(), fx);
            }
            (Sync::Live, HeldItem::Event(event)) => match session.cursor.judge(event.seq) {
                Verdict::Duplicate => {}
                Verdict::Gap => {
                    log::info!(
                        "[client] session {session_id} seq {} after {}; installing a snapshot",
                        event.seq,
                        session.cursor.last_seq.max(session.cursor.head_seq)
                    );
                    let mut held = Held::default();
                    held.push(HeldItem::Event(event));
                    self.recover(socket, session_id, held, fx);
                }
                Verdict::Apply => {
                    session.cursor.last_seq = event.seq;
                    if event.kind == WsEventType::PromptComplete {
                        let turn = event
                            .payload
                            .get("turnId")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        if let Some(turn) = turn
                            && !session.turns.insert(turn)
                        {
                            return;
                        }
                    }
                    fx.push(Effect::Output(Output::Event(event)));
                }
            },
        }
    }
}

/// The host takes a prompt's `turnId` as its `commandId` and refuses any other.
fn prompt_command_id(kind: WsRequestType, payload: &Value, ledgered: bool) -> Option<String> {
    if !ledgered || kind != WsRequestType::SendPrompt {
        return None;
    }
    payload.get("turnId").and_then(Value::as_str).map(str::to_owned)
}
