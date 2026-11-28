//! Which sessions of each runtime agent may call host tools right now.
//!
//! A session is routable while a prompt turn runs, and while the agent works
//! in it outside a prompt turn (`acp::session::unprompted`): Claude Code wakes
//! up when a background shell finishes and runs a turn of its own, and its
//! tool calls then arrive with no prompt open.

use std::collections::{HashMap, HashSet};

use parking_lot::Mutex;

#[derive(Default)]
pub(super) struct TurnRoutes {
    /// Runtime agent id -> sessions with an accepted prompt in flight.
    prompt: Mutex<HashMap<String, HashSet<String>>>,
    /// Runtime agent id -> sessions the agent works in on its own.
    unprompted: Mutex<HashMap<String, HashSet<String>>>,
}

/// Returns whether `session_id` was newly added.
fn insert(
    map: &Mutex<HashMap<String, HashSet<String>>>,
    agent_id: &str,
    session_id: &str,
) -> bool {
    map.lock()
        .entry(agent_id.to_string())
        .or_default()
        .insert(session_id.to_string())
}

fn remove(map: &Mutex<HashMap<String, HashSet<String>>>, agent_id: &str, session_id: &str) {
    let mut map = map.lock();
    if let Some(sessions) = map.get_mut(agent_id) {
        sessions.remove(session_id);
        if sessions.is_empty() {
            map.remove(agent_id);
        }
    }
}

impl TurnRoutes {
    pub(super) fn begin_prompt(&self, agent_id: &str, session_id: &str) {
        insert(&self.prompt, agent_id, session_id);
    }

    pub(super) fn end_prompt(&self, agent_id: &str, session_id: &str) {
        remove(&self.prompt, agent_id, session_id);
    }

    /// Returns whether the session was not already working on its own.
    pub(super) fn begin_unprompted(&self, agent_id: &str, session_id: &str) -> bool {
        insert(&self.unprompted, agent_id, session_id)
    }

    pub(super) fn end_unprompted(&self, agent_id: &str, session_id: &str) {
        remove(&self.unprompted, agent_id, session_id);
    }

    /// Whether `session_id` of `agent_id` still runs a prompt turn.
    pub(super) fn prompt_active(&self, agent_id: &str, session_id: &str) -> bool {
        self.prompt
            .lock()
            .get(agent_id)
            .is_some_and(|sessions| sessions.contains(session_id))
    }

    /// Whether `session_id` of `agent_id` is still working on its own.
    pub(super) fn unprompted_active(&self, agent_id: &str, session_id: &str) -> bool {
        self.unprompted
            .lock()
            .get(agent_id)
            .is_some_and(|sessions| sessions.contains(session_id))
    }

    /// Whether `agent_id` runs a prompt turn in any session.
    pub(super) fn any_prompt(&self, agent_id: &str) -> bool {
        self.prompt.lock().contains_key(agent_id)
    }

    /// Every routable session of `agent_id`: prompt turns and unprompted work.
    pub(super) fn routable(&self, agent_id: &str) -> HashSet<String> {
        let mut sessions = self
            .prompt
            .lock()
            .get(agent_id)
            .cloned()
            .unwrap_or_default();
        if let Some(unprompted) = self.unprompted.lock().get(agent_id) {
            sessions.extend(unprompted.iter().cloned());
        }
        sessions
    }

    /// Drop `session_id` from every agent (the session closed).
    pub(super) fn forget_session(&self, session_id: &str) {
        for map in [&self.prompt, &self.unprompted] {
            map.lock().retain(|_, sessions| {
                sessions.remove(session_id);
                !sessions.is_empty()
            });
        }
    }
}
