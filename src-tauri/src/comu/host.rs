//! Desktop host implementation for Troupe Comu.
//!
//! Coordinates policy checks, Stop latches, self-PID/window protection,
//! per-app `HostTickets` consent, virtual agent cursor configuration, and
//! execution through a [`ComuBackend`] port.
//!
//! Boundary logging only: logs tool names, outcome codes, agent IDs, and
//! redacted session IDs — never typed text, clipboard contents, window titles,
//! or screenshots (CWE-532).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use troupe_comu_engine::{
    ComuCallOutput, ComuEngine, ComuEngineOptions, OsPermissionKind, OsPermissionsStatus,
    WindowOwnerInfo,
};

use crate::web::EventSink;

use super::consent::{ComuAuthRefusal, ComuConsentGate, ComuTicketPort};
use super::policy::ComuPolicyService;
use super::self_guard::check_not_self_target;
use super::stop::ComuStopRegistry;
use super::{ComuFuture, ComuHost};

/// Resolves the owning project ID for a bound ACP `session_id`.
pub type ProjectResolver = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Port over `troupe_comu_engine::ComuEngine` so unit tests can run deterministically
/// on headless hosts without touching native windowing APIs.
pub trait ComuBackend: Send + Sync {
    fn definitions(&self) -> Vec<Value>;
    fn call_tool<'a>(
        &'a self,
        session_id: &'a str,
        tool_name: &'a str,
        args: Value,
    ) -> ComuFuture<'a, ComuCallOutput>;
    fn list_windows_snapshot<'a>(
        &'a self,
        session_id: &'a str,
    ) -> ComuFuture<'a, Vec<WindowOwnerInfo>>;
    fn configure_session_cursor<'a>(
        &'a self,
        session_id: &'a str,
        enabled: bool,
        badge_label: Option<&'a str>,
    ) -> ComuFuture<'a, ()>;
    fn end_session<'a>(&'a self, session_id: &'a str) -> ComuFuture<'a, ()>;
    fn check_os_permissions(&self) -> OsPermissionsStatus;
    fn request_os_permission(&self, kind: OsPermissionKind) -> OsPermissionsStatus;
}

impl ComuBackend for ComuEngine {
    fn definitions(&self) -> Vec<Value> {
        ComuEngine::definitions(self)
    }

    fn call_tool<'a>(
        &'a self,
        session_id: &'a str,
        tool_name: &'a str,
        args: Value,
    ) -> ComuFuture<'a, ComuCallOutput> {
        Box::pin(ComuEngine::call_tool(self, session_id, tool_name, args))
    }

    fn list_windows_snapshot<'a>(
        &'a self,
        session_id: &'a str,
    ) -> ComuFuture<'a, Vec<WindowOwnerInfo>> {
        Box::pin(ComuEngine::list_windows_snapshot(self, session_id))
    }

    fn configure_session_cursor<'a>(
        &'a self,
        session_id: &'a str,
        enabled: bool,
        badge_label: Option<&'a str>,
    ) -> ComuFuture<'a, ()> {
        Box::pin(ComuEngine::configure_session_cursor(
            self,
            session_id,
            enabled,
            badge_label,
        ))
    }

    fn end_session<'a>(&'a self, session_id: &'a str) -> ComuFuture<'a, ()> {
        Box::pin(ComuEngine::end_session(self, session_id))
    }

    fn check_os_permissions(&self) -> OsPermissionsStatus {
        troupe_comu_engine::check_os_permissions()
    }

    fn request_os_permission(&self, kind: OsPermissionKind) -> OsPermissionsStatus {
        troupe_comu_engine::request_os_permission(kind)
    }
}

pub struct DesktopComuHost {
    backend: Arc<dyn ComuBackend>,
    policy: Arc<ComuPolicyService>,
    consent: ComuConsentGate,
    stop: ComuStopRegistry,
    project_of: ProjectResolver,
    agent_names: Mutex<HashMap<String, String>>,
    own_pid: u32,
}

