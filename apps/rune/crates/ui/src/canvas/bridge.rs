//! The host side of figcraft's control protocol (`lib/canvas-bridge.ts`), as a machine with no
//! transport: the app moves [`Bridge::take_outbox`] to the editor (the desktop app's in-process
//! editor, or the web client's iframe) and hands the editor's messages to [`Bridge::receive`].
//!
//! Request `{id, method, params, mountSeq}` → reply `{id, ok, result | error: {code, message},
//! mountSeq}`; events `{method: "event.*", params, mountSeq}`. `params` and `result` stay JSON text:
//! figcraft node ids are u64. Per mount, request ids are allocated here only and increase from 1;
//! every envelope carries the mount's `mountSeq` and a message stamped with another is dropped.
//! Calls made before `event.ready` wait in a queue. The bridge never times a call out, but a
//! queued call with a deadline (an agent call the host stops waiting for) still queued when it
//! passes is answered `CANVAS_TIMEOUT` and never reaches the editor. `dispose` answers whatever is
//! pending, the queue included, `CANVAS_CLOSED`.

use std::collections::BTreeSet;

use serde_json::{Value, json};

/// The bridge's request id for one call; its reply comes back under it.
pub type Ticket = u64;

/// Typed errors of the control protocol (`CANVAS_BRIDGE_ERROR_CODES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Closed,
    Closing,
    Hidden,
    OpenElsewhere,
    Timeout,
    UnsupportedInKiosk,
    UnknownMethod,
    EngineError,
    NoActiveTurn,
    AmbiguousSession,
}

const CODES: [(Code, &str); 10] = [
    (Code::Closed, "CANVAS_CLOSED"),
    (Code::Closing, "CANVAS_CLOSING"),
    (Code::Hidden, "CANVAS_HIDDEN"),
    (Code::OpenElsewhere, "CANVAS_OPEN_ELSEWHERE"),
    (Code::Timeout, "CANVAS_TIMEOUT"),
    (Code::UnsupportedInKiosk, "UNSUPPORTED_IN_KIOSK"),
    (Code::UnknownMethod, "UNKNOWN_METHOD"),
    (Code::EngineError, "ENGINE_ERROR"),
    (Code::NoActiveTurn, "NO_ACTIVE_TURN"),
    (Code::AmbiguousSession, "AMBIGUOUS_SESSION"),
];

impl Code {
    pub fn as_str(self) -> &'static str {
        CODES.iter().find(|(c, _)| *c == self).map_or("ENGINE_ERROR", |(_, s)| s)
    }

    /// A code off the wire; one this side does not know is `ENGINE_ERROR`.
    pub fn parse(text: &str) -> Code {
        CODES.iter().find(|(_, s)| *s == text).map_or(Code::EngineError, |(c, _)| *c)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The result's JSON text, unparsed.
    Ok(String),
    Err { code: Code, message: String },
}

