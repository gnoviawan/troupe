//! Installing a catalog server from the MCP servers page (`use-mcp-catalog-install.ts`,
//! `McpCatalogInstallDialog.tsx`, `McpInstallForm.tsx` and `McpConsentDialog.tsx`): the host
//! describes the form, plans the launch from the answers, shows it for consent, and commits it.
//! The app saves the commit result into the registry the way `saveCatalogInstall` does.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::mcp_catalog::Row;

/// `McpInstallTarget`: the catalog row and the option the install uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub name: String,
    pub version: String,
    pub title: String,
    pub choice: Value,
}

impl Target {
    /// `targetOf`; `None` when the row has no default option.
    #[must_use]
    pub fn of(row: &Row) -> Option<Self> {
        let choice = row.default_option.clone().filter(|c| !c.is_null())?;
        let title = row.title.clone().unwrap_or_else(|| row.name.rsplit('/').next().unwrap_or_default().to_owned());
        Some(Self { name: row.name.clone(), version: row.version.clone(), title, choice })
    }
}

/// `McpInstallFieldTarget.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Env,
    Header,
    Argument,
    Url,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FieldTarget {
    pub kind: FieldKind,
}

/// `McpInstallField`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: String,
    pub label: String,
    pub target: FieldTarget,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default = "string_format")]
    pub format: String,
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

fn string_format() -> String {
    "string".into()
}

/// `ControlKind`: how a field is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Secret,
    Choice,
    Boolean,
    Number,
    FilePath,
    Text,
}

impl Field {
    /// `controlKind`.
    #[must_use]
    pub fn control(&self) -> Control {
        if self.is_secret {
            return Control::Secret;
        }
        if self.choices.is_some() {
            return Control::Choice;
        }
        match self.format.as_str() {
            "boolean" => Control::Boolean,
            "number" => Control::Number,
            "filepath" => Control::FilePath,
            _ => Control::Text,
        }
    }
}

/// `FIELD_GROUPS`: the form's fieldsets, in order.
pub const FIELD_GROUPS: [(FieldKind, &str); 4] = [(FieldKind::Env, "Environment"), (FieldKind::Header, "Headers"), (FieldKind::Argument, "Arguments"), (FieldKind::Url, "URL")];

/// `initialInstallValues`.
#[must_use]
pub fn initial_values(fields: &[Field]) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|f| {
            let value = if f.format == "boolean" {
                (f.default.as_deref().map(|d| d.trim().to_lowercase()) == Some("true".into())).to_string()
            } else {
                f.default.clone().unwrap_or_default()
            };
            (f.id.clone(), value)
        })
        .collect()
}

/// `splitInstallValues`: the plain answers the plan reads, and the secrets only the commit gets.
#[must_use]
pub fn split_values(fields: &[Field], values: &BTreeMap<String, String>) -> (Map<String, Value>, Map<String, Value>) {
    let (mut inputs, mut secrets) = (Map::new(), Map::new());
    for f in fields {
        let bucket = if f.is_secret { &mut secrets } else { &mut inputs };
        bucket.insert(f.id.clone(), json!(values.get(&f.id).cloned().unwrap_or_default()));
    }
    (inputs, secrets)
}

/// The required fields left blank, each with its message (`submit`).
#[must_use]
pub fn missing(fields: &[Field], values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    fields
        .iter()
        .filter(|f| f.is_required && values.get(&f.id).is_none_or(|v| v.trim().is_empty()))
        .map(|f| (f.id.clone(), "Fill this in to continue.".to_owned()))
        .collect()
}

/// `McpInstallFailure`.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    Unsupported(Value),
    Host(String),
    Save,
}

impl Failure {
    /// `canStartOver`: a version the catalog no longer lists fails the same way on every retry.
    #[must_use]
    pub fn can_start_over(&self) -> bool {
        !matches!(self, Self::Host(code) if code == "MCP_CATALOG_NOT_FOUND")
    }

    /// `failureCode`, for the log line.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::Unsupported(reason) => reason.get("code").and_then(Value::as_str).unwrap_or_default().to_owned(),
            Self::Host(code) => code.clone(),
            Self::Save => "save".into(),
        }
    }

    /// `mcpInstallFailureText`.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Unsupported(reason) => super::mcp_catalog::reason_text(reason),
            Self::Save => "Troupe couldn't save the server after you approved it. Start over to try again.".into(),
            Self::Host(code) => host_failure_text(code).into(),
        }
    }
}

/// `HOST_FAILURE_TEXT`.
fn host_failure_text(code: &str) -> &'static str {
    match code {
        "MCP_PLAN_EXPIRED" => "The review expired before it was approved, so nothing was saved. Start over to review it again.",
        "MCP_PLAN_CONSUMED" => "This review was already used. Start over to install it again.",
        "MCP_PLAN_NOT_FOUND" => "Troupe lost track of this install, for example after a reconnect. Start over to review it again.",
        "MCP_PLAN_WRONG_CALLER" => "This install was started from another connection. Start over to review it here.",
        "MCP_PLAN_MISMATCH" => "What would be saved no longer matches what you reviewed, so nothing was saved. Start over to review it again.",
        "MCP_PLAN_LIMIT" => "Too many installs are waiting for approval. Close the others, then start over.",
        "MCP_CATALOG_NOT_FOUND" => "The catalog no longer lists this version. Search again to install the current one.",
        "FORBIDDEN" => "This connection can't make changes on the host, so it can't install servers.",
        _ => "Troupe couldn't install this server. Try again.",
    }
}

