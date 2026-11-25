//! Usage of an Antigravity Account, and the plan-limit message Antigravity
//! sends as plain agent text ("Usage Limit Reached") instead of a structured
//! error.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One usage window of an Account: one quota bucket the agent's `/usage`
/// reported with a percent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityUsageWindow {
    /// Always `model`.
    pub kind: String,
    /// Group and bucket, e.g. "Gemini Models · Weekly Limit Remaining".
    pub label: String,
    pub model_id: String,
    /// Percent used, 0–100.
    pub percent: f64,
    /// ISO-8601 reset time; `None` when the reply named none in its usual format.
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntigravityUsageStatus {
    /// The agent's `/usage` reply had at least one quota meter.
    Ok,
    /// The agent's `/usage` reply had no meter; `message` holds its lines.
    Unavailable,
    /// The Account has no Google login.
    SignedOut,
    /// The check did not finish; see `message`.
    Error,
    /// Nobody has asked for a check since the host started.
    NotChecked,
}

/// The usage of one Account at `checked_at`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityAccountUsage {
    pub account_id: String,
    pub status: AntigravityUsageStatus,
    /// Plan / tier name when Antigravity sent one. Never an email.
    pub plan_type: Option<String>,
    pub windows: Vec<AntigravityUsageWindow>,
    /// Unix epoch milliseconds of the check; `0` when `not_checked`.
    pub checked_at: i64,
    /// The agent's lines the meters cannot show, or why the check failed. No
    /// login details.
    pub message: Option<String>,
}

impl AntigravityAccountUsage {
    #[must_use]
    pub fn status_only(
        account_id: &str,
        status: AntigravityUsageStatus,
        message: impl Into<String>,
        checked_at: i64,
    ) -> Self {
        Self {
            account_id: account_id.to_string(),
            status,
            plan_type: None,
            windows: Vec::new(),
            checked_at,
            message: Some(message.into()),
        }
    }
}

/// The plan limit Antigravity reported in the text of a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntigravityQuotaHit {
    /// Epoch milliseconds of the reset, when the message named one.
    pub reset_at: Option<i64>,
}

const QUOTA_TITLE: &str = "Usage Limit Reached";
const QUOTA_BODY: &str = "You have reached your current quota for this period.";

/// Read the "Usage Limit Reached" message Antigravity sends as agent text
/// when the account's quota is used up. `None` for any other text.
#[must_use]
pub fn antigravity_quota_message(text: &str) -> Option<AntigravityQuotaHit> {
    antigravity_quota_message_at(text, chrono::Utc::now().timestamp_millis())
}

/// [`antigravity_quota_message`] with `now_ms` as the clock for a relative
/// ("reset in …") reset.
#[must_use]
pub fn antigravity_quota_message_at(text: &str, now_ms: i64) -> Option<AntigravityQuotaHit> {
    let text = text.trim();
    if !text.starts_with(QUOTA_TITLE) || !text.contains(QUOTA_BODY) {
        return None;
    }
    let reset_at = if let Some(rest) = after(text, "reset in ") {
        relative_reset(sentence(rest)).map(|ms| now_ms.saturating_add(ms))
    } else if let Some(rest) = after(text, "reset on ") {
        absolute_reset(sentence(rest))
    } else {
        None
    };
    Some(AntigravityQuotaHit { reset_at })
}

fn after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.find(marker).map(|at| &text[at + marker.len()..])
}

/// The text up to the end of its sentence (a period followed by whitespace,
/// or the last character).
fn sentence(text: &str) -> &str {
    let text = text.trim();
    let end = text
        .char_indices()
        .find(|(i, c)| *c == '.' && text[i + 1..].chars().next().is_none_or(char::is_whitespace))
        .map_or(text.len(), |(i, _)| i);
    text[..end].trim()
}

/// "2 hours, 30 minutes" → milliseconds. Any part that is not a count and a
/// known unit makes the whole duration unknown.
fn relative_reset(text: &str) -> Option<i64> {
    let mut total: i64 = 0;
    let mut parts = 0;
    for part in text
        .split(',')
        .flat_map(|part| part.split(" and "))
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let mut words = part.split_whitespace();
        let count: i64 = words.next()?.parse().ok()?;
        let unit = words.next()?.to_ascii_lowercase();
        if words.next().is_some() {
            return None;
        }
        let unit_ms: i64 = match unit.trim_end_matches('s') {
            "day" => 86_400_000,
            "hour" | "hr" => 3_600_000,
            "minute" | "min" => 60_000,
            "second" | "sec" => 1_000,
            _ => return None,
        };
        total = total.checked_add(count.checked_mul(unit_ms)?)?;
        parts += 1;
    }
    (parts > 0).then_some(total)
}

/// "Jan 5, 2026 14:30 UTC" → epoch milliseconds.
fn absolute_reset(text: &str) -> Option<i64> {
    let text = text.trim().strip_suffix("UTC")?.trim();
    chrono::NaiveDateTime::parse_from_str(text, "%b %d, %Y %H:%M")
        .ok()
        .map(|at| at.and_utc().timestamp_millis())
}

/// The plan limit an `agent_error` reports for an Antigravity turn:
/// `data.agent == "antigravity"` and a quota `errorKind`. Returns the kind
/// and the reset time (epoch milliseconds) when the error carries one.
#[must_use]
pub fn antigravity_plan_limit(data: &Value) -> Option<(&'static str, Option<i64>)> {
    if data.get("agent").and_then(Value::as_str) != Some(PLAN_LIMIT_AGENT) {
        return None;
    }
    let kind = match data.get("errorKind").and_then(Value::as_str)? {
        "quota_exhausted" => "quota_exhausted",
        "rate_limited" => "rate_limited",
        _ => return None,
    };
    Some((kind, data.get("resetsAt").and_then(Value::as_i64)))
}

/// The `data.agent` value of an Antigravity plan-limit error.
pub const PLAN_LIMIT_AGENT: &str = "antigravity";

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
