//! One CDP connection over the pipe Chromium opens for
//! `--remote-debugging-pipe`: the host writes requests to the child's fd 3
//! and reads replies and events from its fd 4, each message one JSON
//! object followed by a NUL byte.
//!
//! A reader thread and a writer thread own the two pipe ends, so no async
//! task ever blocks on the pipe. When the reader sees the pipe close, every
//! pending call fails with `NoReply` and the event channel closes, which is
//! how the owner learns that Chromium exited.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::super::cdp::CdpMethod;
use super::super::{EngineError, EngineErrorKind};
use crate::browser_automation::budgets::CDP_RESULT;

/// The largest message the host reads. A screenshot of a large viewport is
/// a few MB of base64; anything far past that is a broken peer.
const MAX_MESSAGE: u64 = 64 * 1024 * 1024;

/// One CDP event. `session` is the attached tab it came from, `None` for
/// the browser target itself.
#[derive(Debug, Clone)]
pub struct CdpEvent {
    pub session: Option<String>,
    pub method: String,
    pub params: Value,
}

type Reply = Result<Value, EngineErrorKind>;

/// `None` once the pipe closed: later calls fail at once.
type Pending = Arc<Mutex<Option<HashMap<u64, oneshot::Sender<Reply>>>>>;

pub struct Connection {
    writer: std::sync::mpsc::Sender<Vec<u8>>,
    pending: Pending,
    next_id: AtomicU64,
}

impl Connection {
    /// Start the reader and writer threads over the host's pipe ends.
    /// Events go to `events` until the pipe closes.
    pub fn start(
        read: File,
        write: File,
        events: mpsc::UnboundedSender<CdpEvent>,
    ) -> std::io::Result<Self> {
        let pending: Pending = Arc::new(Mutex::new(Some(HashMap::new())));
        let (writer, outgoing) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("chromium-cdp-write".into())
            .spawn(move || write_loop(write, &outgoing))?;
        let reader_pending = pending.clone();
        std::thread::Builder::new()
            .name("chromium-cdp-read".into())
            .spawn(move || read_loop(read, &reader_pending, &events))?;
        Ok(Self {
            writer,
            pending,
            next_id: AtomicU64::new(1),
        })
    }

    /// Issue `method` on `session` (`None`: the browser) and wait for its
    /// reply. A protocol error is `Protocol`; it never carries the
    /// engine's message, which can quote page content.
    pub async fn call(
        &self,
        session: Option<&str>,
        method: CdpMethod,
        params: Value,
    ) -> Result<Value, EngineError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        match self.pending.lock().as_mut() {
            Some(map) => map.insert(id, tx),
            None => return Err(EngineError::new(EngineErrorKind::Dispatch)),
        };
        let mut message = json!({ "id": id, "method": method.as_str(), "params": params });
        if let Some(session) = session {
            message["sessionId"] = Value::String(session.to_string());
        }
        let sent = serde_json::to_vec(&message)
            .ok()
            .is_some_and(|bytes| self.writer.send(bytes).is_ok());
        if !sent {
            self.forget(id);
            return Err(EngineError::new(EngineErrorKind::Dispatch));
        }
        match tokio::time::timeout(CDP_RESULT, rx).await {
            Err(_) => {
                self.forget(id);
                Err(EngineError::new(EngineErrorKind::Timeout))
            }
            Ok(Err(_)) => Err(EngineError::new(EngineErrorKind::NoReply)),
            Ok(Ok(reply)) => reply.map_err(EngineError::new),
        }
    }

    /// Whether the pipe is still open.
    pub fn is_open(&self) -> bool {
        self.pending.lock().is_some()
    }

    fn forget(&self, id: u64) {
        if let Some(map) = self.pending.lock().as_mut() {
            map.remove(&id);
        }
    }
}

fn write_loop(mut pipe: File, outgoing: &std::sync::mpsc::Receiver<Vec<u8>>) {
    while let Ok(mut message) = outgoing.recv() {
        message.push(0);
        if pipe
            .write_all(&message)
            .and_then(|()| pipe.flush())
            .is_err()
        {
            log::warn!("[chromium] CDP pipe write failed; the browser is gone");
            return;
        }
    }
}

fn read_loop(pipe: File, pending: &Pending, events: &mpsc::UnboundedSender<CdpEvent>) {
    let mut reader = BufReader::new(pipe);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let read = reader.by_ref().take(MAX_MESSAGE).read_until(0, &mut buf);
        match read {
            Ok(0) => break,
            Ok(_) if buf.last() != Some(&0) => {
                log::warn!("[chromium] CDP message over {MAX_MESSAGE} bytes or truncated; closing the pipe");
                break;
            }
            Ok(_) => {
                buf.pop();
                route(&buf, pending, events);
            }
            Err(_) => break,
        }
    }
    // Dropping every sender fails each waiting call with `NoReply`.
    pending.lock().take();
}

fn route(message: &[u8], pending: &Pending, events: &mpsc::UnboundedSender<CdpEvent>) {
    let Ok(mut value) = serde_json::from_slice::<Value>(message) else {
        log::warn!("[chromium] unparseable CDP message dropped");
        return;
    };
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        let reply = match value.get_mut("result") {
            Some(result) => Ok(result.take()),
            None if value.get("error").is_some() => Err(EngineErrorKind::Protocol),
            None => Err(EngineErrorKind::Malformed),
        };
        let waiter = pending.lock().as_mut().and_then(|map| map.remove(&id));
        if let Some(waiter) = waiter {
            let _ = waiter.send(reply);
        }
        return;
    }
    let Some(method) = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let session = value
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let params = value
        .get_mut("params")
        .map(Value::take)
        .unwrap_or(Value::Null);
    let _ = events.send(CdpEvent {
        session,
        method,
        params,
    });
}
