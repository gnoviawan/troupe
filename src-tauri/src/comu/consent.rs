//! Per-app, foreground-desktop, and clipboard consent gate for Troupe Comu.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Instant;
use troupe_comu_engine::WindowOwnerInfo;

use crate::acp::host_ticket::{ComuConsentAction, ComuTicketRequest, HostTickets};
use crate::browser_automation::core::ports::{ConsentOption, TicketId, TicketOutcome};

use super::policy::{app_identity_key, validate_project_id, ComuPolicyService};

/// Maximum total time an agent call waits across all interactive Comu consent prompts.
pub const CONSENT_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

pub type ConsentFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Port over [`HostTickets`] so `comu::tests` can drive consent deterministically.
pub trait ComuTicketPort: Send + Sync {
    fn request_comu(
        &self,
        ticket: ComuTicketRequest,
        wait: Duration,
    ) -> ConsentFuture<'_, TicketOutcome>;
    fn cancel_comu_session(&self, session_id: &str);
}

impl ComuTicketPort for HostTickets {
    fn request_comu(
        &self,
        ticket: ComuTicketRequest,
        wait: Duration,
    ) -> ConsentFuture<'_, TicketOutcome> {
        HostTickets::request_comu(self, ticket, wait)
    }

    fn cancel_comu_session(&self, session_id: &str) {
        HostTickets::cancel_comu_session(self, session_id);
    }
}

/// Scoped session grant key: stable bundle/AUMID/path/PID identities live in a
/// separate namespace from fallback display names so two apps sharing a display
/// name with different bundle IDs never cross-authorize each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AppGrantKey {
    StableId(String),
    UnidentifiedName(String),
}

impl AppGrantKey {
    fn from_target(target: &ResolvedAppTarget) -> Self {
        if target.has_stable_id {
            Self::StableId(app_identity_key(&target.app_id))
        } else {
            Self::UnidentifiedName(target.app_name.to_ascii_lowercase())
        }
    }
}

#[derive(Default)]
struct SessionGrants {
    apps: HashSet<AppGrantKey>,
    /// True once foreground/desktop takeover was approved for this session.
    foreground_desktop: bool,
}