impl Reply {
    pub fn failure(code: Code, message: impl Into<String>) -> Reply {
        Reply::Err { code, message: message.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Ready,
    /// The version stays decimal text: it never passes through an f64.
    DirtyChanged { dirty: bool, content_version: String },
    Crashed { reason: String },
}

/// What a call did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Called {
    /// Answered without reaching the editor.
    Now(Reply),
    /// Sent, or queued until the editor is ready; the reply arrives under this ticket.
    Waiting(Ticket),
}

/// Namespaces the editor answers; anything else is `UNKNOWN_METHOD` without a round trip.
const FORWARDED: [&str; 6] = ["engine", "document", "node", "ui", "host", "events"];

fn forwarded(method: &str) -> bool {
    match method.split_once('.') {
        Some((ns, rest)) => !ns.is_empty() && !rest.is_empty() && FORWARDED.contains(&ns),
        None => false,
    }
}

struct Queued {
    envelope: Value,
    id: Ticket,
    deadline: Option<f64>,
}

pub struct Bridge {
    mount_seq: u64,
    next_id: Ticket,
    ready: bool,
    disposed: bool,
    stale_logged: bool,
    pending: BTreeSet<Ticket>,
    queued: Vec<Queued>,
    outbox: Vec<Value>,
    answered: Vec<(Ticket, Reply)>,
    events: Vec<Event>,
}

impl Bridge {
    pub fn new(mount_seq: u64) -> Bridge {
        Bridge {
            mount_seq,
            next_id: 0,
            ready: false,
            disposed: false,
            stale_logged: false,
            pending: BTreeSet::new(),
            queued: Vec::new(),
            outbox: Vec::new(),
            answered: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn mount_seq(&self) -> u64 {
        self.mount_seq
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Calls `method` with `params` (JSON text). `deadline` (ms on the caller's clock): drop the
    /// call unrun if it is still queued for a loading editor then.
    pub fn call(&mut self, method: &str, params: &str, deadline: Option<f64>) -> Called {
        if self.disposed {
            return Called::Now(Reply::failure(Code::Closed, "the canvas is closed"));
        }
        if !forwarded(method) {
            return Called::Now(Reply::failure(Code::UnknownMethod, format!("unknown method {method}")));
        }
        self.next_id += 1;
        let id = self.next_id;
        let envelope = json!({"id": id, "method": method, "params": params, "mountSeq": self.mount_seq});
        self.pending.insert(id);
        if self.ready {
            self.outbox.push(envelope);
        } else {
            self.queued.push(Queued { envelope, id, deadline });
        }
        Called::Waiting(id)
    }

    /// One message from the editor, at `now` on the deadlines' clock.
    pub fn receive(&mut self, message: &Value, now: f64) {
        if !message.is_object() {
            return;
        }
        if message.get("mountSeq").and_then(Value::as_u64) != Some(self.mount_seq) {
            if !self.stale_logged {
                self.stale_logged = true;
                log::info!("[canvas] dropped an editor message for another mount (bridge mountSeq {})", self.mount_seq);
            }
            return;
        }
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            if self.pending.remove(&id) {
                self.answered.push((id, parse_reply(message)));
            }
            return;
        }
        let Some(method) = message.get("method").and_then(Value::as_str) else { return };
        let params = message.get("params").and_then(Value::as_str).unwrap_or_default();
        let Some(event) = parse_event(method, params) else { return };
        if event == Event::Ready {
            if self.ready {
                // The editor reloaded: what was sent to the old one gets no answer.
                for id in std::mem::take(&mut self.pending) {
                    self.answered.push((id, Reply::failure(Code::Closed, "the editor reloaded")));
                }
            } else {
                self.ready = true;
                for q in std::mem::take(&mut self.queued) {
                    if q.deadline.is_some_and(|d| now >= d) {
                        self.pending.remove(&q.id);
                        self.answered.push((q.id, host_stopped_waiting()));
                    } else {
                        self.outbox.push(q.envelope);
                    }
                }
            }
        }
        self.events.push(event);
    }

    /// Answers everything pending, the queue included, `CANVAS_CLOSED`; later calls are too.
    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.queued.clear();
        self.outbox.clear();
        for id in std::mem::take(&mut self.pending) {
            self.answered.push((id, Reply::failure(Code::Closed, "the canvas was unmounted")));
        }
    }

    /// Envelopes for the editor, in order.
    pub fn take_outbox(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.outbox)
    }

    pub fn take_answered(&mut self) -> Vec<(Ticket, Reply)> {
        std::mem::take(&mut self.answered)
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

pub fn host_stopped_waiting() -> Reply {
    Reply::failure(Code::Timeout, "the host stopped waiting before the canvas was ready")
}

/// Builds the AD-22 kiosk iframe URL with the canvas token as a path segment (`buildCanvasEmbedUrl`).
/// Query keys are fixed to `embed`, `ho`, and `ms` so untrusted ids can never open `?control`.
pub fn build_embed_url(base_url: &str, canvas_id: &str, canvas_token: &str, host_origin: &str, mount_seq: u64) -> String {
    let base = base_url.trim_end_matches('/');
    let canvas = crate::composer::encode_query(canvas_id);
    let token = crate::composer::encode_query(canvas_token);
    let ho = crate::composer::encode_query(host_origin);
    format!("{base}/canvas/{canvas}/{token}/?embed=kiosk&ho={ho}&ms={mount_seq}")
}

/// Resolves the origin an editor iframe is served from (`canvasOriginOf`): an empty `base_url`
/// (web same-origin) uses `page_origin`; otherwise the scheme + host + optional port of `base_url`.
pub fn canvas_origin_of(base_url: &str, page_origin: &str) -> String {
    if base_url.is_empty() {
        return page_origin.to_owned();
    }
    let Some(scheme_end) = base_url.find("://") else {
        return page_origin.to_owned();
    };
    let rest = &base_url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    format!("{}{}", &base_url[..scheme_end + 3], &rest[..authority_end])
}

fn parse_reply(message: &Value) -> Reply {
    match message.get("ok").and_then(Value::as_bool) {
        Some(true) => match message.get("result").and_then(Value::as_str) {
            Some(result) => Reply::Ok(result.to_owned()),
            None => Reply::failure(Code::EngineError, "malformed reply from the editor"),
        },
        Some(false) => match message.get("error").filter(|e| e.is_object()) {
            Some(error) => {
                let code = error.get("code").and_then(Value::as_str);
                let message = error.get("message").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| code.unwrap_or("ENGINE_ERROR").to_owned());
                Reply::Err { code: code.map_or(Code::EngineError, Code::parse), message }
            }
            None => Reply::failure(Code::EngineError, "malformed reply from the editor"),
        },
        None => Reply::failure(Code::EngineError, "malformed reply from the editor"),
    }
}

/// The fork sends `{"dirty":<bool>,"contentVersion":<u64>}`; the version is kept as its digits.
fn parse_event(method: &str, params: &str) -> Option<Event> {
    match method {
        "event.ready" => Some(Event::Ready),
        "event.dirtyChanged" => {
            let dirty = field(params, "dirty")?;
            let version = field(params, "contentVersion")?;
            let dirty = match dirty {
                "true" => true,
                "false" => false,
                _ => return None,
            };
            version.bytes().all(|b| b.is_ascii_digit()).then(|| Event::DirtyChanged { dirty, content_version: version.to_owned() })
        }
        "event.crashed" => {
            let reason = serde_json::from_str::<Value>(params).ok().and_then(|v| v.get("reason").and_then(Value::as_str).map(str::to_owned));
            Some(Event::Crashed { reason: reason.unwrap_or_else(|| "unknown".into()) })
        }
        _ => None,
    }
}

/// The bare value text after `"name":` in a flat JSON object.
fn field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("\"{name}\"");
    let after = &text[text.find(&key)? + key.len()..];
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    let end = after.find(|c: char| c == ',' || c == '}' || c.is_whitespace()).unwrap_or(after.len());
    Some(&after[..end]).filter(|v| !v.is_empty())
}
