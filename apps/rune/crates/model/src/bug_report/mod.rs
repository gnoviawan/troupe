//! The Report a Bug dialog's report (`lib/bug-report-api.ts`): the exact markdown the dialog
//! previews and sends, the prefilled `issues/new` fallback URL, and the one line a fallback
//! toast says about why. The diagnostics bundle comes from the host already redacted; nothing
//! here logs the title, the body or the diagnostics.

use serde::{Deserialize, Serialize};

use crate::js_trim;
use crate::setup::encode_uri_component;

/// The repository the report files against (`ISSUE_NEW_URL`).
pub const ISSUE_NEW_URL: &str = "https://github.com/gnoviawan/troupe/issues/new";

/// The longest prefill URL before the body falls back to the short one (`ISSUE_URL_MAX_CHARS`).
pub const ISSUE_URL_MAX_CHARS: usize = 8000;

/// The host's diagnostics (`BugReportContext`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub channel: String,
    pub run_id: String,
    pub collected_at: String,
    pub log_available: bool,
    pub log_truncated: bool,
    pub log_tail: String,
}

impl Context {
    /// `DIAGNOSTICS_UNAVAILABLE`: what the report says when the host could not collect.
    #[must_use]
    pub fn unavailable() -> Context {
        let u = || "unavailable".to_owned();
        Context {
            app_version: u(),
            os: u(),
            arch: u(),
            channel: u(),
            run_id: u(),
            collected_at: u(),
            log_available: false,
            log_truncated: false,
            log_tail: String::new(),
        }
    }

    fn environment(&self) -> [String; 4] {
        [
            format!("- **Troupe:** {} ({})", self.app_version, self.channel),
            format!("- **OS:** {} ({})", self.os, self.arch),
            format!("- **Run ID:** {}", self.run_id),
            format!("- **Collected:** {}", self.collected_at),
        ]
    }
}

/// `buildIssueMarkdown`: title, description, environment, the log tail in a fence no line of
/// it can close, and the screenshots note.
#[must_use]
pub fn markdown(context: &Context, title: &str, description: &str, screenshots: usize) -> String {
    let (title, description) = (js_trim(title), js_trim(description));
    let mut lines: Vec<String> = vec![
        format!("## {}", if title.is_empty() { "Bug report" } else { title }),
        String::new(),
        if description.is_empty() { "_No description provided._".to_owned() } else { description.to_owned() },
        String::new(),
        "### Environment".to_owned(),
        String::new(),
    ];
    lines.extend(context.environment());
    lines.push(String::new());
    if context.log_available {
        let longest = context.log_tail.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat(3.max(longest + 1));
        let summary = if context.log_truncated { "termul.log tail (truncated to its byte bound)" } else { "termul.log tail" };
        lines.push("<details>".into());
        lines.push(format!("<summary>{summary}</summary>"));
        lines.push(String::new());
        lines.push(format!("{fence}text"));
        lines.push(context.log_tail.clone());
        lines.push(fence);
        lines.push(String::new());
        lines.push("</details>".into());
    } else {
        lines.push("_The host log (termul.log) was unavailable, so no log tail is attached._".into());
    }
    if screenshots > 0 {
        let plural = if screenshots == 1 { "screenshot was" } else { "screenshots were" };
        lines.push(String::new());
        lines.push("### Screenshots".into());
        lines.push(String::new());
        lines.push(format!(
            "{screenshots} {plural} captured for this report. Images cannot be attached during issue creation — copy each screenshot in the report dialog and paste it into this issue."
        ));
    }
    format!("{}\n", lines.join("\n"))
}

/// `shortIssueBody`: where the rest of the report is, and the environment.
fn short_body(context: &Context, copied: bool) -> String {
    let note = if copied {
        "_The full report (description, diagnostics, and log tail) was copied to the clipboard — paste it into this issue to complete the report._"
    } else {
        "_The full report (description, diagnostics, and log tail) could not be copied to the clipboard — copy the report text from the dialog's preview and paste it into this issue._"
    };
    let mut lines = vec![note.to_owned(), String::new(), "### Environment".to_owned(), String::new()];
    lines.extend(context.environment());
    lines.join("\n")
}

/// Halves a title by UTF-16 units, as `slice(0, ceil(length / 2))` does.
fn half(title: &str) -> String {
    let units: Vec<u16> = title.encode_utf16().collect();
    String::from_utf16_lossy(&units[..units.len().div_ceil(2)])
}

/// `buildIssueUrl`: the full body when it fits the budget, else the short body, with the
/// title halved until it fits. No `labels`: filing without triage rights can 404 on it.
#[must_use]
pub fn issue_url(context: &Context, title: &str, body: &str, copied: bool) -> String {
    let url = |t: &str, b: &str| format!("{ISSUE_NEW_URL}?title={}&body={}", encode_uri_component(t), encode_uri_component(b));
    let full = url(title, body);
    if full.encode_utf16().count() <= ISSUE_URL_MAX_CHARS {
        return full;
    }
    let short = short_body(context, copied);
    let mut title = title.to_owned();
    while !title.is_empty() && url(&title, &short).encode_utf16().count() > ISSUE_URL_MAX_CHARS {
        title = half(&title);
    }
    url(&title, &short)
}

/// Why a submit fell back to the clipboard (`fallbackCause`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cause {
    Unavailable,
    Failed,
    Transport,
}

/// One toast-safe line: the host's bounded, already redacted reason, flattened and cut to 160.
#[must_use]
pub fn fallback_cause(cause: Cause, reason: Option<&str>) -> String {
    if cause == Cause::Transport {
        return "The report could not reach the host submit command (renderer transport failure).".into();
    }
    let flat = reason.map(|r| js_trim(&r.split_whitespace().collect::<Vec<_>>().join(" ")).to_owned()).unwrap_or_default();
    if !flat.is_empty() {
        let units: Vec<u16> = flat.encode_utf16().collect();
        return if units.len() > 160 { format!("{}…", String::from_utf16_lossy(&units[..160])) } else { flat };
    }
    if cause == Cause::Failed { "gh issue create failed.".into() } else { "GitHub CLI is unavailable.".into() }
}

/// The host's submit outcome (`BugReportSubmitResult`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum Outcome {
    Submitted { issue_url: String },
    Unavailable { reason: String },
    Failed { reason: String },
}

#[cfg(test)]
mod tests;
