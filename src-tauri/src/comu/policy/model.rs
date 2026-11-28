//! Data model, validation, and wire views for Troupe Comu policy.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_ALLOWED_APPS: usize = 256;
pub const MAX_APP_ID_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComuCallerScope {
    App,
    Project(String),
}

impl ComuCallerScope {
    pub fn require_app(&self) -> Result<(), ComuPolicyRefusal> {
        match self {
            Self::App => Ok(()),
            Self::Project(_) => Err(ComuPolicyRefusal::AppScopeRequired),
        }
    }

    pub fn require_project(&self, project_id: &str) -> Result<(), ComuPolicyRefusal> {
        match self {
            Self::App => Ok(()),
            Self::Project(bound) if bound == project_id => Ok(()),
            Self::Project(_) => Err(ComuPolicyRefusal::ProjectScopeMismatch),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComuWireCode {
    InvalidParams,
    Forbidden,
    Conflict,
    Unavailable,
    Internal,
}

impl ComuWireCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidParams => "INVALID_PARAMS",
            Self::Forbidden => "FORBIDDEN",
            Self::Conflict => "CONFLICT",
            Self::Unavailable => "CAPABILITY_UNAVAILABLE",
            Self::Internal => "INTERNAL_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComuPolicyRefusal {
    InvalidAppId(&'static str),
    LimitExceeded,
    MalformedOperation(&'static str),
    InvalidProjectId,
    AppScopeRequired,
    ProjectScopeMismatch,
    RevisionConflict { current: u64 },
    PolicyReadOnly,
    StoreUnavailable,
    StorageFailed(String),
}

impl ComuPolicyRefusal {
    #[must_use]
    pub const fn code(&self) -> ComuWireCode {
        match self {
            Self::InvalidAppId(_) | Self::LimitExceeded | Self::MalformedOperation(_) | Self::InvalidProjectId => {
                ComuWireCode::InvalidParams
            }
            Self::AppScopeRequired | Self::ProjectScopeMismatch | Self::PolicyReadOnly => {
                ComuWireCode::Forbidden
            }
            Self::RevisionConflict { .. } => ComuWireCode::Conflict,
            Self::StoreUnavailable => ComuWireCode::Unavailable,
            Self::StorageFailed(_) => ComuWireCode::Internal,
        }
    }

    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::InvalidAppId(_) => "invalid_app_id",
            Self::LimitExceeded => "limit_exceeded",
            Self::MalformedOperation(_) => "malformed_operation",
            Self::InvalidProjectId => "invalid_project_id",
            Self::AppScopeRequired => "app_scope_required",
            Self::ProjectScopeMismatch => "project_scope_mismatch",
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::PolicyReadOnly => "policy_read_only",
            Self::StoreUnavailable => "policy_store_unavailable",
            Self::StorageFailed(_) => "storage_failed",
        }
    }

    #[must_use]
    pub fn envelope_error(&self) -> String {
        let detail = match self {
            Self::InvalidAppId(msg) => (*msg).to_string(),
            Self::LimitExceeded => format!("at most {MAX_ALLOWED_APPS} allowed apps may be saved"),
            Self::MalformedOperation(msg) => (*msg).to_string(),
            Self::InvalidProjectId => "projectId must be a safe non-empty identifier".to_string(),
            Self::AppScopeRequired => "app scope is required".to_string(),
            Self::ProjectScopeMismatch => "caller scope does not match projectId".to_string(),
            Self::RevisionConflict { current } => {
                format!("policy changed on another surface (current revision {current})")
            }
            Self::PolicyReadOnly => "this policy file was written by a newer Troupe version".to_string(),
            Self::StoreUnavailable => "Computer Use policy is only available on the Troupe desktop app".to_string(),
            Self::StorageFailed(msg) => msg.clone(),
        };
        format!("{}: {detail}", self.reason())
    }
}

pub fn validate_project_id(project_id: &str) -> Result<(), ComuPolicyRefusal> {
    if project_id.is_empty()
        || project_id.len() > 128
        || project_id == "."
        || project_id == ".."
        || project_id
            .chars()
            .any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control())
    {
        return Err(ComuPolicyRefusal::InvalidProjectId);
    }
    Ok(())
}

/// Normalize and validate an application identifier (bundle ID, AUMID, or app name).
pub fn normalize_app_entry(raw: &str) -> Result<String, ComuPolicyRefusal> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ComuPolicyRefusal::InvalidAppId("app identifier cannot be empty"));
    }
    if trimmed.len() > MAX_APP_ID_LEN {
        return Err(ComuPolicyRefusal::InvalidAppId("app identifier is too long"));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(ComuPolicyRefusal::InvalidAppId(
            "app identifier cannot contain control characters",
        ));
    }
    let lowered = trimmed.to_ascii_lowercase();
    if lowered == "troupe"
        || lowered == "termul-manager"
        || lowered == "com.termul-manager.app"
    {
        return Err(ComuPolicyRefusal::InvalidAppId(
            "Troupe itself cannot be added to Allowed Apps",
        ));
    }
    Ok(trimmed.to_string())
}