impl DesktopComuHost {
    /// Construct the production desktop Comu host backed by [`ComuEngine`].
    pub fn new(
        options: ComuEngineOptions,
        sinks: Vec<Arc<dyn EventSink>>,
        tickets: Arc<dyn ComuTicketPort>,
        policy: Arc<ComuPolicyService>,
        project_of: ProjectResolver,
    ) -> Result<Arc<Self>, String> {
        let engine = ComuEngine::new(options)?;
        Ok(Self::with_backend(
            engine,
            sinks,
            tickets,
            policy,
            project_of,
            std::process::id(),
        ))
    }

    /// Construct a [`DesktopComuHost`] with an explicit backend and `own_pid` (used by tests).
    #[must_use]
    pub fn with_backend(
        backend: Arc<dyn ComuBackend>,
        sinks: Vec<Arc<dyn EventSink>>,
        tickets: Arc<dyn ComuTicketPort>,
        policy: Arc<ComuPolicyService>,
        project_of: ProjectResolver,
        own_pid: u32,
    ) -> Arc<Self> {
        Arc::new(Self {
            backend,
            consent: ComuConsentGate::new(tickets, Arc::clone(&policy)),
            policy,
            stop: ComuStopRegistry::new(sinks),
            project_of,
            agent_names: Mutex::new(HashMap::new()),
            own_pid,
        })
    }

    fn agent_badge(&self, agent_id: &str) -> String {
        self.agent_names
            .lock()
            .get(agent_id)
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Troupe Agent".to_string())
    }

    async fn run_call(
        &self,
        session_id: &str,
        agent_id: &str,
        tool_name: &str,
        args: Value,
    ) -> ComuCallOutput {
        let redacted_sid = crate::logging::redact_session_id(session_id);

        if troupe_comu_engine::resolve_driver_tool_name(tool_name).is_none() {
            log::warn!(
                "[comu] rejected unknown tool: tool={tool_name} agent={agent_id} session={redacted_sid}"
            );
            return ComuCallOutput::error_text(
                "unknown_tool",
                format!("unknown Troupe Comu tool `{tool_name}`"),
            );
        }

        let app_policy = self.policy.read_app_policy().await;
        if !app_policy.enabled {
            log::info!(
                "[comu] refused (disabled): tool={tool_name} agent={agent_id} session={redacted_sid}"
            );
            return ComuCallOutput::error_text("comu_disabled", ComuAuthRefusal::Disabled.message());
        }

        let Some(mut stop_token) = self.stop.admit(session_id) else {
            log::info!(
                "[comu] refused (stopped_by_user): tool={tool_name} agent={agent_id} session={redacted_sid}"
            );
            return stopped_output();
        };

        let windows = if needs_window_snapshot(tool_name) {
            self.backend.list_windows_snapshot(session_id).await
        } else {
            Vec::new()
        };

        if let Err(reason) = check_not_self_target(self.own_pid, tool_name, &args, &windows) {
            log::warn!(
                "[comu] blocked self-target: tool={tool_name} agent={agent_id} session={redacted_sid}"
            );
            return ComuCallOutput::error_text("self_target_blocked", reason);
        }

        let project_id = (self.project_of)(session_id);
        let auth_outcome = tokio::select! {
            res = self.consent.authorize(session_id, &project_id, tool_name, &args, &windows) => Some(res),
            () = stop_token.wait_stopped() => None,
        };
        match auth_outcome {
            None => {
                log::info!(
                    "[comu] cancelled during consent (stopped_by_user): tool={tool_name} agent={agent_id} session={redacted_sid}"
                );
                return stopped_output();
            }
            Some(Err(refusal)) => {
                log::info!(
                    "[comu] consent refused: code={} tool={tool_name} agent={agent_id} session={redacted_sid}",
                    refusal.code()
                );
                return ComuCallOutput::error_text(refusal.code(), refusal.message());
            }
            Some(Ok(())) => {}
        }

        if !is_passive_probe(tool_name) {
            self.stop.mark_active(session_id);
        }

        if uses_agent_cursor(tool_name) {
            let badge = self.agent_badge(agent_id);
            self.backend
                .configure_session_cursor(
                    session_id,
                    app_policy.cursor_enabled,
                    Some(badge.as_str()),
                )
                .await;
        }

        let exec_outcome = tokio::select! {
            out = self.backend.call_tool(session_id, tool_name, args) => Some(out),
            () = stop_token.wait_stopped() => None,
        };

        match exec_outcome {
            Some(out) => {
                log::info!(
                    "[comu] completed: tool={tool_name} ok={} agent={agent_id} session={redacted_sid}",
                    out.ok
                );
                out
            }
            None => {
                let _ = self
                    .backend
                    .configure_session_cursor(session_id, false, None)
                    .await;
                log::info!(
                    "[comu] interrupted by Stop: tool={tool_name} agent={agent_id} session={redacted_sid}"
                );
                stopped_output()
            }
        }
    }
}

