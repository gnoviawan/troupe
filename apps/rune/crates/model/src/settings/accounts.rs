//! Settings → Accounts (`components/settings/provider/`, `lib/claude-usage-format.ts`,
//! `lib/codex-usage-format.ts` and `lib/antigravity-usage-format.ts`): the words and tones of the
//! account rows of the three providers, and their usage meters. Snapshots and usage reads stay
//! JSON as the host sends them.

use serde_json::Value;

/// `AccountProviderId`, with what each group shows and says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Codex,
    Antigravity,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Antigravity];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
        }
    }

    /// The group heading.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Antigravity => "Antigravity",
        }
    }

    /// `PROVIDER_TERMS[provider].name`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Antigravity => "Antigravity",
        }
    }

    #[must_use]
    pub fn company(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Antigravity => "Google",
        }
    }

    /// `PROVIDER_TERMS[provider].autoSwitchRisk`.
    #[must_use]
    pub fn risk(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic says Claude plan limits assume ordinary, individual use. Its terms do not allow access by automated means unless Anthropic permits it, and Anthropic can suspend an account without notice.",
            Self::Codex => "OpenAI's terms do not allow getting around rate limits or restrictions. Moving a chat to another account at its Codex limit can count as that. OpenAI's own answer to a Codex limit is to buy credits or use an API key.",
            Self::Antigravity => "Google's Antigravity FAQ says automated account rotation is not supported on consumer plans. Google has suspended Antigravity access for breaking its terms.",
        }
    }

    #[must_use]
    pub fn docs_url(self) -> String {
        let page = match self {
            Self::Claude => "claude-accounts",
            Self::Codex => "codex-accounts",
            Self::Antigravity => "antigravity-accounts",
        };
        format!("https://usetroupe.dev/docs/agent-setup/{page}#risks-of-switching-accounts")
    }
}

/// `SHARED_SWITCH_RISK`.
pub const SHARED_SWITCH_RISK: &str = "A provider can suspend all the accounts of one person together. Safer options are to switch by hand when a chat reaches its limit, buy extra usage or credits, or use an API key.";

/// `UsageTone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageTone {
    Normal,
    Warning,
    Critical,
}

/// `usageTone`.
#[must_use]
pub fn usage_tone(percent: f64) -> UsageTone {
    if percent >= 100.0 {
        UsageTone::Critical
    } else if percent >= 80.0 {
        UsageTone::Warning
    } else {
        UsageTone::Normal
    }
}

fn capitalized(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().collect::<String>() + chars.as_str())
}

/// `usageWindowLabel` (Claude).
#[must_use]
pub fn claude_window_label(kind: &str, scope: Option<&str>) -> String {
    match kind {
        "session" => "5-hour".into(),
        "weekly_all" => "Weekly".into(),
        "weekly_scoped" => scope.filter(|s| !s.is_empty()).map_or_else(|| "Weekly".into(), |s| format!("Weekly · {s}")),
        _ => capitalized(&kind.replace('_', " ")),
    }
}

/// `formatUsageReset`, from the reset time in ms.
#[must_use]
pub fn format_reset(at_ms: Option<f64>, now_ms: f64) -> Option<String> {
    let at = at_ms.filter(|a| a.is_finite())?;
    let minutes = ((at - now_ms) / 60_000.0).round().max(0.0) as i64;
    Some(if minutes < 1 {
        "resets now".into()
    } else if minutes < 60 {
        format!("resets in {minutes}m")
    } else if minutes / 60 < 24 {
        let (hours, rest) = (minutes / 60, minutes % 60);
        if rest > 0 { format!("resets in {hours}h {rest}m") } else { format!("resets in {hours}h") }
    } else {
        format!("resets in {}d", ((minutes / 60) as f64 / 24.0).round() as i64)
    })
}

/// `formatUsageChecked`.
#[must_use]
pub fn format_checked(checked_at: f64, now_ms: f64) -> String {
    let minutes = ((now_ms - checked_at).max(0.0) / 60_000.0).floor() as i64;
    if minutes < 1 {
        "Updated just now".into()
    } else if minutes < 60 {
        format!("Updated {minutes}m ago")
    } else {
        format!("Updated {}h ago", minutes / 60)
    }
}

