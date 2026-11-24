//! Plan limits an agent reports as plain turn text. Antigravity ends a turn
//! normally after one `agent_message_chunk` that reads "Usage Limit Reached";
//! the chunk marks the session here, and the turn's end turns the mark into a
//! plan-limit error. Only an agent the driver enabled this for is watched.

use std::collections::HashMap;

use agent_client_protocol::schema::v1::SessionUpdate;

use crate::acp::antigravity_accounts::usage::AntigravityQuotaHit;

#[derive(Default)]
pub(crate) struct QuotaHits {
    enabled: bool,
    hits: HashMap<String, AntigravityQuotaHit>,
}

impl QuotaHits {
    /// Watch this agent's text for the Antigravity limit message.
    pub(crate) fn enable(&mut self) {
        self.enabled = true;
    }

    /// Mark `session_id` when `update` is the limit message. Returns whether
    /// it marked.
    pub(crate) fn note(&mut self, session_id: &str, update: &SessionUpdate) -> bool {
        if !self.enabled {
            return false;
        }
        let Some(hit) = crate::acp::client::antigravity_quota_chunk(update) else {
            return false;
        };
        self.hits.insert(session_id.to_string(), hit);
        true
    }

    /// Forget the mark of `session_id` (a new turn starts).
    pub(crate) fn clear(&mut self, session_id: &str) {
        self.hits.remove(session_id);
    }

    /// Take the mark the turn of `session_id` left, if any.
    pub(crate) fn take(&mut self, session_id: &str) -> Option<AntigravityQuotaHit> {
        self.hits.remove(session_id)
    }
}

#[cfg(test)]
#[path = "quota_hits_tests.rs"]
mod tests;