impl ComuHost for DesktopComuHost {
    fn tool_definitions(&self) -> Vec<Value> {
        self.backend.definitions()
    }

    fn execute<'a>(
        &'a self,
        session_id: &'a str,
        agent_id: &'a str,
        tool_name: &'a str,
        args: Value,
    ) -> ComuFuture<'a, ComuCallOutput> {
        Box::pin(self.run_call(session_id, agent_id, tool_name, args))
    }

    fn os_permissions_status(&self) -> OsPermissionsStatus {
        self.backend.check_os_permissions()
    }

    fn request_os_permission(&self, kind: OsPermissionKind) -> OsPermissionsStatus {
        log::info!("[comu] user requested OS permission prompt: {kind:?}");
        self.backend.request_os_permission(kind)
    }

    fn stop(&self, session_id: &str) -> bool {
        let newly_stopped = self.stop.stop(session_id);
        self.consent.cancel_pending_prompts(session_id);
        let backend = Arc::clone(&self.backend);
        let sid = session_id.to_string();
        spawn_detached(async move {
            backend.configure_session_cursor(&sid, false, None).await;
        });
        log::info!(
            "[comu] Stop latched for session {} (newly_stopped={newly_stopped})",
            crate::logging::redact_session_id(session_id)
        );
        newly_stopped
    }

    fn user_turn(&self, session_id: &str) {
        self.stop.resume_on_user_turn(session_id);
    }

    fn turn_ended(&self, session_id: &str) {
        let backend = Arc::clone(&self.backend);
        let sid = session_id.to_string();
        spawn_detached(async move {
            backend.configure_session_cursor(&sid, false, None).await;
        });
    }

    fn session_ended(&self, session_id: &str) {
        self.stop.forget_session(session_id);
        self.consent.revoke_session(session_id);
        let backend = Arc::clone(&self.backend);
        let sid = session_id.to_string();
        spawn_detached(async move {
            backend.end_session(&sid).await;
        });
    }

    fn name_agent(&self, agent_id: &str, name: &str) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            self.agent_names
                .lock()
                .insert(agent_id.to_string(), trimmed.to_string());
        }
    }

    fn forget_agent(&self, agent_id: &str) {
        self.agent_names.lock().remove(agent_id);
    }
}

fn spawn_detached<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
    }
}

fn stopped_output() -> ComuCallOutput {
    ComuCallOutput::error_text(
        "stopped_by_user",
        "Computer Use was stopped by the user for this turn. Wait for the user's next message before calling Computer Use tools again.",
    )
}

fn needs_window_snapshot(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "comu_get_screen_size"
            | "comu_get_cursor_position"
            | "comu_check_permissions"
            | "comu_list_apps"
            | "comu_clipboard_read"
            | "comu_clipboard_write"
    )
}

fn is_passive_probe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "comu_check_permissions"
            | "comu_get_screen_size"
            | "comu_get_cursor_position"
            | "comu_list_apps"
            | "comu_list_windows"
    )
}

fn uses_agent_cursor(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "comu_click"
            | "comu_double_click"
            | "comu_right_click"
            | "comu_drag"
            | "comu_scroll"
            | "comu_move_cursor"
            | "comu_type_text"
            | "comu_press_key"
            | "comu_hotkey"
            | "comu_run_steps"
    )
}
