//! Usage of an Antigravity Account: one window per model quota bucket, read
//! from the Cloud Code Assist `fetchAvailableModels` reply. The probe is
//! best-effort: a missing login file, an endpoint this host does not
//! understand, or a reply without quota data is `unavailable`, not an error.
//!
//! This module also reads the plan-limit message Antigravity sends as plain
//! agent text ("Usage Limit Reached") instead of a structured error.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The Cloud Code endpoint for accounts on the Google Cloud terms of service.
pub const GCP_TOS_ENDPOINT: &str = "https://cloudcode-pa.googleapis.com";
/// The Cloud Code endpoint for every other account.
pub const DAILY_ENDPOINT: &str = "https://daily-cloudcode-pa.googleapis.com";

/// One usage window of an Account: the quota bucket of one model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityUsageWindow {
    /// Always `model`.
    pub kind: String,
    /// Model display name, e.g. "Gemini 3 Pro".
    pub label: String,
    pub model_id: String,
    /// Percent used, 0–100.
    pub percent: f64,
    /// ISO-8601 reset time; `None` when Antigravity did not send one.
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntigravityUsageStatus {
    /// Windows were read.
    Ok,
    /// No usable quota read on this host (keychain login, unknown reply).
    Unavailable,
    /// The Account has no Google login.
    SignedOut,
    /// The usage could not be read; see `message`.
    Error,
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
    /// Unix epoch milliseconds of the read.
    pub checked_at: i64,
    /// Human-readable reason for `error` or `unavailable`. No login details.
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

/// What one usage probe concluded, before it is stored.
#[derive(Debug)]
pub(crate) enum ProbeVerdict {
    /// The `fetchAvailableModels` reply and the plan name from `loadCodeAssist`.
    Models {
        plan_type: Option<String>,
        reply: Value,
    },
    SignedOut,
    Unavailable(&'static str),
    Failed(&'static str),
}

/// The default `unavailable` message.
pub(crate) const USAGE_UNAVAILABLE: &str = "Antigravity usage is not available";
/// The `unavailable` message of a built-in Account whose login is in the
/// macOS keychain, where Troupe cannot read the refresh token.
pub(crate) const KEYCHAIN_UNAVAILABLE: &str =
    "Antigravity usage is not available for a keychain login";
const NO_LIMITS: &str = "Antigravity did not report plan limits";

/// Map a probe verdict to the usage record of one Account.
#[must_use]
pub(crate) fn usage_from_verdict(
    account_id: &str,
    verdict: ProbeVerdict,
    checked_at: i64,
) -> AntigravityAccountUsage {
    match verdict {
        ProbeVerdict::Models { plan_type, reply } => {
            usage_from_models(account_id, plan_type, &reply, checked_at)
        }
        ProbeVerdict::SignedOut => AntigravityAccountUsage {
            account_id: account_id.to_string(),
            status: AntigravityUsageStatus::SignedOut,
            plan_type: None,
            windows: Vec::new(),
            checked_at,
            message: None,
        },
        ProbeVerdict::Unavailable(message) => AntigravityAccountUsage::status_only(
            account_id,
            AntigravityUsageStatus::Unavailable,
            message,
            checked_at,
        ),
        ProbeVerdict::Failed(message) => AntigravityAccountUsage::status_only(
            account_id,
            AntigravityUsageStatus::Error,
            message,
            checked_at,
        ),
    }
}

/// Map a `fetchAvailableModels` reply to one window per model with quota
/// data. Disabled and internal models are skipped. Windows are sorted by
/// percent used (most used first), then by label. No window is `unavailable`.
#[must_use]
pub fn usage_from_models(
    account_id: &str,
    plan_type: Option<String>,
    reply: &Value,
    checked_at: i64,
) -> AntigravityAccountUsage {
    let mut windows: Vec<AntigravityUsageWindow> = reply
        .get("models")
        .and_then(Value::as_object)
        .map(|models| {
            models
                .iter()
                .filter_map(|(id, model)| window_from(id, model))
                .collect()
        })
        .unwrap_or_default();
    if windows.is_empty() {
        return AntigravityAccountUsage::status_only(
            account_id,
            AntigravityUsageStatus::Unavailable,
            NO_LIMITS,
            checked_at,
        );
    }
    windows.sort_by(|a, b| {
        b.percent
            .total_cmp(&a.percent)
            .then_with(|| a.label.cmp(&b.label))
    });
    AntigravityAccountUsage {
        account_id: account_id.to_string(),
        status: AntigravityUsageStatus::Ok,
        plan_type,
        windows,
        checked_at,
        message: None,
    }
}

fn window_from(model_id: &str, model: &Value) -> Option<AntigravityUsageWindow> {
    let flag = |key: &str| model.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("disabled") || flag("isInternal") {
        return None;
    }
    let quota = model.get("quotaInfo").filter(|quota| quota.is_object())?;
    let resets_at = quota
        .get("resetTime")
        .and_then(Value::as_str)
        .filter(|reset| !reset.is_empty())
        .map(str::to_string);
    let remaining = quota.get("remainingFraction").and_then(Value::as_f64);
    let percent = match (remaining, &resets_at) {
        (Some(fraction), _) => ((1.0 - fraction) * 100.0).clamp(0.0, 100.0),
        // A bucket with a reset time and no remaining fraction is used up.
        (None, Some(_)) => 100.0,
        (None, None) => return None,
    };
    let label = model
        .get("displayName")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(model_id)
        .to_string();
    Some(AntigravityUsageWindow {
        kind: "model".to_string(),
        label,
        model_id: model_id.to_string(),
        percent,
        resets_at,
    })
}

/// The Cloud Code Assist project of a `loadCodeAssist` reply: a string, or
/// an object with an `id`.
#[must_use]
pub(crate) fn code_assist_project(load: &Value) -> Option<String> {
    let project = load.get("cloudaicompanionProject")?;
    project
        .as_str()
        .or_else(|| project.get("id").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// The plan name of a `loadCodeAssist` reply: the paid tier, else the
/// current tier, by name, else by id.
#[must_use]
pub(crate) fn plan_name(load: &Value) -> Option<String> {
    let tier_name = |tier: &str| {
        let tier = load.get(tier)?;
        ["name", "id"].iter().find_map(|key| {
            tier.get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
    };
    tier_name("paidTier").or_else(|| tier_name("currentTier"))
}

/// The endpoint `fetchAvailableModels` must go to for this account.
#[must_use]
pub(crate) fn quota_endpoint(load: &Value) -> &'static str {
    let gcp_tos = load
        .get("paidTier")
        .and_then(|tier| tier.get("usesGcpTos"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if gcp_tos {
        GCP_TOS_ENDPOINT
    } else {
        DAILY_ENDPOINT
    }
}

/// A token-endpoint error body that means the login is gone.
#[must_use]
pub(crate) fn is_invalid_grant(body: &Value) -> bool {
    body.get("error").and_then(Value::as_str) == Some("invalid_grant")
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
