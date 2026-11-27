//! How automations read (`automation-format.ts`): the state pill, statuses, durations,
//! trigger summaries, a card's detail line and wall-clock times.

use serde::Serialize;

use super::types::{
    AgentTool, Automation, NodeSpec, Run, RunStatus, RunTrigger, Schedule, StepStatus, Trigger,
};

/// What the list and the header show; needs-approval wins because it asks the user to act.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    NeedsApproval,
    Disabled,
    Armed,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Armed => "Armed",
            State::NeedsApproval => "Needs approval",
            State::Disabled => "Disabled",
        }
    }
}

pub fn state(a: &Automation) -> State {
    if a.approved_revision != Some(a.revision) {
        State::NeedsApproval
    } else if a.enabled {
        State::Armed
    } else {
        State::Disabled
    }
}

/// The tone a status label takes, as the web client's class names name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Primary,
    Success,
    Destructive,
    Muted,
    Warning,
}

pub fn run_status(status: RunStatus) -> (&'static str, Tone) {
    match status {
        RunStatus::Running => ("Running", Tone::Primary),
        RunStatus::Succeeded => ("Succeeded", Tone::Success),
        RunStatus::Failed => ("Failed", Tone::Destructive),
        RunStatus::Cancelled => ("Cancelled", Tone::Muted),
        RunStatus::Skipped => ("Skipped", Tone::Muted),
    }
}

pub fn step_status(status: StepStatus) -> (&'static str, Tone) {
    match status {
        StepStatus::Running => ("Running", Tone::Primary),
        StepStatus::Succeeded => ("Succeeded", Tone::Success),
        StepStatus::Failed => ("Failed", Tone::Destructive),
        StepStatus::Skipped => ("Skipped", Tone::Muted),
        StepStatus::Waiting => ("Waiting", Tone::Warning),
    }
}

/// The approval node a run is parked on, from the run's own graph; none for a wait.
pub fn parked_approval(run: &Run) -> Option<&super::types::AutomationNode> {
    let parked = run.parked.as_ref()?;
    let node = run.graph.node(&parked.node_id)?;
    matches!(node.spec, NodeSpec::Approval { .. }).then_some(node)
}

/// A run parked on an approval reads as waiting for it; its status stays `running`.
pub fn run_presentation(run: &Run) -> (&'static str, Tone) {
    if parked_approval(run).is_some() {
        ("Waiting for approval", Tone::Warning)
    } else {
        run_status(run.status)
    }
}

pub fn trigger_label(by: RunTrigger) -> &'static str {
    match by {
        RunTrigger::Scheduled => "Scheduled",
        RunTrigger::CatchUp => "Catch-up",
        RunTrigger::Manual => "Manual",
    }
}

pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{} ms", ms.round().max(0.0) as i64);
    }
    let seconds = (ms / 1000.0).round() as i64;
    if seconds < 60 {
        return format!("{seconds} s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return if seconds % 60 != 0 {
            format!("{minutes} min {} s", seconds % 60)
        } else {
            format!("{minutes} min")
        };
    }
    let hours = minutes / 60;
    if hours < 24 {
        return if minutes % 60 != 0 {
            format!("{hours} h {} min", minutes % 60)
        } else {
            format!("{hours} h")
        };
    }
    let days = hours / 24;
    if hours % 24 != 0 {
        format!("{days} d {} h", hours % 24)
    } else {
        format!("{days} {}", if days == 1 { "day" } else { "days" })
    }
}

/// `trigger_summary` for a schedule `at` needs the wall clock: `offset_min` is the local
/// zone's offset from UTC and `now_ms` the current time.
pub fn trigger_summary(trigger: &Trigger, now_ms: f64, offset_min: i64) -> String {
    match trigger {
        Trigger::Manual => "Run manually".to_owned(),
        Trigger::Schedule {
            schedule: Schedule::Cron { expr, timezone },
        } => format!("Cron {expr} ({timezone})"),
        Trigger::Schedule {
            schedule: Schedule::Every { every_ms, .. },
        } => format!("Every {}", format_duration(*every_ms as f64)),
        Trigger::Schedule {
            schedule: Schedule::At { at },
        } => format!(
            "Once at {}",
            format_date_time(*at as f64, now_ms, offset_min)
        ),
    }
}

fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r);
    let Some(rest) = rest else {
        return url.to_owned();
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || host.contains("{{") {
        return url.to_owned();
    }
    host.to_owned()
}

/// The one line a diagram card shows under the label.
pub fn node_key_detail(spec: &NodeSpec) -> String {
    match spec {
        NodeSpec::Http { method, url, .. } => format!("{} {}", method.as_str(), url_host(url)),
        NodeSpec::Agent {
            agent_id, model, ..
        } => match model {
            Some(model) => format!("{agent_id} · {model}"),
            None => agent_id.clone(),
        },
        NodeSpec::Transform { fields } => {
            let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            if names.is_empty() {
                "No fields".to_owned()
            } else {
                names.join(", ")
            }
        }
        NodeSpec::Condition { left, op, right } => [
            left.as_str(),
            op.as_str(),
            right.as_deref().unwrap_or_default(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" "),
        NodeSpec::Fail { message } | NodeSpec::Approval { message, .. } => message.clone(),
        NodeSpec::ForEach {
            items,
            max_iterations,
        } => format!("{items} · max {max_iterations}"),
        NodeSpec::Merge { mode } => match mode {
            super::types::MergeMode::All => "When every branch ran".to_owned(),
            super::types::MergeMode::Any => "When any branch ran".to_owned(),
        },
        NodeSpec::Wait { duration_ms } => format!("Wait {}", format_duration(*duration_ms as f64)),
        NodeSpec::McpTool { tool, .. } => tool.clone(),
    }
}

pub fn agent_tool_label(tool: AgentTool) -> &'static str {
    match tool {
        AgentTool::Read => "read files",
        AgentTool::Search => "search",
        AgentTool::Edit => "edit files",
        AgentTool::Execute => "run commands (full access)",
        AgentTool::Fetch => "fetch from the network",
        AgentTool::Other => "MCP and other tools",
    }
}

/// The run policy's tool allowlist of one agent node, in words.
pub fn agent_tools_label(tools: &[AgentTool]) -> String {
    if tools.is_empty() {
        return "no tools".to_owned();
    }
    format!(
        "tools: {}",
        tools
            .iter()
            .map(|t| agent_tool_label(*t))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Days since 1970-01-01 to (year, month 1-12, day): Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, usize, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m as usize, d)
}

/// Local wall-clock time in the en-US short form, "Mon, Oct 30, 9:00 AM", with the year when
/// it is not the current one ("Mon, Jan 4, 2027, 9:00 AM").
pub fn format_date_time(ms: f64, now_ms: f64, offset_min: i64) -> String {
    let local = |t: f64| (t as i64).div_euclid(60_000) + offset_min;
    let minutes = local(ms);
    let days = minutes.div_euclid(1440);
    let (year, month, day) = civil(days);
    let (now_year, ..) = civil(local(now_ms).div_euclid(1440));
    let of_day = minutes.rem_euclid(1440);
    let (hour, minute) = (of_day / 60, of_day % 60);
    let (h12, half) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    let weekday = WEEKDAYS[days.rem_euclid(7) as usize];
    let date = format!("{weekday}, {} {day}", MONTHS[month - 1]);
    let date = if year == now_year {
        date
    } else {
        format!("{date}, {year}")
    };
    format!("{date}, {h12}:{minute:02} {half}")
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
