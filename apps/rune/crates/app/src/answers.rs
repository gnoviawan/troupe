//! The user's answers to the agent's prompts on their way to the host: the model drops the
//! prompt and asks for the command (`Effect::Answer`); a refusal hands it back. Also the
//! subagent panel's Stop (`useStopSubagent`), which React sends outside the store too.

use model::acp::Event;
use model::acp::answer::Answer;
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use crate::State;
use crate::sync::Pending;

impl State {
    /// `respond_permission`, `answer_question` or `respond_elicitation`, ledgered.
    pub(crate) fn send_answer(&mut self, answer: Answer) {
        let agent_id = answer.agent_id().to_owned();
        let (kind, payload) = match &answer {
            Answer::Permission { pending, option_id } => (
                WsRequestType::RespondPermission,
                json!({ "agentId": agent_id, "requestId": pending.request_id, "optionId": option_id }),
            ),
            Answer::Question { pending, values } => (
                WsRequestType::AnswerQuestion,
                json!({ "agentId": agent_id, "questionId": pending.question_id, "values": (!values.is_empty()).then_some(values) }),
            ),
            Answer::Elicitation { pending, action, content } => (
                WsRequestType::RespondElicitation,
                json!({ "agentId": agent_id, "requestId": pending.request_id, "action": action, "content": content }),
            ),
        };
        log::info!("[chat] send {kind:?} to {agent_id}");
        self.command(kind, payload, Pending::Answer(Box::new(answer)));
    }

    /// The host's reply to an answer: a refusal puts the prompt back and says so.
    pub(crate) fn answer_settled(&mut self, answer: Answer, result: Result<Option<Value>, WsError>) {
        let Err(e) = result else { return };
        log::warn!("[chat] an answer was refused: {}: {}", e.code, e.message);
        let title = match &answer {
            Answer::Permission { .. } => Some("Could not send the permission response. Try again."),
            Answer::Question { .. } => Some("Could not send your answer. Try again."),
            Answer::Elicitation { .. } => None,
        };
        let resolved = e.code == "already_resolved";
        self.acp_step(Event::AnswerFailed(answer, e.message.clone(), Some(e.code.clone())));
        if let Some(title) = title.filter(|_| !resolved) {
            self.toast_error(title, "");
        }
    }

    /// `stop_subagent` for the record `subagent_id` of the chat `session_id`; the agent confirms
    /// by ending the record as cancelled.
    pub(crate) fn stop_subagent(&mut self, session_id: String, subagent_id: String) {
        let Some(agent_id) = self.chat.acp.sessions.get(&session_id).map(|s| s.agent_id.clone()) else {
            log::warn!("[chat] stop of a subagent in an unknown chat {session_id}");
            return;
        };
        log::info!("[chat] stopping subagent {} in chat {session_id}", subagent_id.chars().take(128).collect::<String>());
        self.chat.reader.set_stopping(&subagent_id, true);
        let payload = json!({ "agentId": agent_id, "rootSessionId": session_id, "subagentId": subagent_id });
        self.call(WsRequestType::StopSubagent, payload, Pending::StopSubagent(subagent_id));
    }

    /// The host's answer to a stop: `already_ended` frees the button, a refusal says why.
    pub(crate) fn subagent_stop_settled(&mut self, subagent_id: String, result: Result<Option<Value>, WsError>) {
        match result {
            Ok(reply) => {
                if reply.as_ref().and_then(|r| r.get("outcome")).and_then(Value::as_str) == Some("already_ended") {
                    self.chat.reader.set_stopping(&subagent_id, false);
                }
            }
            Err(e) => {
                log::warn!("[chat] stop of subagent {subagent_id} failed: {}: {}", e.code, e.message);
                self.chat.reader.set_stopping(&subagent_id, false);
                self.toast_error("Couldn't stop the subagent", &e.message);
            }
        }
    }
}