/// `planLabel`.
#[must_use]
pub fn plan_label(plan: Option<&str>) -> Option<String> {
    plan.filter(|p| !p.is_empty()).map(capitalized)
}

/// `AccountReadiness`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    SignedOut,
    Unconfirmed,
}

impl Readiness {
    /// `accountReadiness`: only a confirmed login passes.
    #[must_use]
    pub fn of(account: &Value, usage: Option<&Value>) -> Self {
        let signed_in = account.get("signedIn").and_then(Value::as_bool);
        if signed_in == Some(false) || usage.and_then(|u| u.get("status")).and_then(Value::as_str) == Some("signed_out") {
            Self::SignedOut
        } else if signed_in == Some(true) {
            Self::Ready
        } else {
            Self::Unconfirmed
        }
    }

    /// `NOT_READY_REASON`.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::Ready => "",
            Self::SignedOut => "Signed out",
            Self::Unconfirmed => "Could not check sign-in",
        }
    }
}

/// `AccountTone`: the status dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountTone {
    Ok,
    Warning,
    Critical,
    SignedOut,
    Waiting,
    Unknown,
}

impl From<Readiness> for AccountTone {
    fn from(r: Readiness) -> Self {
        match r {
            Readiness::Ready => Self::Ok,
            Readiness::SignedOut => Self::SignedOut,
            Readiness::Unconfirmed => Self::Unknown,
        }
    }
}

/// One usage window as a meter draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    pub label: String,
    pub percent: f64,
    pub resets_at: Option<String>,
}

fn windows(usage: Option<&Value>) -> Vec<&Value> {
    usage
        .filter(|u| u.get("status").and_then(Value::as_str) == Some("ok"))
        .and_then(|u| u.get("windows"))
        .and_then(Value::as_array)
        .map(|w| w.iter().collect())
        .unwrap_or_default()
}

fn percent_of(w: &Value) -> f64 {
    w.get("percent").and_then(Value::as_f64).unwrap_or(0.0)
}

fn reset_of(w: &Value) -> Option<String> {
    w.get("resetsAt").and_then(Value::as_str).map(str::to_owned)
}

fn primary_index(ws: &[&Value]) -> Option<usize> {
    ws.iter().enumerate().fold(None::<usize>, |top, (i, w)| if top.is_none_or(|t| percent_of(w) > percent_of(ws[t])) { Some(i) } else { top })
}

#[must_use]
pub fn claude_meter(w: &Value) -> Meter {
    let kind = w.get("kind").and_then(Value::as_str).unwrap_or_default();
    Meter { label: claude_window_label(kind, w.get("scope").and_then(Value::as_str)), percent: percent_of(w), resets_at: reset_of(w) }
}

/// `primaryUsageWindow` (Claude): the most used window.
#[must_use]
pub fn claude_primary(usage: Option<&Value>) -> Option<Meter> {
    let ws = windows(usage);
    primary_index(&ws).map(|i| claude_meter(ws[i]))
}

/// Every Claude window, for the expanded detail.
#[must_use]
pub fn claude_all(usage: Option<&Value>) -> Vec<Meter> {
    windows(usage).into_iter().map(claude_meter).collect()
}

/// `OtherWindows`: the windows beside the main meter; a scoped weekly one reads as its scope.
#[must_use]
pub fn claude_others(usage: Option<&Value>) -> Vec<Meter> {
    let ws = windows(usage);
    let primary = primary_index(&ws);
    ws.iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != primary)
        .map(|(_, w)| {
            let mut m = claude_meter(w);
            let scoped = w.get("kind").and_then(Value::as_str) == Some("weekly_scoped");
            if let Some(scope) = w.get("scope").and_then(Value::as_str).filter(|s| scoped && !s.is_empty()) {
                m.label = scope.to_owned();
            }
            m
        })
        .collect()
}

