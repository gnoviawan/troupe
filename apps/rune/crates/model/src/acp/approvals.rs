//! Prompts that wait on the user: permission requests, `ask_user`
//! questions, elicitations, and the host's browser consent tickets. Port of
//! the request handlers in `slices/prompt.ts`, of `child-approvals.ts`, and
//! of `parseBrowserConsentTicket` in `lib/browser-consent-api.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::subagent::{Reader, SubagentState};
use super::types::Object;
use super::{Effect, State, clamp, js};

const HOST_TICKET_PREFIX: &str = "host:";
const OPTION_IDS: [&str; 3] = ["allow_session", "allow_project", "deny"];
const ACTIONS: [&str; 7] = [
    "navigate",
    "local_network",
    "password_submit",
    "payment_submit",
    "upload",
    "download",
    "tainted_exfil",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPermission {
    pub request_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub options: Value,
    /// The call the request is about, clamped; `null` when it could not be kept.
    pub tool_call: Option<Object>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestion {
    pub question_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub question: Value,
    pub options: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_free_text: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_select: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingElicitation {
    pub request_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub mode: Value,
    pub message: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub fields: Vec<Object>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_other: Option<bool>,
}

/// Something that raised a prompt in a session, as an agent.
trait Raised {
    fn session_id(&self) -> &str;
    fn agent_id(&self) -> &str;
}

macro_rules! raised {
    ($($kind:ty),*) => {$(
        impl Raised for $kind {
            fn session_id(&self) -> &str {
                &self.session_id
            }
            fn agent_id(&self) -> &str {
                &self.agent_id
            }
        }
    )*};
}
raised!(PendingPermission, PendingQuestion, PendingElicitation);

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestEvent {
    #[serde(default)]
    pub agent_id: String,
    pub session_id: String,
    pub request_id: String,
    #[serde(default)]
    pub tool_call: Option<Object>,
    #[serde(default)]
    pub options: Value,
    #[serde(rename = "_meta", default)]
    pub meta: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequestEvent {
    #[serde(default)]
    pub agent_id: String,
    pub session_id: String,
    pub question_id: String,
    #[serde(default)]
    pub question: Value,
    #[serde(default)]
    pub options: Value,
    #[serde(default)]
    pub allow_free_text: Option<bool>,
    #[serde(default)]
    pub multi_select: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionCancelledEvent {
    pub question_id: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationRequestEvent {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub session_id: String,
    pub request_id: String,
    #[serde(default)]
    pub mode: Value,
    #[serde(default)]
    pub message: Value,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<Object>>,
    #[serde(default)]
    pub allow_other: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTicketClosedEvent {
    #[serde(default)]
    pub request_id: Value,
}

/// `parseBrowserConsentTicket`: a host-originated browser consent ticket
/// riding the permission request contract, or `None`.
fn browser_ticket(event: &PermissionRequestEvent) -> Option<Value> {
    if !event.request_id.starts_with(HOST_TICKET_PREFIX) || event.session_id.is_empty() {
        return None;
    }
    let browser = event
        .meta
        .as_ref()?
        .get("termul")
        .filter(|termul| termul.get("origin_kind").and_then(Value::as_str) == Some("browser"))?
        .get("browser")
        .filter(|browser| browser.is_object() || browser.is_array())?;
    let origin = browser.get("origin")?.as_str()?;
    let target_url = browser.get("targetUrl")?.as_str()?;
    let action = browser
        .get("action")
        .and_then(Value::as_str)
        .filter(|action| ACTIONS.contains(action))
        .unwrap_or("navigate");
    let offered: Vec<&str> = event
        .options
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| option.get("optionId")?.as_str())
        .filter_map(|id| OPTION_IDS.iter().find(|known| **known == id).copied())
        .collect();
    let mut ticket = json!({
        "requestId": event.request_id,
        "sessionId": event.session_id,
        "origin": origin,
        "targetKind": if browser.get("targetKind").and_then(Value::as_str) == Some("file") { "file" } else { "web" },
        "targetUrl": target_url,
        "targetUrlBytes": browser.get("targetUrlBytes").filter(|n| n.is_number()).cloned()
            .unwrap_or_else(|| json!(js::len(target_url))),
        "action": action,
        "profile": if browser.get("profile").and_then(Value::as_str) == Some("ephemeral") { "ephemeral" } else { "global" },
        "options": if offered.is_empty() { OPTION_IDS.to_vec() } else { offered },
    });
    if let Some(name) = browser.get("fileName").and_then(Value::as_str) {
        ticket["fileName"] = json!(name);
    }
    if let Some(element) = browser.get("element").and_then(Value::as_str) {
        ticket["element"] = json!(element);
    }
    let file = browser
        .get("file")
        .filter(|file| file.is_object() || file.is_array());
    if let Some((name, bytes)) = file.and_then(|file| {
        Some((
            file.get("name")?.as_str()?,
            file.get("bytes").filter(|b| b.is_number())?,
        ))
    }) {
        ticket["file"] = json!({ "name": name, "bytes": bytes });
    }
    Some(ticket)
}

/// Drop the prompts `agent_id` raised in `session_id`; how many went.
fn drop_agent_prompts<T: Raised>(
    pending: &mut BTreeMap<String, T>,
    session_id: &str,
    agent_id: &str,
) -> usize {
    let before = pending.len();
    pending
        .retain(|_, prompt| !(prompt.session_id() == session_id && prompt.agent_id() == agent_id));
    before - pending.len()
}

impl State {
    /// `dismissSettledChildApprovals`: the card of an orchestrated run that
    /// settled dismisses the prompts its agent still had up in the session.
    pub(crate) fn dismiss_settled_child_approvals(
        &mut self,
        session_id: &str,
        call: &Object,
        fx: &mut Vec<Effect>,
    ) {
        let record = Reader {
            book: &mut self.book,
            fx,
        }
        .subagent(call);
        let Some(record) = record else {
            return;
        };
        let (true, SubagentState::Idle { .. }, Some(agent)) = (
            record.source == "orchestrated",
            &record.state,
            &record.agent,
        ) else {
            return;
        };
        let agent_id = agent.agent_id.as_str();
        let permissions = drop_agent_prompts(&mut self.pending_permissions, session_id, agent_id);
        let questions = drop_agent_prompts(&mut self.pending_questions, session_id, agent_id);
        let elicitations = drop_agent_prompts(&mut self.pending_elicitations, session_id, agent_id);
        if permissions + questions + elicitations == 0 {
            return;
        }
        fx.push(Effect::info(
            "acp.childApprovals",
            format!(
                "Dismissed {permissions} approval(s), {questions} question(s) and {elicitations} elicitation(s) of a settled orchestrated subagent"
            ),
        ));
    }

    pub(crate) fn on_permission_request(
        &mut self,
        event: PermissionRequestEvent,
        event_seq: Option<f64>,
        fx: &mut Vec<Effect>,
    ) {
        if self.book.history_covered(&event.session_id, event_seq) {
            return;
        }
        if let Some(ticket) = browser_ticket(&event) {
            self.on_browser_consent_ticket(ticket, fx);
            return;
        }
        if self.pending_permissions.contains_key(&event.request_id) {
            return;
        }
        let tool_call = event
            .tool_call
            .and_then(|call| clamp::tool_call(&mut self.book, fx, &event.session_id, call));
        self.pending_permissions.insert(
            event.request_id.clone(),
            PendingPermission {
                request_id: event.request_id,
                agent_id: event.agent_id,
                session_id: event.session_id,
                options: event.options,
                tool_call,
            },
        );
    }

    /// `_onBrowserConsentTicket`: hold the ticket and ask the workspace to
    /// bring the session's chat tab forward if another tab hides it.
    fn on_browser_consent_ticket(&mut self, ticket: Value, fx: &mut Vec<Effect>) {
        let request_id = ticket["requestId"].as_str().unwrap_or_default().to_owned();
        let session_id = ticket["sessionId"].as_str().unwrap_or_default().to_owned();
        self.pending_browser_consents.insert(request_id, ticket);
        fx.push(Effect::RevealChatTab { session_id });
    }

    pub(crate) fn on_browser_ticket_closed(&mut self, event: BrowserTicketClosedEvent) {
        if let Some(request_id) = event.request_id.as_str() {
            self.pending_browser_consents.remove(request_id);
        }
    }

    pub(crate) fn on_question_request(
        &mut self,
        event: QuestionRequestEvent,
        event_seq: Option<f64>,
    ) {
        if self.book.history_covered(&event.session_id, event_seq)
            || self.pending_questions.contains_key(&event.question_id)
        {
            return;
        }
        self.pending_questions.insert(
            event.question_id.clone(),
            PendingQuestion {
                question_id: event.question_id,
                agent_id: event.agent_id,
                session_id: event.session_id,
                question: event.question,
                options: event.options,
                allow_free_text: event.allow_free_text,
                multi_select: event.multi_select,
            },
        );
    }

    pub(crate) fn on_question_cancelled(&mut self, event: QuestionCancelledEvent) {
        self.pending_questions.remove(&event.question_id);
    }

    pub(crate) fn on_elicitation_request(
        &mut self,
        event: ElicitationRequestEvent,
        event_seq: Option<f64>,
    ) {
        if (!event.session_id.is_empty() && self.book.history_covered(&event.session_id, event_seq))
            || self.pending_elicitations.contains_key(&event.request_id)
        {
            return;
        }
        // An older host sends options as strings; the contract is objects.
        let fields = event
            .fields
            .unwrap_or_default()
            .into_iter()
            .map(|mut field| {
                let options: Vec<Value> = field
                    .get("options")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|option| match option {
                        Value::String(text) => json!({ "value": text, "label": text }),
                        other => other,
                    })
                    .collect();
                field.insert("options".into(), Value::Array(options));
                field
            })
            .collect();
        self.pending_elicitations.insert(
            event.request_id.clone(),
            PendingElicitation {
                request_id: event.request_id,
                agent_id: event.agent_id,
                session_id: event.session_id,
                mode: event.mode,
                message: event.message,
                url: event.url,
                fields,
                allow_other: event.allow_other,
            },
        );
    }
}