/// `McpInstallProbe`.
#[derive(Clone, Debug, PartialEq)]
pub enum Probe {
    Connected(usize),
    Disconnected(Option<String>),
    SignInRequired,
}

impl Probe {
    /// `probeInstalled`, from the store's probe state once it settled.
    #[must_use]
    pub fn read(status: Option<&str>, tools: usize, error: Option<&str>, stdio: bool) -> Self {
        match status {
            Some("connected") => Self::Connected(tools),
            Some("authRequired") if !stdio => Self::SignInRequired,
            _ => Self::Disconnected(error.map(str::to_owned)),
        }
    }

    /// `mcpInstallProbeText`.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Connected(n) => format!("It responds and offers {n} {}.", if *n == 1 { "tool" } else { "tools" }),
            Self::SignInRequired => "It needs you to sign in before agents can use it.".into(),
            Self::Disconnected(Some(e)) if !e.is_empty() => format!("Troupe couldn't reach it yet: {e}"),
            Self::Disconnected(_) => "Troupe couldn't reach it yet. Check it in the MCP servers list.".into(),
        }
    }
}

/// A form the host described, with the answers so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Filling {
    pub target: Target,
    pub form: Value,
    pub fields: Vec<Field>,
    pub values: BTreeMap<String, String>,
}

impl Filling {
    /// A described form; `None` when its fields do not parse.
    #[must_use]
    pub fn new(target: Target, form: Value) -> Option<Self> {
        let fields: Vec<Field> = serde_json::from_value(form.get("fields").cloned().unwrap_or(json!([]))).ok()?;
        let values = initial_values(&fields);
        Some(Self { target, form, fields, values })
    }

    /// The plan request (`planInstall`).
    #[must_use]
    pub fn plan_request(&self) -> Value {
        let (inputs, _) = split_values(&self.fields, &self.values);
        json!({ "source": self.form["source"], "choice": self.target.choice, "inputs": inputs })
    }
}

/// `McpInstallFlow`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Flow {
    #[default]
    Idle,
    Describing(Target),
    Form { filling: Filling, error: Option<String>, field_errors: BTreeMap<String, String> },
    Planning(Filling),
    Consent { filling: Filling, plan: Value },
    Committing { filling: Filling, plan: Value },
    Probing { target: Target, server: Value },
    Done { target: Target, server: Value, probe: Probe },
    Failed { target: Target, failure: Failure },
}

impl Flow {
    #[must_use]
    pub fn target(&self) -> Option<&Target> {
        match self {
            Self::Idle => None,
            Self::Describing(target) | Self::Probing { target, .. } | Self::Done { target, .. } | Self::Failed { target, .. } => Some(target),
            Self::Form { filling, .. } | Self::Planning(filling) | Self::Consent { filling, .. } | Self::Committing { filling, .. } => Some(&filling.target),
        }
    }
}

/// `CONSENT_WARNING_COPY`.
#[must_use]
pub fn warning_text(warning: &str) -> &'static str {
    match warning {
        "runs-local-code" => "This runs a program on this computer with your permissions.",
        "runs-inline-code" => "It hands a shell or interpreter code written in its arguments. Read every argument before you approve.",
        "mounts-host-path" => "The container can read and change the folders on this computer listed below.",
        "mounts-sensitive-path" => "It mounts the root folder, a home folder or the Docker socket, which gives the container control of this computer.",
        "sends-data-remote" => "This sends your data to a remote server.",
        "mutable-tag" => "This image tag can change without notice.",
        "deprecated" => "The registry marks this server deprecated.",
        "from-file" => "These values came from a file. You didn't type them.",
        _ => "",
    }
}

/// `DANGER_WARNINGS`: these show in a danger notice, the rest in a warning notice.
#[must_use]
pub fn is_danger(warning: &str) -> bool {
    warning == "mounts-sensitive-path"
}

/// `namespaceLabel`.
#[must_use]
pub fn namespace_label(namespace: &Value) -> String {
    let field = |k: &str| namespace.get(k).and_then(Value::as_str).unwrap_or_default();
    if field("kind") == "github" { format!("GitHub account {}", field("account")) } else { format!("Domain {}", field("domain")) }
}

/// `snapshotAge`, from `formatRelativeTime`'s compact form.
#[must_use]
pub fn snapshot_age(relative: &str) -> String {
    if relative == "now" {
        return "just now".into();
    }
    let digits = relative.trim_end_matches(['m', 'h', 'd', 'w']);
    let unit_len = relative.len() - digits.len();
    if unit_len == 1 && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) { format!("{relative} ago") } else { relative.to_owned() }
}

#[cfg(test)]
#[path = "mcp_install_tests.rs"]
mod tests;
