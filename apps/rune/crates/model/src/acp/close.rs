//! The user closing or deleting a chat (`closeSession` and
//! `deleteHistorySession` in `slices/session.ts`).
//!
//! Each host call is an effect the app answers with
//! `Event::SessionCommandDone` or `Event::SessionCommandFailed` and the
//! error's message; the rest of the action runs on that answer, as it runs
//! after the `await` in TypeScript.

use serde_json::Value;

use super::js;
use super::types::SessionStatus;
use super::{Effect, State};

impl State {
    /// `closeSession`: end the agent's session first when it is open.
    pub(crate) fn close_session(&mut self, session_id: String, fx: &mut Vec<Effect>) {
        match self.sessions.get(&session_id) {
            Some(session) if session.status != SessionStatus::Closed => {
                fx.push(Effect::CloseAgentSession {
                    agent_id: session.agent_id.clone(),
                    session_id,
                });
            }
            _ => self.finish_close(&session_id, fx),
        }
    }

    /// The rest of `closeSession`: the chat closes, what waited on it goes,
    /// and its transcript is persisted and then freed.
    fn finish_close(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        fx.push(Effect::DeleteSessionTempFiles {
            session_id: session_id.to_owned(),
        });
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.status = SessionStatus::Closed;
            session.active_turn = false;
            session.open_turn_id = None;
            session.replaying = Some(None);
            // A closed chat can never run its armed switch.
            session.rest.insert("switching".into(), Value::Null);
        }
        self.prepared_sessions.retain(|_, id| id != session_id);
        self.pending_permissions
            .retain(|_, pending| pending.session_id != session_id);
        self.pending_questions
            .retain(|_, pending| pending.session_id != session_id);
        self.pending_elicitations
            .retain(|_, pending| pending.session_id != session_id);
        self.prompt_queues.remove(session_id);
        self.suppress_queue_flush.remove(session_id);
        if self.sessions.contains_key(session_id) && self.messages.contains_key(session_id) {
            fx.push(Effect::PersistSession {
                session_id: session_id.to_owned(),
            });
            fx.push(Effect::DropSessionTranscript {
                session_id: session_id.to_owned(),
            });
        } else {
            self.drop_session_transcript(session_id);
        }
    }

    /// `deleteHistorySession`: the agent forgets the session when it can,
    /// then the durable history goes, and only then the chat.
    pub(crate) fn delete_history_session(&mut self, session_id: String, fx: &mut Vec<Effect>) {
        self.book
            .host_reports
            .retain(|(reported, _), _| *reported != session_id);
        let live = self.sessions.get(&session_id);
        let indexed = self
            .session_index
            .iter()
            .find(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id.as_str()));
        let agent_id = match live {
            Some(session) => Some(session.agent_id.clone()),
            None => indexed
                .and_then(|entry| entry.get("agentId"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
        .filter(|id| !id.is_empty());
        let capabilities = agent_id
            .as_ref()
            .and_then(|id| self.agents.get(id))
            .and_then(|agent| agent.get("capabilities"));
        let advertised = capabilities
            .and_then(|caps| caps.get("sessionCapabilities"))
            .and_then(|caps| caps.get("delete"))
            .is_some_and(js::truthy);
        let unknown = live.is_none() && !capabilities.is_some_and(js::truthy);
        match agent_id {
            Some(agent_id) if advertised || unknown => fx.push(Effect::DeleteAgentSession {
                agent_id,
                session_id,
            }),
            _ => fx.push(Effect::DeleteSessionPayload { session_id }),
        }
    }

    /// The durable history is gone: drop the chat everywhere.
    fn finish_delete(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.status = SessionStatus::Closed;
            session.active_turn = false;
            session.open_turn_id = None;
            session.replaying = Some(None);
            // A deleted chat can never be retried.
            session
                .rest
                .insert("launchFirstTurnBlocks".into(), Value::Null);
        }
        self.session_index
            .retain(|entry| entry.get("id").and_then(Value::as_str) != Some(session_id));
        self.prepared_sessions.retain(|_, id| id != session_id);
        // The cached payload goes with the history it mirrored.
        self.book.durable_history.remove(session_id);
        self.drop_session_transcript(session_id);
        fx.push(Effect::CloseChatTab {
            session_id: session_id.to_owned(),
        });
        fx.push(Effect::DeleteSessionTempFiles {
            session_id: session_id.to_owned(),
        });
    }

    /// The answer to a close or delete command; `failure` is the error's
    /// message.
    pub(crate) fn on_session_command(
        &mut self,
        command: Effect,
        failure: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        match command {
            Effect::CloseAgentSession { session_id, .. } => {
                if let Some(message) = failure {
                    fx.push(Effect::warn(
                        "acp.closeSession",
                        format!("Failed to close session {session_id}: Error: {message}"),
                    ));
                }
                self.finish_close(&session_id, fx);
            }
            Effect::DeleteAgentSession { session_id, .. } => {
                if let Some(message) = failure {
                    fx.push(Effect::warn(
                        "acp.deleteHistorySession",
                        format!("session/delete failed for {session_id}: {message}"),
                    ));
                }
                fx.push(Effect::DeleteSessionPayload { session_id });
            }
            Effect::DeleteSessionPayload { session_id } => match failure {
                None => self.finish_delete(&session_id, fx),
                Some(_) => fx.push(Effect::warn(
                    "acp.deleteHistorySession",
                    "failed to delete session history".into(),
                )),
            },
            _ => {}
        }
    }
}
