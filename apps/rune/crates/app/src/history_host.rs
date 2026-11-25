//! A chat tab reopens through the model's history open (`openHistorySession`): the payload
//! read (`get_session_payload`), the agent, then `load_session` or `resume_session`, and the
//! end of the replay window. A switched chat's reopen follows its markers to the session the
//! conversation continues in, so its tab moves there with the whole transcript.

use client::cursor::Cursor;
use client::ws::Command;
use model::acp::window::PayloadRead;
use model::acp::{Effect, Event};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use crate::State;
use crate::sync::Pending;

impl State {
    /// Opens a chat tab's session from its history through the model.
    pub(crate) fn reopen_chat(&mut self, session_id: &str) {
        log::info!("[chat] reopen {session_id}");
        match serde_json::from_value(json!({ "openHistorySession": [session_id] })) {
            Ok(event) => self.acp_step(event),
            Err(e) => log::warn!("[chat] could not ask to reopen {session_id}: {e}"),
        }
    }

    /// Runs a history-open effect; any other effect comes back.
    pub(crate) fn run_history_effect(&mut self, effect: Effect) -> Option<Effect> {
        match &effect {
            Effect::ReadSessionPayload { session_id, .. } => {
                let payload = json!({ "sessionId": session_id });
                self.call(WsRequestType::GetSessionPayload, payload, Pending::Payload(Box::new(effect)));
            }
            // The web transport keeps no cache of its own: the host's copy is the one read.
            Effect::ReloadSessionPayload { .. } => self.chat.deferred.push(Event::SessionPayloadRead(effect, Ok(None))),
            Effect::EndReplay { .. } => self.chat.deferred.push(Event::ReplayEnd(effect)),
            Effect::LoadSession { agent_id, session_id, cwd, additional_directories, mcp_servers, .. }
            | Effect::ResumeSession { agent_id, session_id, cwd, additional_directories, mcp_servers, .. } => {
                let kind = match effect {
                    Effect::LoadSession { .. } => WsRequestType::LoadSession,
                    _ => WsRequestType::ResumeSession,
                };
                let mut payload = json!({ "agentId": agent_id, "sessionId": session_id, "cwd": cwd });
                if !additional_directories.is_empty() {
                    payload["additionalDirectories"] = json!(additional_directories);
                }
                if !mcp_servers.is_empty() {
                    payload["mcpServers"] = json!(mcp_servers);
                }
                log::info!("[chat] {kind:?} {session_id}");
                self.command(kind, payload, Pending::Agent(Box::new(effect)));
            }
            _ => return Some(effect),
        }
        None
    }

    /// The payload a history read asked for. A reopened chat is followed live from the end of
    /// the history it installed.
    pub(crate) fn history_payload_read(&mut self, effect: Effect, result: Result<Option<Value>, WsError>) {
        let opened = match &effect {
            Effect::ReadSessionPayload { session_id, read: PayloadRead::History(_) } => Some(session_id.clone()),
            _ => None,
        };
        let outcome = match result {
            Ok(payload) => Ok(payload.filter(|p| !p.is_null())),
            Err(e) => {
                log::warn!("[chat] reading a session's history failed: {}: {}", e.code, e.message);
                Err(e.message)
            }
        };
        let found = matches!(outcome, Ok(Some(_)));
        self.acp_step(Event::SessionPayloadRead(effect, outcome));
        if let Some(session_id) = opened.filter(|_| found) {
            self.follow_history(&session_id);
        }
    }

    fn follow_history(&mut self, session_id: &str) {
        self.chat.requested.insert(session_id.to_owned());
        if !self.chat.loaded.insert(session_id.to_owned()) {
            return;
        }
        let seq = self.chat.acp.book.history_watermarks.get(session_id).copied().unwrap_or(0.0) as u64;
        log::info!("[chat] following {session_id} from seq {seq}");
        let cursor = Cursor { last_seq: seq, head_seq: seq, ..Cursor::default() };
        self.send(Command::Subscribe { session_id: session_id.to_owned(), cursor });
    }
}
