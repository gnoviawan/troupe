use crate::acp::antigravity_accounts::usage::{
    AntigravityAccountUsage, AntigravityUsageStatus, AntigravityUsageWindow,
};

const QUOTA_HEADING: &str = "### Quota Limits";
const RESET_FORMAT: &str = "%b %d, %Y, %H:%M UTC";
const MAX_LINE_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QuotaReport {
    Meters {
        windows: Vec<AntigravityUsageWindow>,
        notes: Vec<String>,
    },
    NoMeters(Vec<String>),
}

impl QuotaReport {
    pub(crate) fn into_usage(self, account_id: &str, checked_at: i64) -> AntigravityAccountUsage {
        let (status, windows, notes) = match self {
            Self::Meters { windows, notes } => (AntigravityUsageStatus::Ok, windows, notes),
            Self::NoMeters(notes) => (AntigravityUsageStatus::Unavailable, Vec::new(), notes),
        };
        AntigravityAccountUsage {
            account_id: account_id.to_string(),
            status,
            plan_type: None,
            windows,
            checked_at,
            message: (!notes.is_empty()).then(|| notes.join("\n")),
        }
    }
}

#[must_use]
pub(crate) fn parse_usage_reply(text: &str) -> QuotaReport {
    let Some(section) = quota_section(text) else {
        return QuotaReport::NoMeters(vec![first_line(text)]);
    };
    let mut windows = Vec::new();
    let mut notes = Vec::new();
    let mut group: Option<&str> = None;
    for line in section {
        let indent = line.len() - line.trim_start().len();
        let Some(item) = line.trim().strip_prefix("- ") else {
            continue;
        };
        if item.starts_with('*') && !item.starts_with("**") {
            continue;
        }
        match (indent, bold(item)) {
            (0, Some(Bold::Heading(name))) => group = Some(name),
            (0, Some(Bold::Field(name, value))) => notes.push(format!("{name}: {value}")),
            (0, None) => notes.push(clip(item)),
            (1..=3, Some(Bold::Field(name, value))) => {
                let label = bucket_label(group, name);
                match window(&label, value) {
                    Some(window) => windows.push(window),
                    None => notes.push(clip(&format!("{label}: {value}"))),
                }
            }
            (1..=3, _) => notes.push(clip(&bucket_label(group, item))),
            _ => {}
        }
    }
    if windows.is_empty() {
        if notes.is_empty() {
            notes.push("Antigravity listed no quota limits.".to_string());
        }
        return QuotaReport::NoMeters(notes);
    }
    windows.sort_by(|a, b| b.percent.total_cmp(&a.percent));
    QuotaReport::Meters { windows, notes }
}

fn quota_section(text: &str) -> Option<impl Iterator<Item = &str>> {
    let mut lines = text.lines();
    lines.find(|line| line.trim() == QUOTA_HEADING)?;
    Some(lines.take_while(|line| !line.trim_start().starts_with("### ")))
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(
            || "Antigravity sent an empty /usage reply.".to_string(),
            clip,
        )
}

enum Bold<'a> {
    Heading(&'a str),
    Field(&'a str, &'a str),
}

fn bold(item: &str) -> Option<Bold<'_>> {
    let inner = item.strip_prefix("**")?;
    let (name, rest) = inner.split_once("**")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(Bold::Heading(name.trim()));
    }
    Some(Bold::Field(name.trim().strip_suffix(':')?.trim(), rest))
}

fn bucket_label(group: Option<&str>, bucket: &str) -> String {
    match group {
        Some(group) => format!("{group} · {bucket}"),
        None => bucket.to_string(),
    }
}

fn window(label: &str, value: &str) -> Option<AntigravityUsageWindow> {
    let (amount, reset) = match value
        .strip_suffix(')')
        .and_then(|value| value.split_once(" (resets "))
    {
        Some((amount, reset)) => (amount, Some(reset)),
        None => (value, None),
    };
    let remaining: f64 = amount.strip_suffix("% remaining")?.trim().parse().ok()?;
    if !(0.0..=100.0).contains(&remaining) {
        return None;
    }
    Some(AntigravityUsageWindow {
        kind: "model".to_string(),
        label: label.to_string(),
        model_id: label.to_string(),
        percent: 100.0 - remaining,
        resets_at: reset.and_then(reset_iso),
    })
}

fn reset_iso(text: &str) -> Option<String> {
    chrono::NaiveDateTime::parse_from_str(text.trim(), RESET_FORMAT)
        .ok()
        .map(|at| {
            at.and_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
}

fn clip(line: &str) -> String {
    line.chars().take(MAX_LINE_CHARS).collect()
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
