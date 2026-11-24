//! Requests the app asked for, kept until their reply arrives.
//!
//! A ledgered request carries a `commandId` minted once. When its socket dies
//! before the reply, the request goes back to the queue and the next live
//! socket resends it with the same `commandId`, so the host runs it at most
//! once (AD-5). An unledgered request that was on the wire fails with
//! `closed` instead, because resending it could run it twice.
//!
//! The host keeps one ledger for every client and carrier, so a `commandId`
//! is `{client}-{carrier}{seed}-{n}`. `client` is the `client_instance_id`
//! when it is short and plain, else `_` and a hash of it, so two clients never
//! share it. `carrier` keeps `/ws` and `/terminal/ws` apart, and `seed`
//! ([`Config::seed`]) keeps apart two clients built for one window.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::machine::{CallId, Config};

/// The socket a ledger mints for; the host keeps one ledger for both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    Ws,
    Terminal,
}

/// The longest `client_instance_id` used as is; the id stays within the host's
/// 128-byte `commandId` limit.
const PLAIN_CLIENT_MAX: usize = 64;

/// How long to wait before asking again about a command the host reports
/// still running (`{ "receipt": "in_progress" }`).
pub const IN_PROGRESS_RETRY_S: f64 = 1.0;

/// Where one request stands.
#[derive(Clone, Debug, PartialEq)]
pub enum CallState {
    /// Waiting for a live socket.
    Queued,
    /// On the wire under this request id.
    Sent { request: String },
    /// The host is still running the first send; ask again at `retry_at`.
    InProgress { retry_at: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call<K> {
    /// Submission order: calls go on the wire in the order the app made them.
    order: u64,
    pub kind: K,
    pub payload: Value,
    pub command_id: Option<String>,
    pub state: CallState,
}

#[derive(Clone, Debug)]
pub struct Ledger<K> {
    calls: BTreeMap<CallId, Call<K>>,
    prefix: String,
    added: u64,
}

impl<K: Clone> Ledger<K> {
    #[must_use]
    pub fn new(config: &Config, carrier: Carrier) -> Self {
        let id = &config.client_instance_id;
        let plain = !id.is_empty()
            && id.len() <= PLAIN_CLIENT_MAX
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let client = if plain {
            id.clone()
        } else {
            format!("_{:016x}", fnv1a(id.as_bytes()))
        };
        let carrier = match carrier {
            Carrier::Ws => 'w',
            Carrier::Terminal => 't',
        };
        let prefix = format!("{client}-{carrier}{:x}", config.seed);
        Self {
            calls: BTreeMap::new(),
            prefix,
            added: 0,
        }
    }

    /// Queue a request. A ledgered one gets its `commandId` now. Returns
    /// `false`, and keeps the earlier request, when `call` is already in use.
    #[must_use]
    pub fn add(&mut self, call: CallId, kind: K, payload: Value, ledgered: bool) -> bool {
        if self.calls.contains_key(&call) {
            return false;
        }
        self.added += 1;
        let command_id = ledgered.then(|| format!("{}-{}", self.prefix, self.added));
        self.calls.insert(
            call,
            Call {
                order: self.added,
                kind,
                payload,
                command_id,
                state: CallState::Queued,
            },
        );
        true
    }

    /// The requests to put on the wire now, in the order the app made them.
    #[must_use]
    pub fn due(&self, now: f64) -> Vec<CallId> {
        let mut due: Vec<(u64, CallId)> = self
            .calls
            .iter()
            .filter(|(_, c)| match c.state {
                CallState::Queued => true,
                CallState::InProgress { retry_at } => now >= retry_at,
                CallState::Sent { .. } => false,
            })
            .map(|(id, c)| (c.order, *id))
            .collect();
        due.sort_unstable();
        due.into_iter().map(|(_, id)| id).collect()
    }

    #[must_use]
    pub fn get(&self, call: CallId) -> Option<&Call<K>> {
        self.calls.get(&call)
    }

    pub fn mark_sent(&mut self, call: CallId, request: String) {
        if let Some(c) = self.calls.get_mut(&call) {
            c.state = CallState::Sent { request };
        }
    }

    /// The host is still running it: returns when to ask again.
    pub fn mark_in_progress(&mut self, call: CallId, now: f64) -> f64 {
        let retry_at = now + IN_PROGRESS_RETRY_S;
        if let Some(c) = self.calls.get_mut(&call) {
            c.state = CallState::InProgress { retry_at };
        }
        retry_at
    }

    pub fn take(&mut self, call: CallId) -> Option<Call<K>> {
        self.calls.remove(&call)
    }

    /// The socket died. Ledgered requests wait for the next one; the
    /// unledgered requests that were on the wire are returned as failed.
    pub fn down(&mut self) -> Vec<(CallId, Call<K>)> {
        let failed: Vec<CallId> = self
            .calls
            .iter()
            .filter(|(_, c)| c.command_id.is_none() && matches!(c.state, CallState::Sent { .. }))
            .map(|(id, _)| *id)
            .collect();
        for c in self.calls.values_mut() {
            c.state = CallState::Queued;
        }
        failed
            .into_iter()
            .filter_map(|id| self.calls.remove(&id).map(|c| (id, c)))
            .collect()
    }
}

/// 64-bit FNV-1a: stable on every target, enough to keep client ids apart.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The host's answer to a resend whose first run is still going.
#[must_use]
pub fn is_in_progress_receipt(payload: Option<&Value>) -> bool {
    payload
        .and_then(|p| p.get("receipt"))
        .and_then(Value::as_str)
        == Some("in_progress")
}
