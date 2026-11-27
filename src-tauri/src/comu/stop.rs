//! Per-session Stop latch and active-session indicator events for Troupe Comu.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use tokio::sync::watch;

use crate::web::EventSink;

/// Session-scoped event emitted when an agent starts or ends Computer Use in a chat.
pub const EVENT_COMU_SESSION_ACTIVE: &str = "acp:comu_session_active";
/// Session-scoped event emitted when Stop latches or clears on the next user turn.
pub const EVENT_COMU_STOP_CHANGED: &str = "acp:comu_stop_changed";

struct SessionSlot {
    epoch: u64,
    stopped: bool,
    tx: watch::Sender<u64>,
}

/// Token held by an in-flight Comu call; resolves as soon as Stop is pressed.
#[derive(Clone)]
pub struct ComuStopToken {
    epoch: u64,
    rx: watch::Receiver<u64>,
}

impl ComuStopToken {
    pub async fn wait_stopped(&mut self) {
        while *self.rx.borrow_and_update() == self.epoch {
            if self.rx.changed().await.is_err() {
                std::future::pending::<()>().await;
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
        if self.active_sessions.lock().insert(session_id.to_string()) {
            crate::acp::events::fan_out(
                &self.sinks,
                Some(session_id),
                EVENT_COMU_SESSION_ACTIVE,
                &json!({ "sessionId": session_id, "active": true }),
            );
        }
    }

    /// Latch `session_id` stopped and cancel any in-flight call. Returns `true`
    /// when newly stopped.
    pub fn stop(&self, session_id: &str) -> bool {
        let newly_stopped = {
            let mut slots = self.slots.lock();
            let slot = slots.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = watch::channel(0);
                SessionSlot {
                    epoch: 0,
                    stopped: false,
                    tx,
                }
            });
            let first = !slot.stopped;
            slot.stopped = true;
            slot.epoch = slot.epoch.saturating_add(1);
            let _ = slot.tx.send(slot.epoch);
            first
        };
        crate::acp::events::fan_out(
            &self.sinks,
            Some(session_id),
            EVENT_COMU_STOP_CHANGED,
            &json!({ "sessionId": session_id, "stopped": true }),
        );
        newly_stopped
    }

    /// Clear Stop on the user's next prompt turn.
    pub fn resume_on_user_turn(&self, session_id: &str) -> bool {
        let cleared = {
            let mut slots = self.slots.lock();
            if let Some(slot) = slots.get_mut(session_id) {
                if slot.stopped {
                    slot.stopped = false;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if cleared {
            crate::acp::events::fan_out(
                &self.sinks,
                Some(session_id),
                EVENT_COMU_STOP_CHANGED,
                &json!({ "sessionId": session_id, "stopped": false }),
            );
        }
        cleared
    }

    /// Drop session state on close.
    pub fn forget_session(&self, session_id: &str) {
        self.slots.lock().remove(session_id);
        if self.active_sessions.lock().remove(session_id) {
            crate::acp::events::fan_out(
                &self.sinks,
                Some(session_id),
                EVENT_COMU_SESSION_ACTIVE,
                &json!({ "sessionId": session_id, "active": false }),
            );
        }
    }
}
