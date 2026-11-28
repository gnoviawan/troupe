//! Client-side browser pane state: tab sessions (`browser-session-store.ts`),
//! agent browser control tracking (`browser-control-store.ts`), URL normalization
//! (`browser-url.ts`), and browser policy views/copy (`browser-policy-api.ts`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod session;

pub use session::{
    BrowserDialog, BrowserDialogKind, BrowserFileChooser, BrowserLoadError, BrowserTab,
    BrowserTabView, MAX_BROWSER_ZOOM, MIN_BROWSER_ZOOM, RecentBrowserUrl, SessionEvent,
    SessionState, ViewportDimensions, ViewportOrientation, ViewportPreset, ZOOM_STEPS,
    clamp_browser_zoom, next_zoom_step, resolve_viewport_dimensions, step_session,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayUrlParts {
    pub scheme: String,
    pub host: String,
    pub path: String,
    pub is_https: bool,
}

/// Normalizes user-entered address bar text into a navigable URL.
#[must_use]
pub fn normalize_browser_input_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("about:") {
        return Some(trimmed.to_owned());
    }
    if is_localhost_input(&lower) {
        return Some(format!("http://{trimmed}"));
    }
    if is_port_shorthand(trimmed) {
        return Some(format!("http://localhost{trimmed}"));
    }
    Some(format!("https://{trimmed}"))
}

fn is_localhost_input(lower: &str) -> bool {
    let rest = if let Some(r) = lower.strip_prefix("localhost") {
        r
    } else if let Some(r) = lower.strip_prefix("127.0.0.1") {
        r
    } else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let after_port = if let Some(p) = rest.strip_prefix(':') {
        let digits = p.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return false;
        }
        &p[digits..]
    } else {
        rest
    };
    after_port.is_empty() || matches!(after_port.as_bytes()[0], b'/' | b'?' | b'#')
}

fn is_port_shorthand(s: &str) -> bool {
    let Some(rest) = s.strip_prefix(':') else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let tail = &rest[digits..];
    tail.is_empty() || matches!(tail.as_bytes()[0], b'/' | b'?' | b'#')
}

/// Splits a URL into scheme, host, and path/query/hash for the URL well.
#[must_use]
pub fn split_display_url(raw: &str) -> Option<DisplayUrlParts> {
    if raw.is_empty() {
        return None;
    }
    let (scheme_name, after_scheme) = raw.split_once("://")?;
    if scheme_name.is_empty()
        || !scheme_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    {
        return None;
    }
    let scheme_lower = scheme_name.to_ascii_lowercase();
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let host = &after_scheme[..authority_end];
    if host.is_empty() {
        return None;
    }
    let rest = &after_scheme[authority_end..];
    let path = if rest.is_empty() || rest == "/" {
        String::new()
    } else if rest.starts_with('/') {
        rest.to_owned()
    } else {
        format!("/{rest}")
    };
    Some(DisplayUrlParts {
        scheme: format!("{scheme_lower}://"),
        host: host.to_owned(),
        path,
        is_https: scheme_lower == "https",
    })
}

/// Returns the `host[:port]` of `raw_url`, or `raw_url` itself when unparseable.
#[must_use]
pub fn display_host(raw_url: &str) -> String {
    split_display_url(raw_url).map_or_else(|| raw_url.to_owned(), |parts| parts.host)
}

/// Extracts `(host, port)` when `raw_url` targets `localhost` or `127.0.0.1`.
#[must_use]
pub fn parse_localhost_port(raw_url: &str) -> Option<(String, u16)> {
    let parts = split_display_url(raw_url)?;
    let is_https = parts.is_https;
    let is_http = parts.scheme == "http://";
    if !is_http && !is_https {
        return None;
    }
    let (hostname, explicit_port) = match parts.host.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (parts.host.as_str(), None),
    };
    if !hostname.eq_ignore_ascii_case("localhost") && hostname != "127.0.0.1" {
        return None;
    }
    let port = match explicit_port {
        Some(p) => p.parse::<u16>().ok().filter(|&n| n > 0)?,
        None => {
            if is_https {
                443
            } else {
                80
            }
        }
    };
    Some((parts.host, port))
}

