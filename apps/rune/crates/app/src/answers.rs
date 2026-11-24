//! The user's answers to the agent's prompts on their way to the host: the model drops the
//! prompt and asks for the command (`Effect::Answer`); a refusal hands it back.

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
}
