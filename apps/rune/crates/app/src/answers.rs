//! The user's answers to the agent's prompts on their way to the host: the model drops the
//! prompt and asks for the command (`Effect::Answer`); a refusal hands it back.

use model::acp::Event;
use rune::{Key, KeyEvent};
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

    /// The question card's keys (`QuestionStepper`): a digit picks that option, Enter sends the
    /// picks, Escape cancels. Only while the chat on screen shows a question.
    pub(crate) fn question_key(&mut self, key: &KeyEvent) -> bool {
        if key.mods != rune::Mods::default() {
            return false;
        }
        let active = model::workspace::query(&self.workspace.store, model::workspace::Query::GetActiveTab);
        let model::workspace::Reply::Tab(Some(model::workspace::Tab::AgentChat { session_id, .. })) = active else {
            return false;
        };
        let Some(question) = self.chat.acp.pending_questions.values().find(|q| q.session_id == session_id).cloned() else {
            return false;
        };
        let id = question.question_id.clone();
        match &key.key {
            Key::Escape => {
                log::info!("[chat] cancel question {id}");
                self.chat.reader.picks.forget(&id);
                self.acp_step(Event::AnswerQuestion(id, Vec::new()));
            }
            Key::Enter => {
                let values = self.chat.reader.picks.of(&id).to_vec();
                if values.is_empty() {
                    return true;
                }
                log::info!("[chat] answer question {id}");
                self.chat.reader.picks.forget(&id);
                self.acp_step(Event::AnswerQuestion(id, values));
            }
            Key::Text(digit) => {
                let Some(n) = digit.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
                    return false;
                };
                let Some(value) = question.options.get(n - 1).and_then(|o| o.get("value")).and_then(Value::as_str) else {
                    return false;
                };
                let multi = ui::chat::approvals::question_is_multi(&question);
                self.chat.reader.picks.toggle(&id, value, multi);
            }
            _ => return false,
        }
        true
    }
}
