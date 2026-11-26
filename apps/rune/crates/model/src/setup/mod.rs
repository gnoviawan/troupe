//! First run on the web client: the server setup wizard (`ServerSetupWizard`), its restart
//! handoff (`RestartingScreen`), the web access token gate (`web-auth-gate.ts`) and the
//! first-run onboarding gate (`onboarding-gate.ts`, `land-onboarding.ts`). The rules here are
//! pure; the app runs the probes and draws.

use serde::{Deserialize, Serialize};

/// The two bind hosts the server accepts.
pub const LOOPBACK: &str = "127.0.0.1";
pub const ALL: &str = "0.0.0.0";

/// `normalizeSetupHost`: any accepted spelling onto the two bind modes; unknown is loopback.
#[must_use]
pub fn normalize_host(host: &str) -> &'static str {
    match crate::js_trim(host).to_lowercase().as_str() {
        "0.0.0.0" | "all" | "any" => ALL,
        _ => LOOPBACK,
    }
}

/// The wizard's answers (`ServerSetupAnswers`). `port` and `updateIntervalSecs` are what the
/// number fields hold: `None` while a field does not read as a number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answers {
    pub host: String,
    pub port: Option<f64>,
    pub sessions_dir: String,
    pub projects_file: String,
    pub allow_remote_writes: bool,
    pub update_channel: Option<String>,
    pub update_interval_secs: Option<f64>,
}

impl Answers {
    /// `normalizeAnswers`: the host as a bind mode, and remote writes only off loopback.
    #[must_use]
    pub fn normalized(mut self) -> Answers {
        self.set_host(&self.host.clone());
        self
    }

    /// `handleHostChange`: a loopback bind drops remote writes, as the CLI never asks.
    pub fn set_host(&mut self, value: &str) {
        self.host = normalize_host(value).to_owned();
        if self.host != ALL {
            self.allow_remote_writes = false;
        }
    }

    /// The client's own check before the server's dry run.
    #[must_use]
    pub fn port_error(&self) -> Option<&'static str> {
        match self.port {
            Some(p) if p.fract() == 0.0 && (1.0..=65535.0).contains(&p) => None,
            _ => Some("Enter a whole number between 1 and 65535."),
        }
    }
}

/// The Updates select: value, label and the hint under it.
pub const CHANNELS: [(&str, &str, &str); 3] = [
    ("none", "Don't check for updates", "Troupe will not look for new versions."),
    ("stable", "Stable", "Recommended — finished, tested releases."),
    ("nightly", "Nightly", "The very latest changes every day — expect breakage."),
];

/// The Reachability select's rows.
pub const HOSTS: [(&str, &str); 2] = [(LOOPBACK, "Only this computer"), (ALL, "Other devices on the network")];

/// The hint under the reachability select.
#[must_use]
pub fn host_hint(host: &str) -> &'static str {
    if host == ALL {
        "Phones, tablets and computers that can reach this machine will be able to connect."
    } else {
        "Safest — nothing else on the network can connect."
    }
}

fn is_wildcard(host: &str) -> bool {
    matches!(crate::js_trim(host).to_lowercase().as_str(), "0.0.0.0" | "::" | "[::]" | "all" | "any")
}

/// `restartTargetHost`: a wildcard bind is reached on the host name the page came from.
#[must_use]
pub fn restart_host(host: &str, page_host: &str) -> String {
    if !is_wildcard(host) {
        return crate::js_trim(host).to_owned();
    }
    if page_host.is_empty() { LOOPBACK.to_owned() } else { page_host.to_owned() }
}

#[must_use]
pub fn restart_origin(host: &str, port: u16, page_host: &str) -> String {
    format!("http://{}:{port}", restart_host(host, page_host))
}

#[must_use]
pub fn restart_health_url(host: &str, port: u16, page_host: &str) -> String {
    format!("{}/health", restart_origin(host, port, page_host))
}

/// `restartLandingUrl`: the new origin, with a token on the `#token=` fragment. `path` is the
/// page's own path, so the Rune client served under a prefix lands on itself.
#[must_use]
pub fn restart_landing_url(host: &str, port: u16, token: Option<&str>, page_host: &str, path: &str) -> String {
    let origin = restart_origin(host, port, page_host);
    let path = if path.is_empty() { "/" } else { path };
    match token {
        Some(token) => format!("{origin}{path}#token={}", encode_uri_component(token)),
        None => format!("{origin}{path}"),
    }
}

/// JavaScript's `encodeURIComponent`.
#[must_use]
pub fn encode_uri_component(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The web auth gate (`WebAuthGateStatus`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthGate {
    #[default]
    Checking,
    Ok,
    Unauthorized,
    NetworkError,
}

/// How a boot probe of a gated route ended: the host's refusal code, if any.
#[must_use]
pub fn auth_verdict(result: Result<(), Option<&str>>) -> AuthGate {
    match result {
        Ok(()) => AuthGate::Ok,
        Err(Some("UNAUTHORIZED")) => AuthGate::Unauthorized,
        Err(_) => AuthGate::NetworkError,
    }
}

/// What the token gate's form says after a submit (`WebTokenGateScreen`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submit {
    Empty,
    Invalid,
    Unreachable,
}

impl Submit {
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Submit::Empty => "Enter the access token from your server.",
            Submit::Invalid => "Invalid token — check and try again.",
            Submit::Unreachable => "Could not reach the server. Try again.",
        }
    }
}

/// `WriteAdmission`: whether this client may write settings and add projects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Writes {
    #[default]
    Pending,
    Admitted,
    Refused,
    TimedOut,
}

/// `AppSettingsHostRead`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostRead {
    #[default]
    Pending,
    Found,
    Missing,
    Failed,
}

/// `OnboardingFacts`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    pub host_read: HostRead,
    pub completed_at: Option<String>,
    pub projects_loaded: bool,
    pub project_count: usize,
    pub writes: Writes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Pending,
    Show,
    Skip,
}

/// `decideOnboarding`: first run shows only when every fact holds; one fact that says skip
/// decides at once.
#[must_use]
pub fn decide(f: &Facts) -> Decision {
    if f.projects_loaded && f.project_count > 0 {
        return Decision::Skip;
    }
    if f.host_read == HostRead::Failed {
        return Decision::Skip;
    }
    if f.host_read != HostRead::Pending && f.completed_at.is_some() {
        return Decision::Skip;
    }
    if matches!(f.writes, Writes::Refused | Writes::TimedOut) {
        return Decision::Skip;
    }
    if f.host_read == HostRead::Pending || !f.projects_loaded || f.writes == Writes::Pending {
        return Decision::Pending;
    }
    Decision::Show
}

/// `OnboardingLanding`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Landing {
    Launcher { project_id: String },
    QuickChat,
    Workspace,
}

/// `onboardingLanding`.
#[must_use]
pub fn landing(agent: Option<&str>, project_id: Option<&str>) -> Landing {
    match (project_id, agent) {
        (Some(id), _) if !id.is_empty() => Landing::Launcher { project_id: id.to_owned() },
        (_, Some(_)) => Landing::QuickChat,
        _ => Landing::Workspace,
    }
}

/// `ONBOARDING_STEPS`.
pub const STEPS: [&str; 3] = ["welcome", "agents", "project"];

/// The welcome step's outline.
pub const OUTLINE: [&str; 3] = ["Check the agents on this computer", "Pick a project folder", "Send your first message"];

#[cfg(test)]
mod tests;
