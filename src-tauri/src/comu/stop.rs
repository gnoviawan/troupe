//! Per-session Stop latch and active-session indicator events for Troupe Comu.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde_json::json;
use tokio::sync::watch;

use crate::web::EventSink;

/// Session-scoped event emitted when an agent starts or ends Computer Use in a chat.
pub const EVENT_COMU_SESSION_ACTIVE: &str = "acp:comu_session_active";
/// Session-scoped event emitted when Stop latches or clears on the next user turn.
pub const EVENT_COMU_STOP_CHANGED: &str = "acp:comu_stop_changed";

/// Authoritative Computer Use activity and Stop state for one session,
/// attached to `/ws` `subscribe` and `recover_session_snapshot` replies so
/// reconnecting clients recover missed live-only transitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComuSessionRuntimeState {
    pub active: bool,
    pub stopped: bool,
    pub seq: u64,
}

#[derive(Default)]
struct LiveComuRegistry {
    seq: u64,
    /// `session_id -> (active, stopped)`
    sessions: HashMap<String, (bool, bool)>,
}

static LIVE_COMU_REGISTRY: LazyLock<Mutex<LiveComuRegistry>> =
    LazyLock::new(|| Mutex::new(LiveComuRegistry::default()));

/// Read the current Computer Use `(active, stopped, seq)` state for `session_id`.
#[must_use]
pub fn session_runtime_state(session_id: &str) -> ComuSessionRuntimeState {
    let reg = LIVE_COMU_REGISTRY.lock();
    let (active, stopped) = reg.sessions.get(session_id).copied().unwrap_or((false, false));
    ComuSessionRuntimeState {
        active,
        stopped,
        seq: reg.seq,
    }
}

struct SessionSlot {
    epoch: u64,
    stopped: bool,
    tx: watch::Sender<u64>,
}

/// Token held by an in-flight Comu call; resolves as soon as Stop is pressed
/// or the session slot is dropped on teardown.
#[derive(Clone)]
pub struct ComuStopToken {
    epoch: u64,
    rx: watch::Receiver<u64>,
}

impl ComuStopToken {
    #[must_use]
    pub fn is_stopped(&mut self) -> bool {
        *self.rx.borrow_and_update() != self.epoch || self.rx.has_changed().is_err()
    }

    pub async fn wait_stopped(&mut self) {
        while *self.rx.borrow_and_update() == self.epoch {
            if self.rx.changed().await.is_err() {
                return;
            }
        }
    }
}

pub struct ComuStopRegistry {
    sinks: Vec<Arc<dyn EventSink>>,
    slots: Mutex<HashMap<String, SessionSlot>>,
    active_sessions: Mutex<HashSet<String>>,
}