pub struct ComuConsentGate {
    tickets: Arc<dyn ComuTicketPort>,
    policy: Arc<ComuPolicyService>,
    grants: Mutex<HashMap<String, SessionGrants>>,
    prompt_locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl ComuConsentGate {
    #[must_use]
    pub fn new(tickets: Arc<dyn ComuTicketPort>, policy: Arc<ComuPolicyService>) -> Self {
        Self {
            tickets,
            policy,
            grants: Mutex::new(HashMap::new()),
            prompt_locks: Mutex::new(HashMap::new()),
        }
    }

    fn session_prompt_lock(&self, session_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.prompt_locks.lock();
        Arc::clone(
            map.entry(session_id.to_string())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    /// Cancel any open Comu consent prompt for `session_id` (leaving Browser tickets intact).
    pub fn cancel_pending_prompts(&self, session_id: &str) {
        self.tickets.cancel_comu_session(session_id);
    }

    /// Revoke all in-memory grants and cancel open Comu consent cards for `session_id`.
    pub fn revoke_session(&self, session_id: &str) {
        self.grants.lock().remove(session_id);
        self.prompt_locks.lock().remove(session_id);
        self.tickets.cancel_comu_session(session_id);
    }

    /// Authorize `tool_name` + `args` against `ComuPolicyService` and session grants,
    /// raising `HostTickets` prompts within one shared `CONSENT_WAIT_TIMEOUT` deadline.
    pub async fn authorize(
        &self,
        session_id: &str,
        project_id: &str,
        tool_name: &str,
        args: &Value,
        windows: &[WindowOwnerInfo],
    ) -> Result<(), ComuAuthRefusal> {
        let app_policy = self.policy.read_app_policy().await;
        if !app_policy.enabled {
            return Err(ComuAuthRefusal::Disabled);
        }

        if is_discovery_only(tool_name) {
            return Ok(());
        }

        let deadline = Instant::now() + CONSENT_WAIT_TIMEOUT;
        let lock = self.session_prompt_lock(session_id);
        let _guard = lock.lock().await;

        // 1. Clipboard plain-text read check, also for reads inside `comu_run_steps`.
        if app_policy.ask_before_clipboard_read && reads_clipboard_text(tool_name, args, 0) {
            self.prompt_once(
                session_id,
                project_id,
                "System Clipboard",
                "clipboard",
                ComuConsentAction::ClipboardRead,
                tool_name,
                &ConsentOption::ONCE_OR_DENY,
                deadline,
            )
            .await?;
            if !is_batch(tool_name) {
                return Ok(());
            }
        }

        // 2. Foreground / whole-desktop check.
        if requires_foreground_or_desktop(tool_name, args)
            && app_policy.ask_before_foreground_desktop
            && !self.has_foreground_grant(session_id)
        {
            let target = resolve_target_app(tool_name, args, windows);
            let app_label = target
                .as_ref()
                .map(|t| t.app_name.as_str())
                .unwrap_or("Desktop");
            let app_id = target
                .as_ref()
                .map(|t| t.app_id.as_str())
                .unwrap_or("desktop");
            let decision = self
                .prompt_once(
                    session_id,
                    project_id,
                    app_label,
                    app_id,
                    ComuConsentAction::ForegroundDesktop,
                    tool_name,
                    &ConsentOption::ONCE_OR_DENY,
                    deadline,
                )
                .await?;
            if decision == ConsentOption::AllowSession {
                self.grants
                    .lock()
                    .entry(session_id.to_string())
                    .or_default()
                    .foreground_desktop = true;
            }
        }

        // 3. Per-application consent check (including every step in `comu_run_steps`).
        if app_policy.ask_before_driving_apps {
            let can_persist_project = validate_project_id(project_id).is_ok();
            for target in collect_target_apps(tool_name, args, windows) {
                if self.has_app_grant(session_id, &target) {
                    continue;
                }
                let saved_name = if target.has_stable_id {
                    ""
                } else {
                    target.app_name.as_str()
                };
                if self
                    .policy
                    .is_app_allowlisted(project_id, &target.app_id, saved_name)
                    .await
                {
                    self.record_session_app_grant(session_id, &target);
                    continue;
                }
                let can_save_this_app =
                    can_persist_project && !target.app_id.starts_with("pid:");
                let options: &'static [ConsentOption] = if can_save_this_app {
                    &ConsentOption::ALL
                } else {
                    &ConsentOption::ONCE_OR_DENY
                };
                let decision = self
                    .prompt_once(
                        session_id,
                        project_id,
                        &target.app_name,
                        &target.app_id,
                        ComuConsentAction::DriveApp,
                        tool_name,
                        options,
                        deadline,
                    )
                    .await?;
                self.record_session_app_grant(session_id, &target);
                if decision == ConsentOption::AllowProject && can_save_this_app {
                    let entry = if target.has_stable_id {
                        &target.app_id
                    } else {
                        &target.app_name
                    };
                    self.policy
                        .record_project_approval(project_id, entry)
                        .await;
                }
            }
        }

        Ok(())
    }

    fn has_foreground_grant(&self, session_id: &str) -> bool {
        self.grants
            .lock()
            .get(session_id)
            .is_some_and(|g| g.foreground_desktop)
    }

    fn has_app_grant(&self, session_id: &str, target: &ResolvedAppTarget) -> bool {
        let key = AppGrantKey::from_target(target);
        self.grants
            .lock()
            .get(session_id)
            .is_some_and(|g| g.apps.contains(&key))
    }

    fn record_session_app_grant(&self, session_id: &str, target: &ResolvedAppTarget) {
        let key = AppGrantKey::from_target(target);
        self.grants
            .lock()
            .entry(session_id.to_string())
            .or_default()
            .apps
            .insert(key);
    }

    #[allow(clippy::too_many_arguments)]
    async fn prompt_once(
        &self,
        session_id: &str,
        project_id: &str,
        app_name: &str,
        app_id: &str,
        action: ComuConsentAction,
        tool_name: &str,
        options: &'static [ConsentOption],
        deadline: Instant,
    ) -> Result<ConsentOption, ComuAuthRefusal> {
        let Some(wait) = deadline.checked_duration_since(Instant::now()) else {
            return Err(ComuAuthRefusal::TimedOut);
        };
        if wait.is_zero() {
            return Err(ComuAuthRefusal::TimedOut);
        }
        let ticket = ComuTicketRequest {
            id: TicketId::mint(),
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            app_name: app_name.to_string(),
            app_id: app_id.to_string(),
            action,
            tool_name: tool_name.to_string(),
            options,
        };
        match self.tickets.request_comu(ticket, wait).await {
            TicketOutcome::Answered(ConsentOption::Deny) => Err(ComuAuthRefusal::Denied),
            TicketOutcome::Answered(opt) => Ok(opt),
            TicketOutcome::Unattended => Err(ComuAuthRefusal::Unattended),
            TicketOutcome::TimedOut => Err(ComuAuthRefusal::TimedOut),
            TicketOutcome::Cancelled => Err(ComuAuthRefusal::Cancelled),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComuAuthRefusal {
    Disabled,
    Denied,
    Unattended,
    TimedOut,
    Cancelled,
}

impl ComuAuthRefusal {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "comu_disabled",
            Self::Denied => "consent_denied",
            Self::Unattended => "consent_unattended",
            Self::TimedOut => "consent_timed_out",
            Self::Cancelled => "consent_cancelled",
        }
    }

    #[must_use]
    pub const fn message(&self) -> &'static str {
        match self {
            Self::Disabled => {
                "Troupe Comu (Computer Use) is turned off in Settings -> Agents -> Computer Use."
            }
            Self::Denied => "The user denied Computer Use permission for this action.",
            Self::Unattended => {
                "Computer Use needs interactive user approval, and no client is viewing this chat."
            }
            Self::TimedOut => "Timed out waiting for Computer Use approval from the user.",
            Self::Cancelled => "Computer Use approval was cancelled.",
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedAppTarget {
    app_id: String,
    app_name: String,
    has_stable_id: bool,
}

fn is_discovery_only(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "comu_list_apps"
            | "list_apps"
            | "comu_list_windows"
            | "list_windows"
            | "comu_get_screen_size"
            | "get_screen_size"
            | "comu_get_cursor_position"
            | "get_cursor_position"
            | "comu_check_permissions"
            | "check_permissions"
    )
}

fn requires_foreground_or_desktop(tool_name: &str, args: &Value) -> bool {
    if matches!(
        tool_name,
        "comu_bring_to_front"
            | "bring_to_front"
            | "comu_get_desktop_state"
            | "get_desktop_state"
    ) {
        return true;
    }
    if args.get("delivery_mode").and_then(Value::as_str) == Some("foreground") {
        return true;
    }
    if args.pointer("/target/kind").and_then(Value::as_str) == Some("desktop") {
        return true;
    }
    if (tool_name == "comu_run_steps" || tool_name == "run_steps")
        && (args.get("foreground_fallback").and_then(Value::as_bool) == Some(true)
            || args
                .get("steps")
                .and_then(Value::as_array)
                .is_some_and(|steps| {
                    steps.iter().any(|s| {
                        let st = s.get("tool").and_then(Value::as_str).unwrap_or_default();
                        let sa = s.get("args").unwrap_or(&Value::Null);
                        requires_foreground_or_desktop(st, sa)
                    })
                }))
    {
        return true;
    }
    false
}

const MAX_STEP_CONSENT_DEPTH: usize = 4;

fn is_batch(tool_name: &str) -> bool {
    tool_name == "comu_run_steps" || tool_name == "run_steps"
}

/// Whether the call reads clipboard plain text, directly or in a batch step.
fn reads_clipboard_text(tool_name: &str, args: &Value, depth: usize) -> bool {
    if (tool_name == "comu_clipboard_read" || tool_name == "clipboard_read")
        && args.get("include_text").and_then(Value::as_bool) == Some(true)
    {
        return true;
    }
    depth < MAX_STEP_CONSENT_DEPTH
        && is_batch(tool_name)
        && args
            .get("steps")
            .and_then(Value::as_array)
            .is_some_and(|steps| {
                steps.iter().any(|step| {
                    let st = step.get("tool").and_then(Value::as_str).unwrap_or_default();
                    let sa = step.get("args").unwrap_or(&Value::Null);
                    reads_clipboard_text(st, sa, depth + 1)
                })
            })
}

fn collect_target_apps(
    tool_name: &str,
    args: &Value,
    windows: &[WindowOwnerInfo],
) -> Vec<ResolvedAppTarget> {
    let mut out: Vec<ResolvedAppTarget> = Vec::new();
    collect_target_apps_inner(tool_name, args, windows, 0, &mut out);
    out
}

fn collect_target_apps_inner(
    tool_name: &str,
    args: &Value,
    windows: &[WindowOwnerInfo],
    depth: usize,
    out: &mut Vec<ResolvedAppTarget>,
) {
    let mut push_unique = |target: ResolvedAppTarget| {
        let key = AppGrantKey::from_target(&target);
        if !out
            .iter()
            .any(|existing| AppGrantKey::from_target(existing) == key)
        {
            out.push(target);
        }
    };
    if let Some(t) = resolve_target_app(tool_name, args, windows) {
        push_unique(t);
    }
    if depth >= MAX_STEP_CONSENT_DEPTH {
        return;
    }
    if tool_name == "comu_run_steps" || tool_name == "run_steps" {
        if let Some(steps) = args.get("steps").and_then(Value::as_array) {
            for step in steps {
                let st = step.get("tool").and_then(Value::as_str).unwrap_or_default();
                let sa = step.get("args").unwrap_or(&Value::Null);
                collect_target_apps_inner(st, sa, windows, depth + 1, out);
            }
        }
    }
}

fn resolve_target_app(
    tool_name: &str,
    args: &Value,
    windows: &[WindowOwnerInfo],
) -> Option<ResolvedAppTarget> {
    if matches!(
        tool_name,
        "comu_clipboard_read"
            | "clipboard_read"
            | "comu_clipboard_write"
            | "clipboard_write"
            | "comu_get_desktop_state"
            | "get_desktop_state"
    ) {
        return None;
    }

    if tool_name == "comu_launch_app" || tool_name == "launch_app" {
        let stable_id = ["bundle_id", "aumid", "path", "launch_path"]
            .into_iter()
            .find_map(|key| {
                args.get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            });
        let raw_name = args
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let id = stable_id.or(raw_name)?;
        let name = raw_name.unwrap_or(id);
        return Some(ResolvedAppTarget {
            app_id: id.to_string(),
            app_name: name.to_string(),
            has_stable_id: stable_id.is_some(),
        });
    }

    let wid = args
        .get("window_id")
        .or_else(|| args.pointer("/target/window_id"))
        .and_then(Value::as_i64);
    if let Some(wid) = wid {
        if let Some(win) = windows.iter().find(|w| w.window_id == wid) {
            return Some(target_from_window(win));
        }
    }

    let pid = args
        .get("pid")
        .or_else(|| args.pointer("/target/pid"))
        .and_then(Value::as_i64);
    if let Some(pid) = pid {
        if let Some(win) = windows.iter().find(|w| w.pid == pid) {
            return Some(target_from_window(win));
        }
        return Some(ResolvedAppTarget {
            app_id: format!("pid:{pid}"),
            app_name: format!("Process {pid}"),
            has_stable_id: true,
        });
    }

    None
}

fn target_from_window(win: &WindowOwnerInfo) -> ResolvedAppTarget {
    let stable_bundle = win
        .bundle_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let app_name = if win.app_name.trim().is_empty() {
        stable_bundle
            .clone()
            .unwrap_or_else(|| format!("Process {}", win.pid))
    } else {
        win.app_name.clone()
    };
    match stable_bundle {
        Some(bundle_id) => ResolvedAppTarget {
            app_id: bundle_id,
            app_name,
            has_stable_id: true,
        },
        None => ResolvedAppTarget {
            app_id: app_name.clone(),
            app_name,
            has_stable_id: false,
        },
    }
}
