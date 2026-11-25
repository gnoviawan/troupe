//! The store's read-only helpers that views call: one-line predicates over
//! `State` or a value the view already holds.

use serde_json::Value;

use super::State;
use super::create::QUICK_CHAT_PROJECT;
use super::reuse_key::parse_reuse_key;
use super::session::RESUME_FAILED_PREFIX;
use super::splice::SPLICE_PREFIX;
use super::switch::{COMMANDS_BLOCK, SUBAGENTS_BLOCK};
use super::types::{ChatMessage, Role};

/// `isBackgroundWorkBlock`: only background subagents or commands block the
/// switch.
pub fn is_background_work_block(reason: Option<&str>) -> bool {
    matches!(reason, Some(SUBAGENTS_BLOCK | COMMANDS_BLOCK))
}

/// `isResumeFailureError`: `lastError` records a failed resume, not a turn
/// note.
pub fn is_resume_failure_error(last_error: Option<&str>) -> bool {
    last_error.is_some_and(|error| error.starts_with(RESUME_FAILED_PREFIX))
}

/// `hasModelRelevantOptionsCache`: the cached options list models, or a
/// model config option with choices.
pub fn has_model_relevant_options_cache(entry: Option<&Value>) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    let listed = |value: Option<&Value>| {
        value
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
    };
    if listed(entry.pointer("/models/availableModels")) {
        return true;
    }
    entry["configOptions"].as_array().is_some_and(|options| {
        options
            .iter()
            .any(|option| option["category"] == "model" && listed(option.get("options")))
    })
}

/// `configIdFromReuseKey`.
pub fn config_id_from_reuse_key(key: &str) -> &str {
    parse_reuse_key(key).config_id
}

/// `lastOwnUserPromptBlocks`: the blocks of the chat's last user prompt that
/// is not spliced pre-switch history.
pub fn last_own_user_prompt_blocks(messages: &[ChatMessage]) -> Option<&[Value]> {
    messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User && !message.id.starts_with(SPLICE_PREFIX))
        .map(|message| message.blocks.as_slice())
}

impl State {
    /// `isEphemeralAcpSession`: a warm-pool or other backend session that is
    /// not a saved chat.
    pub fn is_ephemeral_session(&self, session_id: &str) -> bool {
        self.book.ephemeral_sessions.contains(session_id)
    }

    /// `isAnyAgentAuthInFlight`.
    pub fn is_any_agent_auth_in_flight(&self) -> bool {
        !self.book.auth_flights.is_empty()
    }

    /// `sessionProjectId`: the live record's project, else its index entry's.
    /// An empty id and the unassigned sentinel name no project.
    pub fn session_project_id(&self, session_id: &str) -> Option<&str> {
        fn project(value: Option<&Value>) -> Option<&str> {
            value.and_then(Value::as_str).filter(|id| !id.is_empty())
        }
        let live = self
            .sessions
            .get(session_id)
            .and_then(|session| project(session.rest.get("projectId")));
        let project_id = live.or_else(|| {
            self.session_index
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id))
                .and_then(|entry| project(entry.get("projectId")))
        })?;
        (project_id != QUICK_CHAT_PROJECT).then_some(project_id)
    }
}

/// The switch gate as the composer's agent menu shows it (`useSwitchGate`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwitchGate {
    /// The busy gate refuses a switch now.
    pub blocked: bool,
    /// A turn runs (`sessionTurnBusy`).
    pub turn_running: bool,
    /// The running turn is all that blocks, so cancel-then-switch can clear it.
    pub turn_busy: bool,
}

impl State {
    /// `useSwitchGate`.
    pub fn switch_gate(&self, session_id: &str) -> SwitchGate {
        let turn_running = self.sessions.get(session_id).is_some_and(|s| s.turn_busy());
        let besides = self.switch_blocked_besides_turn(session_id).is_some();
        SwitchGate {
            blocked: turn_running || besides,
            turn_running,
            turn_busy: turn_running && !besides,
        }
    }

    /// The warm session an armed switch prepared for its target
    /// (`preparedSessions[prepareChatKey(toConfigId, cwd)]`), whose models
    /// the composer shows until the switch runs.
    pub fn switch_target_session(&self, session_id: &str) -> Option<&super::types::Session> {
        let session = self.sessions.get(session_id)?;
        let target = session.rest.get("switching")?.get("toConfigId")?.as_str()?;
        let cwd = session.rest.get("cwd").and_then(Value::as_str).unwrap_or_default();
        let key = super::warm::prepare_chat_key(target, cwd, None);
        self.sessions.get(self.prepared_sessions.get(&key)?)
    }
}