impl ComuStopRegistry {
    #[must_use]
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self {
            sinks,
            slots: Mutex::new(HashMap::new()),
            active_sessions: Mutex::new(HashSet::new()),
        }
    }

    /// Issue a cancellation token if `session_id` is not stopped; returns `None`
    /// when stopped by the user until the next prompt turn.
    pub fn admit(&self, session_id: &str) -> Option<ComuStopToken> {
        let mut slots = self.slots.lock();
        let slot = slots.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = watch::channel(0);
            SessionSlot {
                epoch: 0,
                stopped: false,
                tx,
            }
        });
        if slot.stopped {
            return None;
        }
        Some(ComuStopToken {
            epoch: slot.epoch,
            rx: slot.tx.subscribe(),
        })
    }

    /// Mark `session_id` as actively using Computer Use so UI surfaces show the Stop bar.
    pub fn mark_active(&self, session_id: &str) {
        let mut active = self.active_sessions.lock();
        if active.insert(session_id.to_string()) {
            let seq = {
                let mut reg = LIVE_COMU_REGISTRY.lock();
                reg.seq = reg.seq.saturating_add(1);
                reg.sessions
                    .entry(session_id.to_string())
                    .or_insert((false, false))
                    .0 = true;
                reg.seq
            };
            crate::acp::events::fan_out(
                &self.sinks,
                Some(session_id),
                EVENT_COMU_SESSION_ACTIVE,
                &json!({ "sessionId": session_id, "active": true, "seq": seq }),
            );
        }
    }

    /// The turn or the agent's own work that drove Computer Use in
    /// `session_id` ended: hide the Stop bar until the next Comu action.
    pub fn mark_inactive(&self, session_id: &str) {
        let mut active = self.active_sessions.lock();
        if !active.remove(session_id) {
            return;
        }
        let seq = {
            let mut reg = LIVE_COMU_REGISTRY.lock();
            reg.seq = reg.seq.saturating_add(1);
            if let Some(entry) = reg.sessions.get_mut(session_id) {
                entry.0 = false;
            }
            reg.seq
        };
        crate::acp::events::fan_out(
            &self.sinks,
            Some(session_id),
            EVENT_COMU_SESSION_ACTIVE,
            &json!({ "sessionId": session_id, "active": false, "seq": seq }),
        );
        log::info!(
            "[comu] session {} no longer drives Computer Use",
            crate::logging::redact_session_id(session_id)
        );
    }

    /// Latch `session_id` stopped and cancel any in-flight call. Returns `true`
    /// when newly stopped. Emits `EVENT_COMU_STOP_CHANGED` while holding the
    /// slot lock so latch transitions arrive in strict causal order.
    pub fn stop(&self, session_id: &str) -> bool {
        let mut slots = self.slots.lock();
        let slot = slots.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = watch::channel(0);
            SessionSlot {
                epoch: 0,
                stopped: false,
                tx,
            }
        });
        let newly_stopped = !slot.stopped;
        slot.stopped = true;
        slot.epoch = slot.epoch.saturating_add(1);
        let _ = slot.tx.send_replace(slot.epoch);
        let seq = {
            let mut reg = LIVE_COMU_REGISTRY.lock();
            reg.seq = reg.seq.saturating_add(1);
            reg.sessions
                .entry(session_id.to_string())
                .or_insert((false, false))
                .1 = true;
            reg.seq
        };
        crate::acp::events::fan_out(
            &self.sinks,
            Some(session_id),
            EVENT_COMU_STOP_CHANGED,
            &json!({ "sessionId": session_id, "stopped": true, "seq": seq }),
        );
        newly_stopped
    }

    /// Clear Stop on the user's next prompt turn.
    pub fn resume_on_user_turn(&self, session_id: &str) -> bool {
        let mut slots = self.slots.lock();
        let Some(slot) = slots.get_mut(session_id) else {
            return false;
        };
        if !slot.stopped {
            return false;
        }
        slot.stopped = false;
        let seq = {
            let mut reg = LIVE_COMU_REGISTRY.lock();
            reg.seq = reg.seq.saturating_add(1);
            if let Some(entry) = reg.sessions.get_mut(session_id) {
                entry.1 = false;
            }
            reg.seq
        };
        crate::acp::events::fan_out(
            &self.sinks,
            Some(session_id),
            EVENT_COMU_STOP_CHANGED,
            &json!({ "sessionId": session_id, "stopped": false, "seq": seq }),
        );
        true
    }

    /// Drop session state on close; dropping `slot.tx` immediately wakes any
    /// `ComuStopToken::wait_stopped` awaiting on that session.
    pub fn forget_session(&self, session_id: &str) {
        self.slots.lock().remove(session_id);
        let was_active = self.active_sessions.lock().remove(session_id);
        let seq = {
            let mut reg = LIVE_COMU_REGISTRY.lock();
            let removed = reg.sessions.remove(session_id).is_some();
            if was_active || removed {
                reg.seq = reg.seq.saturating_add(1);
            }
            reg.seq
        };
        if was_active {
            crate::acp::events::fan_out(
                &self.sinks,
                Some(session_id),
                EVENT_COMU_SESSION_ACTIVE,
                &json!({ "sessionId": session_id, "active": false, "seq": seq }),
            );
        }
    }
}