/// The meters of a Codex or Antigravity row, in the host's order.
#[must_use]
pub fn meters(provider: Provider, usage: Option<&Value>) -> Vec<Meter> {
    windows(usage)
        .into_iter()
        .map(|w| {
            let label = match provider {
                Provider::Codex => if w.get("kind").and_then(Value::as_str) == Some("weekly") { "Weekly" } else { "5-hour" }.to_owned(),
                Provider::Antigravity => w
                    .get("label")
                    .and_then(Value::as_str)
                    .filter(|l| !l.is_empty())
                    .or_else(|| w.get("modelId").and_then(Value::as_str))
                    .unwrap_or_default()
                    .to_owned(),
                Provider::Claude => claude_meter(w).label,
            };
            Meter { label, percent: percent_of(w), resets_at: reset_of(w) }
        })
        .collect()
}

/// `accountStatus` (Claude): the dot and the words under the name.
#[must_use]
pub fn claude_status(account: &Value, usage: Option<&Value>, signing_in: bool, projects: usize) -> (AccountTone, String) {
    let plan = plan_label(usage.and_then(|u| u.get("subscriptionType")).and_then(Value::as_str));
    let with_plan = |words: &str| plan.as_ref().map_or_else(|| words.to_owned(), |p| format!("{p} · {words}"));
    if signing_in {
        return (AccountTone::Waiting, with_plan("waiting for browser"));
    }
    match Readiness::of(account, usage) {
        Readiness::SignedOut => return (AccountTone::SignedOut, with_plan("signed out")),
        Readiness::Unconfirmed => return (AccountTone::Unknown, "Could not check sign-in".into()),
        Readiness::Ready => {}
    }
    if let Some(primary) = claude_primary(usage) {
        match usage_tone(primary.percent) {
            UsageTone::Critical => return (AccountTone::Critical, with_plan("limit reached")),
            UsageTone::Warning => return (AccountTone::Warning, with_plan(&format!("near the {} limit", primary.label))),
            UsageTone::Normal => {}
        }
    }
    let projects = match projects {
        0 => None,
        1 => Some("1 project".to_owned()),
        n => Some(format!("{n} projects")),
    };
    let meta = match (plan, projects) {
        (Some(p), Some(n)) => format!("{p} · {n}"),
        (Some(p), None) => p,
        (None, Some(n)) => n,
        (None, None) => "Signed in".into(),
    };
    (AccountTone::Ok, meta)
}

/// The words under a Codex or Antigravity row's name: the plan or why it is not ready, then
/// the projects that chose it.
#[must_use]
pub fn other_status(account: &Value, usage: Option<&Value>, project_names: &[String]) -> (AccountTone, String) {
    let readiness = Readiness::of(account, usage);
    let first = if readiness == Readiness::Ready {
        plan_label(usage.and_then(|u| u.get("planType")).and_then(Value::as_str)).unwrap_or_else(|| "Signed in".into())
    } else {
        readiness.reason().to_owned()
    };
    let joined = project_names.join(", ");
    let words: Vec<&str> = [first.as_str(), joined.as_str()].into_iter().filter(|w| !w.is_empty()).collect();
    (readiness.into(), words.join(" · "))
}

/// The group header's count.
#[must_use]
pub fn count_text(n: usize) -> String {
    if n == 1 { "1 account".into() } else { format!("{n} accounts") }
}

/// `oldestCheck`.
#[must_use]
pub fn oldest_check<'a>(usage: impl Iterator<Item = &'a Value>) -> Option<f64> {
    usage.filter_map(|u| u.get("checkedAt").and_then(Value::as_f64)).reduce(f64::min)
}

/// `DefaultPill`: not drawn when the name already says "Default".
#[must_use]
pub fn shows_default_pill(name: &str) -> bool {
    name.trim().to_lowercase() != "default"
}

/// The remove confirm's line on the projects that move.
#[must_use]
pub fn moving_text(count: usize, default_name: Option<&str>) -> String {
    let lead = if count == 1 { "1 project moves".to_owned() } else { format!("{count} projects move") };
    default_name.map_or_else(|| format!("{lead} to the default account"), |n| format!("{lead} to the default account ({n})"))
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
