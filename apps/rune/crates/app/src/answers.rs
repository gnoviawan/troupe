//! The user's answers to the agent's prompts on their way to the host: the model drops the
//! prompt and asks for the command (`Effect::Answer`); a refusal hands it back. Also the
//! subagent panel's Stop (`useStopSubagent`), which React sends outside the store too.

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

    /// `stop_subagent` for the record `subagent_id` of the chat `session_id`; the agent confirms
    /// by ending the record as cancelled.
    pub(crate) fn stop_subagent(&mut self, session_id: String, subagent_id: String) {
        let agent_id = self.chat.acp.sessions.get(&session_id).map(|s| s.agent_id.clone()).filter(|a| !a.is_empty());
        let Some(agent_id) = agent_id.filter(|_| !subagent_id.is_empty()) else {
            log::warn!("[chat] stop of a subagent without an agent or id in chat {session_id}");
            return;
        };
        log::info!("[chat] stopping subagent {} in chat {session_id}", subagent_id.chars().take(128).collect::<String>());
        self.chat.reader.set_stopping(&subagent_id, true);
        let payload = json!({ "agentId": agent_id, "rootSessionId": session_id, "subagentId": subagent_id });
        self.call(WsRequestType::StopSubagent, payload, Pending::StopSubagent(subagent_id));
    }

    /// `respondBrowserConsent`: send exactly the option the user pressed for the host's ticket.
    /// The card leaves at once; a refusal other than a stale ticket brings it back.
    pub(crate) fn respond_browser_consent(&mut self, request_id: String, option_id: String) {
        let Some(ticket) = self.chat.acp.pending_browser_consents.remove(&request_id) else { return };
        let session = ticket.get("sessionId").and_then(Value::as_str).unwrap_or_default();
        let agent_id = self.chat.acp.sessions.get(session).map(|s| s.agent_id.clone()).unwrap_or_default();
        log::info!("[chat] browser consent {option_id} for {request_id}");
        let payload = json!({ "agentId": agent_id, "requestId": request_id, "optionId": option_id });
        self.call(WsRequestType::RespondPermission, payload, Pending::BrowserConsent(Box::new(ticket)));
    }

    /// The host's reply to a consent answer (`classify`): a stale ticket stays gone, a too-early
    /// or refused answer restores the card and says why.
    pub(crate) fn browser_consent_settled(&mut self, ticket: Value, result: Result<Option<Value>, WsError>) {
        let Err(e) = result else { return };
        let request_id = ticket.get("requestId").and_then(Value::as_str).unwrap_or_default().to_owned();
        log::warn!("[chat] browser consent answer refused: requestId={request_id} code={}", e.code);
        let message = match e.code.as_str() {
            "TICKET_NOT_FOUND" | "TICKET_ALREADY_RESOLVED" | "stale" | "already_resolved" => return,
            "TICKET_TOO_EARLY" | "rate_limited" => "That was too quick. Check the request, then choose again.",
            _ => "Could not send the browser consent answer. Try again.",
        };
        self.chat.acp.pending_browser_consents.entry(request_id).or_insert(ticket);
        self.toast_error(message, "");
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

    /// The question card's keys (`QuestionStepper`): a digit picks that option, Enter sends the
    /// picks. Only while the chat on screen shows a question; the card's own Escape cancels it.
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
            Key::Enter => {
                let multi = ui::chat::approvals::question_is_multi(&question);
                let values = self.chat.reader.picks.answer(&id, multi);
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