/// Returns true when two URLs share the same origin and pathname.
#[must_use]
pub fn same_origin_and_path(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return true;
    }
    match (split_display_url(a), split_display_url(b)) {
        (Some(ua), Some(ub)) => {
            let path_a = ua.path.split(['?', '#']).next().unwrap_or("");
            let path_b = ub.path.split(['?', '#']).next().unwrap_or("");
            ua.scheme == ub.scheme
                && ua.host.eq_ignore_ascii_case(&ub.host)
                && path_a == path_b
        }
        _ => a == b,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlState {
    /// Agent browser tab id -> owning session id.
    pub tab_sessions: BTreeMap<String, String>,
    /// Sessions latched by Stop (`true` matches the JS record shape).
    pub stopped_sessions: BTreeMap<String, bool>,
}

impl ControlState {
    #[must_use]
    pub fn session_drives_browser(&self, session_id: &str) -> bool {
        self.tab_sessions.values().any(|owner| owner == session_id)
    }

    #[must_use]
    pub fn is_session_stopped(&self, session_id: &str) -> bool {
        self.stopped_sessions.get(session_id).copied().unwrap_or(false)
    }

    #[must_use]
    pub fn agent_tab_session(&self, tab_id: &str) -> Option<&str> {
        self.tab_sessions.get(tab_id).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlEvent {
    TrackAgentTab(String, String),
    UntrackAgentTab(String),
    SetStopped(String, bool),
    ForgetSession(String),
}

pub fn step_control(state: &mut ControlState, event: ControlEvent) {
    match event {
        ControlEvent::TrackAgentTab(tab_id, session_id) => {
            if state.tab_sessions.get(&tab_id).map(String::as_str) != Some(session_id.as_str()) {
                state.tab_sessions.insert(tab_id, session_id);
            }
        }
        ControlEvent::UntrackAgentTab(tab_id) => {
            state.tab_sessions.remove(&tab_id);
        }
        ControlEvent::SetStopped(session_id, stopped) => {
            if stopped {
                state.stopped_sessions.insert(session_id, true);
            } else {
                state.stopped_sessions.remove(&session_id);
            }
        }
        ControlEvent::ForgetSession(session_id) => {
            state.tab_sessions.retain(|_, owner| owner != &session_id);
            state.stopped_sessions.remove(&session_id);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserActionClass {
    Navigate,
    LocalNetwork,
    PasswordSubmit,
    PaymentSubmit,
    Upload,
    Download,
    TaintedExfil,
}

impl BrowserActionClass {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Navigate => "navigate",
            Self::LocalNetwork => "local_network",
            Self::PasswordSubmit => "password_submit",
            Self::PaymentSubmit => "payment_submit",
            Self::Upload => "upload",
            Self::Download => "download",
            Self::TaintedExfil => "tainted_exfil",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Navigate => "Open a site",
            Self::LocalNetwork => "Open a local network address",
            Self::PasswordSubmit => "Submit a password form",
            Self::PaymentSubmit => "Submit a payment form",
            Self::Upload => "Upload a file",
            Self::Download => "Download a file",
            Self::TaintedExfil => "Send page data to another site",
        }
    }
}

pub const ACTION_LABELS: [(BrowserActionClass, &str); 7] = [
    (BrowserActionClass::Navigate, "Open a site"),
    (BrowserActionClass::LocalNetwork, "Open a local network address"),
    (BrowserActionClass::PasswordSubmit, "Submit a password form"),
    (BrowserActionClass::PaymentSubmit, "Submit a payment form"),
    (BrowserActionClass::Upload, "Upload a file"),
    (BrowserActionClass::Download, "Download a file"),
    (BrowserActionClass::TaintedExfil, "Send page data to another site"),
];

pub const APP_PREAPPROVABLE_ACTIONS: [BrowserActionClass; 4] = [
    BrowserActionClass::PasswordSubmit,
    BrowserActionClass::Upload,
    BrowserActionClass::Download,
    BrowserActionClass::TaintedExfil,
];

pub const PROJECT_PREAPPROVABLE_ACTIONS: [BrowserActionClass; 4] = [
    BrowserActionClass::PasswordSubmit,
    BrowserActionClass::Upload,
    BrowserActionClass::Download,
    BrowserActionClass::TaintedExfil,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPreapproval {
    pub action: BrowserActionClass,
    pub origin: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearImpact {
    pub sessions: u64,
    pub projects: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBrowserPolicy {
    pub revision: u64,
    #[serde(default)]
    pub read_only: bool,
    pub ask_before_opening_sites: bool,
    pub ask_before_password_submit: bool,
    pub global_profile_enabled: bool,
    pub effective_global_profile: bool,
    #[serde(default)]
    pub preapprovals: Vec<BrowserPreapproval>,
    #[serde(default)]
    pub clear_impact: ClearImpact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBrowserPolicy {
    pub revision: u64,
    #[serde(default)]
    pub read_only: bool,
    pub profile_mode: String,
    pub effective_profile_mode: String,
    #[serde(default)]
    pub allow_list: Vec<String>,
    #[serde(default)]
    pub preapprovals: Vec<BrowserPreapproval>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserPolicyFailure {
    pub code: String,
    pub reason: Option<String>,
    pub message: String,
}

const KNOWN_REASONS: [&str; 15] = [
    "invalid_origin",
    "invalid_wildcard",
    "reserved_origin",
    "preapproval_not_allowed",
    "limit_exceeded",
    "malformed_operation",
    "invalid_project_id",
    "app_scope_required",
    "project_scope_mismatch",
    "revision_conflict",
    "policy_read_only",
    "policy_store_unavailable",
    "profile_engine_unavailable",
    "profile_clear_failed",
    "storage_failed",
];

#[must_use]
pub fn parse_policy_failure(code: &str, error: &str) -> BrowserPolicyFailure {
    if let Some((head, tail)) = error.split_once(": ")
        && KNOWN_REASONS.contains(&head)
    {
        return BrowserPolicyFailure {
            code: code.to_owned(),
            reason: Some(head.to_owned()),
            message: tail.to_owned(),
        };
    }
    BrowserPolicyFailure {
        code: code.to_owned(),
        reason: None,
        message: error.to_owned(),
    }
}

#[must_use]
pub fn describe_failure(failure: &BrowserPolicyFailure) -> String {
    if let Some(reason) = failure.reason.as_deref() {
        let copy = match reason {
            "invalid_origin" => Some("Enter an http or https address such as https://example.com."),
            "invalid_wildcard" => {
                Some("Use * only as the first part of the host, as in https://*.example.com.")
            }
            "reserved_origin" => {
                Some("Troupe's own addresses and local-only addresses can't be allowed.")
            }
            "preapproval_not_allowed" => Some("This action can't be pre-approved here."),
            "limit_exceeded" => Some("The list is full. Remove an entry first."),
            "malformed_operation" => Some("Troupe could not read that change."),
            "invalid_project_id" => Some("This project is not available."),
            "app_scope_required" => {
                Some("This device can only change the rules for its own project.")
            }
            "project_scope_mismatch" => Some("This device is connected to a different project."),
            "revision_conflict" => Some("The rules kept changing. Reload and try again."),
            "policy_read_only" => Some(
                "These rules were saved by a newer version of Troupe, or their file is damaged. You can read them but not change them.",
            ),
            "policy_store_unavailable" => Some("Browser rules aren't available on this host."),
            "profile_engine_unavailable" => {
                Some("This build can't clear the agent browser profile yet.")
            }
            "profile_clear_failed" => {
                Some("The agent browser profile could not be cleared. Try again.")
            }
            "storage_failed" => Some("The change could not be saved. Try again."),
            _ => None,
        };
        if let Some(text) = copy {
            return text.to_owned();
        }
    }
    match failure.code.as_str() {
        "NETWORK_ERROR" => "Could not reach the host. Check the connection.".to_owned(),
        "UNAUTHORIZED" => "Sign in again to change browser rules.".to_owned(),
        _ => failure.message.clone(),
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
