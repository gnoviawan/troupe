//! The user's answers to the prompts that wait on them: `respondPermission`,
//! `answerQuestion` and `respondElicitation` in `slices/prompt.ts`. Each drops
//! its prompt at once, so a double click sends one answer, and returns the
//! socket command as [`Effect::Answer`]. The app hands a failed one back as
//! `answerFailed`, which puts the prompt back unless another client already
//! answered it (`isAlreadyResolved`).
//!
//! Not here: the answering marks a resubscribe reads (`noteAnswering`), which
//! belong to the resubscribe install.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::approvals::{PendingElicitation, PendingPermission, PendingQuestion};
use super::{Effect, State};

/// The host's code for a ticket another connection already answered.
const ALREADY_RESOLVED: &str = "already_resolved";

/// One answer on its way to the host, with the prompt it answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "prompt",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Answer {
    /// `respond_permission`; no option cancels the request.
    Permission {
        pending: PendingPermission,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        option_id: Option<String>,
    },
    /// `answer_question` with the chosen values.
    Question {
        pending: PendingQuestion,
        values: Vec<String>,
    },
    /// `respond_elicitation`: `accept` with the form's content, `decline` or
    /// `cancel`.
    Elicitation {
        pending: PendingElicitation,
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<Value>,
    },
}

impl Answer {
    /// The agent the answer goes to.
    pub fn agent_id(&self) -> &str {
        match self {
            Answer::Permission { pending, .. } => &pending.agent_id,
            Answer::Question { pending, .. } => &pending.agent_id,
            Answer::Elicitation { pending, .. } => &pending.agent_id,
        }
    }
}

impl State {
    pub(crate) fn respond_permission(
        &mut self,
        request_id: &str,
        option_id: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        if let Some(pending) = self.pending_permissions.remove(request_id) {
            fx.push(Effect::Answer(Answer::Permission { pending, option_id }));
        }
    }

    pub(crate) fn answer_question(
        &mut self,
        question_id: &str,
        values: Vec<String>,
        fx: &mut Vec<Effect>,
    ) {
        if let Some(pending) = self.pending_questions.remove(question_id) {
            fx.push(Effect::Answer(Answer::Question { pending, values }));
        }
    }

    pub(crate) fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: String,
        content: Option<Value>,
        fx: &mut Vec<Effect>,
    ) {
        if let Some(pending) = self.pending_elicitations.remove(request_id) {
            fx.push(Effect::Answer(Answer::Elicitation {
                pending,
                action,
                content,
            }));
        }
    }

    /// The host refused the answer: the prompt comes back so the user can
    /// answer again, unless the ticket is gone for good. A refused elicitation
    /// answer is logged; the others reach the user as the card's toast.
    pub(crate) fn on_answer_failed(
        &mut self,
        answer: Answer,
        message: &str,
        code: Option<&str>,
        fx: &mut Vec<Effect>,
    ) {
        let resolved = code == Some(ALREADY_RESOLVED);
        match answer {
            Answer::Permission { .. } | Answer::Question { .. } if resolved => {}
            Answer::Permission { pending, .. } => {
                self.pending_permissions
                    .insert(pending.request_id.clone(), pending);
            }
            Answer::Question { pending, .. } => {
                self.pending_questions
                    .insert(pending.question_id.clone(), pending);
            }
            Answer::Elicitation { pending, .. } => {
                fx.push(Effect::Log {
                    level: crate::Level::Warn,
                    source: "acp.respondElicitation".into(),
                    message: format!(
                        "Elicitation response failed for {}: {message}",
                        pending.request_id
                    ),
                });
                self.pending_elicitations
                    .insert(pending.request_id.clone(), pending);
            }
        }
    }
}
