//! What the one-shots read from their agent's reply
//! (`parseGeneratedCommitMessage`, `parseGeneratedBugReportDraft`), and the
//! state a temporary session leaves (`dropEphemeralSessionState`).

use serde_json::{Value, json};

use super::{State, js};

/// `parseGeneratedCommitMessage` and `parseGeneratedBugReportDraft` share
/// this: an optional ```` ```json ```` fence, then one JSON object.
fn parse_object(raw: &str, invalid: &str) -> Result<serde_json::Map<String, Value>, String> {
    let trimmed = crate::js_trim(raw);
    let fenced = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
        .map(|body| {
            let body = body
                .get(..4)
                .filter(|head| head.eq_ignore_ascii_case("json"))
                .map_or(body, |_| &body[4..]);
            crate::js_trim(body)
        });
    let text = fenced.unwrap_or(trimmed);
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(record)) => Ok(record),
        _ => Err(format!("Error: {invalid}")),
    }
}

fn single_line(text: &str, max: usize) -> bool {
    !text.contains(['\r', '\n']) && js::len(text) <= max
}

pub(crate) fn parse_commit_message(raw: &str) -> Result<Value, String> {
    let record = parse_object(
        raw,
        "The ACP agent returned an invalid commit message response",
    )?;
    let summary = record
        .get("summary")
        .and_then(Value::as_str)
        .map(crate::js_trim)
        .filter(|summary| !summary.is_empty())
        .ok_or("Error: The ACP agent returned a blank commit summary")?;
    if !single_line(summary, 72) {
        return Err("Error: The ACP agent returned a commit summary that is longer than 72 characters or contains a newline".into());
    }
    let description = match record.get("description") {
        None => "",
        Some(Value::String(text)) => crate::js_trim(text),
        Some(_) => {
            return Err("Error: The ACP agent returned an invalid commit description".into());
        }
    };
    Ok(json!({ "summary": summary, "description": description }))
}

pub(crate) fn parse_bug_report(raw: &str) -> Result<Value, String> {
    let record = parse_object(
        raw,
        "The ACP agent returned an invalid bug report draft response",
    )?;
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .map(crate::js_trim)
            .filter(|text| !text.is_empty())
    };
    let title = text("title").ok_or("Error: The ACP agent returned a blank bug report title")?;
    if !single_line(title, 120) {
        return Err("Error: The ACP agent returned a bug report title that is longer than 120 characters or contains a newline".into());
    }
    let body = text("body").ok_or("Error: The ACP agent returned a blank bug report body")?;
    Ok(json!({ "title": title, "body": body }))
}

impl State {
    /// `dropEphemeralSessionState`.
    pub(crate) fn drop_ephemeral_session(&mut self, session_id: &str) {
        self.book.ephemeral_sessions.remove(session_id);
        self.sessions.remove(session_id);
        self.messages.remove(session_id);
        self.tool_calls.remove(session_id);
        self.agent_switches.remove(session_id);
        self.plans.remove(session_id);
        self.commands.remove(session_id);
        self.commands_seq.remove(session_id);
        self.session_usage.remove(session_id);
        self.pending_permissions
            .retain(|_, p| p.session_id != session_id);
        self.pending_questions
            .retain(|_, q| q.session_id != session_id);
        self.pending_elicitations
            .retain(|_, e| e.session_id != session_id);
        self.prompt_queues.remove(session_id);
        self.suppress_queue_flush.remove(session_id);
        if self.active_session_id.as_deref() == Some(session_id) {
            self.active_session_id = None;
        }
    }
}