#[must_use]
pub fn app_entry_matches(saved: &str, candidate_id: &str, candidate_name: &str) -> bool {
    let s = saved.trim();
    if s.is_empty() {
        return false;
    }
    (!candidate_id.is_empty() && s.eq_ignore_ascii_case(candidate_id.trim()))
        || (!candidate_name.is_empty() && s.eq_ignore_ascii_case(candidate_name.trim()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComuAppPolicy {
    pub revision: u64,
    pub read_only: bool,
    pub enabled: bool,
    pub cursor_enabled: bool,
    pub screen_indicator_enabled: bool,
    pub ask_before_driving_apps: bool,
    pub ask_before_foreground_desktop: bool,
    pub ask_before_clipboard_read: bool,
    pub allowed_apps: Vec<String>,
}

impl Default for ComuAppPolicy {
    fn default() -> Self {
        Self {
            revision: 0,
            read_only: false,
            enabled: true,
            cursor_enabled: true,
            screen_indicator_enabled: true,
            ask_before_driving_apps: true,
            ask_before_foreground_desktop: true,
            ask_before_clipboard_read: true,
            allowed_apps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComuProjectPolicy {
    pub revision: u64,
    pub read_only: bool,
    pub allowed_apps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComuAppOp {
    SetEnabled(bool),
    SetCursorEnabled(bool),
    SetScreenIndicatorEnabled(bool),
    SetAskBeforeDrivingApps(bool),
    SetAskBeforeForegroundDesktop(bool),
    SetAskBeforeClipboardRead(bool),
    AddApp(String),
    RemoveApp(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComuProjectOp {
    AddApp(String),
    RemoveApp(String),
}

pub fn parse_app_op(value: &Value) -> Result<ComuAppOp, ComuPolicyRefusal> {
    let Some(obj) = value.as_object() else {
        return Err(ComuPolicyRefusal::MalformedOperation("operation must be an object"));
    };
    let Some(op) = obj.get("op").and_then(Value::as_str) else {
        return Err(ComuPolicyRefusal::MalformedOperation("missing string field `op`"));
    };
    let bool_field = |key: &'static str| -> Result<bool, ComuPolicyRefusal> {
        obj.get(key)
            .and_then(Value::as_bool)
            .ok_or(ComuPolicyRefusal::MalformedOperation("expected boolean field"))
    };
    match op {
        "set_enabled" => Ok(ComuAppOp::SetEnabled(bool_field("enabled")?)),
        "set_cursor_enabled" => Ok(ComuAppOp::SetCursorEnabled(bool_field("enabled")?)),
        "set_screen_indicator_enabled" => {
            Ok(ComuAppOp::SetScreenIndicatorEnabled(bool_field("enabled")?))
        }
        "set_ask_before_driving_apps" => {
            Ok(ComuAppOp::SetAskBeforeDrivingApps(bool_field("enabled")?))
        }
        "set_ask_before_foreground_desktop" => Ok(ComuAppOp::SetAskBeforeForegroundDesktop(
            bool_field("enabled")?,
        )),
        "set_ask_before_clipboard_read" => {
            Ok(ComuAppOp::SetAskBeforeClipboardRead(bool_field("enabled")?))
        }
        "add_app" => {
            let app = obj
                .get("app")
                .and_then(Value::as_str)
                .ok_or(ComuPolicyRefusal::MalformedOperation("missing string field `app`"))?;
            Ok(ComuAppOp::AddApp(normalize_app_entry(app)?))
        }
        "remove_app" => {
            let app = obj
                .get("app")
                .and_then(Value::as_str)
                .ok_or(ComuPolicyRefusal::MalformedOperation("missing string field `app`"))?;
            Ok(ComuAppOp::RemoveApp(app.trim().to_string()))
        }
        _ => Err(ComuPolicyRefusal::MalformedOperation("unknown app policy op")),
    }
}

pub fn parse_project_op(value: &Value) -> Result<ComuProjectOp, ComuPolicyRefusal> {
    let Some(obj) = value.as_object() else {
        return Err(ComuPolicyRefusal::MalformedOperation("operation must be an object"));
    };
    let Some(op) = obj.get("op").and_then(Value::as_str) else {
        return Err(ComuPolicyRefusal::MalformedOperation("missing string field `op`"));
    };
    let app = obj
        .get("app")
        .and_then(Value::as_str)
        .ok_or(ComuPolicyRefusal::MalformedOperation("missing string field `app`"))?;
    match op {
        "add_app" => Ok(ComuProjectOp::AddApp(normalize_app_entry(app)?)),
        "remove_app" => Ok(ComuProjectOp::RemoveApp(app.trim().to_string())),
        _ => Err(ComuPolicyRefusal::MalformedOperation(
            "unknown project policy op",
        )),
    }
}

pub fn apply_app_mutation(
    policy: &mut ComuAppPolicy,
    op: ComuAppOp,
) -> Result<bool, ComuPolicyRefusal> {
    if policy.read_only {
        return Err(ComuPolicyRefusal::PolicyReadOnly);
    }
    let changed = match op {
        ComuAppOp::SetEnabled(v) => replace_bool(&mut policy.enabled, v),
        ComuAppOp::SetCursorEnabled(v) => replace_bool(&mut policy.cursor_enabled, v),
        ComuAppOp::SetScreenIndicatorEnabled(v) => {
            replace_bool(&mut policy.screen_indicator_enabled, v)
        }
        ComuAppOp::SetAskBeforeDrivingApps(v) => {
            replace_bool(&mut policy.ask_before_driving_apps, v)
        }
        ComuAppOp::SetAskBeforeForegroundDesktop(v) => {
            replace_bool(&mut policy.ask_before_foreground_desktop, v)
        }
        ComuAppOp::SetAskBeforeClipboardRead(v) => {
            replace_bool(&mut policy.ask_before_clipboard_read, v)
        }
        ComuAppOp::AddApp(app) => insert_unique_app(&mut policy.allowed_apps, app)?,
        ComuAppOp::RemoveApp(app) => remove_matching_app(&mut policy.allowed_apps, &app),
    };
    if changed {
        policy.revision = policy.revision.saturating_add(1);
    }
    Ok(changed)
}

pub fn apply_project_mutation(
    policy: &mut ComuProjectPolicy,
    op: ComuProjectOp,
) -> Result<bool, ComuPolicyRefusal> {
    if policy.read_only {
        return Err(ComuPolicyRefusal::PolicyReadOnly);
    }
    let changed = match op {
        ComuProjectOp::AddApp(app) => insert_unique_app(&mut policy.allowed_apps, app)?,
        ComuProjectOp::RemoveApp(app) => remove_matching_app(&mut policy.allowed_apps, &app),
    };
    if changed {
        policy.revision = policy.revision.saturating_add(1);
    }
    Ok(changed)
}

fn replace_bool(slot: &mut bool, next: bool) -> bool {
    if *slot == next {
        return false;
    }
    *slot = next;
    true
}

fn insert_unique_app(list: &mut Vec<String>, app: String) -> Result<bool, ComuPolicyRefusal> {
    if list.iter().any(|existing| existing.eq_ignore_ascii_case(&app)) {
        return Ok(false);
    }
    if list.len() >= MAX_ALLOWED_APPS {
        return Err(ComuPolicyRefusal::LimitExceeded);
    }
    list.push(app);
    Ok(true)
}

fn remove_matching_app(list: &mut Vec<String>, target: &str) -> bool {
    let before = list.len();
    list.retain(|entry| !entry.eq_ignore_ascii_case(target));
    list.len() != before
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuAppPolicyView {
    pub revision: u64,
    pub read_only: bool,
    pub enabled: bool,
    pub cursor_enabled: bool,
    pub screen_indicator_enabled: bool,
    pub ask_before_driving_apps: bool,
    pub ask_before_foreground_desktop: bool,
    pub ask_before_clipboard_read: bool,
    pub allowed_apps: Vec<String>,
}

impl From<&ComuAppPolicy> for ComuAppPolicyView {
    fn from(policy: &ComuAppPolicy) -> Self {
        Self {
            revision: policy.revision,
            read_only: policy.read_only,
            enabled: policy.enabled,
            cursor_enabled: policy.cursor_enabled,
            screen_indicator_enabled: policy.screen_indicator_enabled,
            ask_before_driving_apps: policy.ask_before_driving_apps,
            ask_before_foreground_desktop: policy.ask_before_foreground_desktop,
            ask_before_clipboard_read: policy.ask_before_clipboard_read,
            allowed_apps: policy.allowed_apps.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuProjectPolicyView {
    pub project_id: String,
    pub revision: u64,
    pub read_only: bool,
    pub allowed_apps: Vec<String>,
}

impl ComuProjectPolicyView {
    #[must_use]
    pub fn from_policy(project_id: &str, policy: &ComuProjectPolicy) -> Self {
        Self {
            project_id: project_id.to_string(),
            revision: policy.revision,
            read_only: policy.read_only,
            allowed_apps: policy.allowed_apps.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComuPolicyChanged {
    pub scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub revision: u64,
}
