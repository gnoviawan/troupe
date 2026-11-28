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
use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;
use troupe_comu_engine::{
    ComuCallOutput, ComuContentBlock, ComuEngine, ComuEngineOptions, OsPermissionKind,
    OsPermissionsStatus, WindowOwnerInfo,
};

use crate::web::EventSink;

use super::consent::{ComuAuthRefusal, ComuConsentGate, ComuTicketPort};
use super::indicator::{ComuScreenIndicator, IndicatorState};
use super::policy::ComuPolicyService;
use super::self_guard::check_not_self_target;
use super::stop::{ComuStopRegistry, ComuStopToken};
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

struct CursorCoordinators {
    generations: Mutex<HashMap<String, u64>>,
    locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Default for CursorCoordinators {
    fn default() -> Self {
        Self {
            generations: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
        }
    }
}

impl CursorCoordinators {
    fn bump(&self, session_id: &str) -> u64 {
        let mut map = self.generations.lock();
        let entry = map.entry(session_id.to_string()).or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    fn current(&self, session_id: &str) -> u64 {
        self.generations.lock().get(session_id).copied().unwrap_or(0)
    }

    fn lock_for(&self, session_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.locks.lock();
        Arc::clone(
            map.entry(session_id.to_string())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn forget(&self, session_id: &str) {
        self.generations.lock().remove(session_id);
        self.locks.lock().remove(session_id);
    }
}

pub struct DesktopComuHost {
    backend: Arc<dyn ComuBackend>,
    policy: Arc<ComuPolicyService>,
    consent: ComuConsentGate,
    stop: ComuStopRegistry,
    project_of: ProjectResolver,
    agent_names: Mutex<HashMap<String, String>>,
    cursor_coord: Arc<CursorCoordinators>,
    indicator: IndicatorState,
    runtime: Option<tokio::runtime::Handle>,
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
        let runtime = tokio::runtime::Handle::try_current()
            .ok()
            .or_else(|| Some(tauri::async_runtime::handle().inner().clone()));
        Ok(Self::build(
            engine,
            sinks,
            tickets,
            policy,
            project_of,
            runtime,
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
        let runtime = tokio::runtime::Handle::try_current().ok();
        Self::build(
            backend,
            sinks,
            tickets,
            policy,
            project_of,
            runtime,
            own_pid,
        )
    }

    fn build(
        backend: Arc<dyn ComuBackend>,
        sinks: Vec<Arc<dyn EventSink>>,
        tickets: Arc<dyn ComuTicketPort>,
        policy: Arc<ComuPolicyService>,
        project_of: ProjectResolver,
        runtime: Option<tokio::runtime::Handle>,
        own_pid: u32,
    ) -> Arc<Self> {
        Arc::new(Self {
            backend,
            consent: ComuConsentGate::new(tickets, Arc::clone(&policy)),
            policy,
            stop: ComuStopRegistry::new(sinks),
            project_of,
            agent_names: Mutex::new(HashMap::new()),
            cursor_coord: Arc::new(CursorCoordinators::default()),
            indicator: IndicatorState::default(),
            runtime,
            own_pid,
        })
    }

    /// Install the desktop screen-edge indicator (`lib.rs` setup). Only the
    /// first call takes effect.
    pub fn install_screen_indicator(&self, port: Arc<dyn ComuScreenIndicator>) {
        self.indicator.install(port);
    }

    /// `session_id` starts acting on the computer: show the chat banner and
    /// the screen-edge indicator.
    async fn begin_driving(
        &self,
        session_id: &str,
        agent_id: &str,
        stop_token: &mut ComuStopToken,
    ) {
        self.stop.mark_active(session_id);
        // Read now: the user may have changed it during the consent wait.
        let enabled = self
            .policy
            .read_app_policy()
            .await
            .screen_indicator_enabled;
        self.indicator
            .session_driving(session_id, &self.agent_badge(agent_id), enabled);
        // Stop releases the session; one that landed during the read above
        // released it before this registration.
        if stop_token.is_stopped() {
            self.indicator.session_released(session_id);
        }
    }

    fn agent_badge(&self, agent_id: &str) -> String {
        self.agent_names
            .lock()
            .get(agent_id)
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Troupe Agent".to_string())
    }

    fn spawn_host_task<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(future);
            return;
        }
        if let Some(handle) = &self.runtime {
            handle.spawn(future);
            return;
        }
        // Synchronous teardown outside any Tokio context: run on a short-lived worker thread
        // so backend session cleanup is never silently dropped.
        std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(future);
            }
        });
    }

    fn spawn_cursor_disable(&self, session_id: &str, expected_gen: u64) {
        let backend = Arc::clone(&self.backend);
        let coord = Arc::clone(&self.cursor_coord);
        let sid = session_id.to_string();
        self.spawn_host_task(async move {
            let lock = coord.lock_for(&sid);
            let _guard = lock.lock().await;
            if coord.current(&sid) == expected_gen {
                backend.configure_session_cursor(&sid, false, None).await;
            }
        });
    }

    async fn dispatch_checked(
        &self,
        session_id: &str,
        agent_id: &str,
        tool_name: &str,
        args: Value,
        stop_token: &mut ComuStopToken,
    ) -> Option<ComuCallOutput> {
        if stop_token.is_stopped() {
            return None;
        }
        if is_batch_call(tool_name, &args) {
            return self
                .dispatch_multi_steps_checked(session_id, agent_id, args, stop_token)
                .await;
        }
        tokio::select! {
            biased;
            () = stop_token.wait_stopped() => None,
            out = async { self.backend.call_tool(session_id, tool_name, args).await } => Some(out),
        }
    }

    /// Execute `comu_run_steps` one step at a time, normalizing raw step names,
    /// rejecting nested `run_steps`, re-validating `self_guard` against fresh
    /// window bounds before each step, and preserving per-step structured/error/image outputs.
    async fn dispatch_multi_steps_checked(
        &self,
        session_id: &str,
        agent_id: &str,
        args: Value,
        stop_token: &mut ComuStopToken,
    ) -> Option<ComuCallOutput> {
        if let Err(out) = validate_run_steps_args(&args) {
            return Some(out);
        }
        let stop_on_error = args
            .get("stop_on_error")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let inherited_pid = args.get("pid").cloned();
        let inherited_wid = args.get("window_id").cloned();
        let steps = args
            .get("steps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut results = Vec::with_capacity(steps.len());
        let mut extra_blocks: Vec<ComuContentBlock> = Vec::new();
        // The batch drives the computer from its first acting step on.
        let mut driving = false;
        let mut all_ok = true;
        for (idx, step) in steps.into_iter().enumerate() {
            if stop_token.is_stopped() {
                return None;
            }
            let raw_step_tool = step
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(step_tool) = resolve_public_step_tool(raw_step_tool) else {
                return Some(ComuCallOutput::error_text(
                    "disallowed_step_tool",
                    format!("step {idx} references disallowed or unknown tool `{raw_step_tool}`"),
                ));
            };
            let mut merged = step
                .get("args")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            if let Some(obj) = merged.as_object_mut() {
                if !obj.contains_key("pid") {
                    if let Some(p) = &inherited_pid {
                        obj.insert("pid".to_string(), p.clone());
                    }
                }
                if !obj.contains_key("window_id") {
                    if let Some(w) = &inherited_wid {
                        obj.insert("window_id".to_string(), w.clone());
                    }
                }
            }

            if needs_window_snapshot(step_tool) {
                let fresh_windows = self.backend.list_windows_snapshot(session_id).await;
                if let Err(reason) = check_not_self_target(
                    self.own_pid,
                    step_tool,
                    &merged,
                    &fresh_windows,
                    &self.indicator.window_ids(),
                ) {
                    return Some(ComuCallOutput::error_text("self_target_blocked", reason));
                }
            }

            if !driving && !is_passive_probe(step_tool) {
                self.begin_driving(session_id, agent_id, stop_token).await;
                driving = true;
            }
            if stop_token.is_stopped() {
                return None;
            }
            let step_out = tokio::select! {
                biased;
                () = stop_token.wait_stopped() => return None,
                out = async { self.backend.call_tool(session_id, step_tool, merged).await } => out,
            };
            let ok = step_out.ok;
            if !ok {
                all_ok = false;
                if stop_on_error {
                    return Some(step_out);
                }
            }

            let error_texts: Vec<String> = if ok {
                Vec::new()
            } else {
                step_out
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        ComuContentBlock::Text { text } => Some(text.clone()),
                        ComuContentBlock::Image { .. } => None,
                    })
                    .collect()
            };
            let mut entry = json!({
                "step": idx,
                "tool": step_tool,
                "ok": ok,
                "structured": step_out.structured.clone(),
            });
            if !ok && !error_texts.is_empty() {
                entry["error"] = json!(error_texts.join("\n"));
            }
            results.push(entry);

            for block in step_out.blocks {
                match block {
                    ComuContentBlock::Image { .. } => extra_blocks.push(block),
                    ComuContentBlock::Text { text } if !ok => {
                        extra_blocks.push(ComuContentBlock::Text {
                            text: format!("step {idx} ({step_tool}): {text}"),
                        });
                    }
                    ComuContentBlock::Text { .. } => {}
                }
            }
        }

        let summary = json!({
            "status": if all_ok { "completed" } else { "partial" },
            "steps": results,
        });
        let mut blocks = Vec::with_capacity(1 + extra_blocks.len());
        blocks.push(ComuContentBlock::Text {
            text: summary.to_string(),
        });
        blocks.extend(extra_blocks);
        Some(ComuCallOutput {
            ok: all_ok,
            blocks,
            structured: Some(summary),
        })
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

        if (tool_name == "comu_run_steps" || tool_name == "run_steps")
            && args.get("steps").is_some_and(Value::is_array)
        {
            if let Err(out) = validate_run_steps_args(&args) {
                log::warn!(
                    "[comu] rejected invalid comu_run_steps batch: agent={agent_id} session={redacted_sid}"
                );
                return out;
            }
        }

        let windows = if needs_window_snapshot(tool_name) {
            self.backend.list_windows_snapshot(session_id).await
        } else {
            Vec::new()
        };

        if let Err(reason) = check_not_self_target(
            self.own_pid,
            tool_name,
            &args,
            &windows,
            &self.indicator.window_ids(),
        ) {
            log::warn!(
                "[comu] blocked self-target: tool={tool_name} agent={agent_id} session={redacted_sid}"
            );
            return ComuCallOutput::error_text("self_target_blocked", reason);
        }

        let project_id = (self.project_of)(session_id);
        let auth_outcome = tokio::select! {
            biased;
            () = stop_token.wait_stopped() => None,
            res = self.consent.authorize(session_id, &project_id, tool_name, &args, &windows) => Some(res),
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

        // Re-validate self-target protection against current window bounds after consent
        // so window movement while the consent card was open cannot hit Troupe.
        if needs_window_snapshot(tool_name) {
            let post_windows = self.backend.list_windows_snapshot(session_id).await;
            if let Err(reason) = check_not_self_target(
                self.own_pid,
                tool_name,
                &args,
                &post_windows,
                &self.indicator.window_ids(),
            ) {
                log::warn!(
                    "[comu] blocked self-target post-consent: tool={tool_name} agent={agent_id} session={redacted_sid}"
                );
                return ComuCallOutput::error_text("self_target_blocked", reason);
            }
        }

        if stop_token.is_stopped() {
            return stopped_output();
        }

        // A batch starts driving at its first acting step.
        if !is_batch_call(tool_name, &args) && !is_passive_probe(tool_name) {
            self.begin_driving(session_id, agent_id, &mut stop_token)
                .await;
        }

        if uses_agent_cursor(tool_name) {
            let my_gen = self.cursor_coord.bump(session_id);
            let badge = self.agent_badge(agent_id);
            let lock = self.cursor_coord.lock_for(session_id);
            let _guard = lock.lock().await;
            if self.cursor_coord.current(session_id) == my_gen && !stop_token.is_stopped() {
                self.backend
                    .configure_session_cursor(
                        session_id,
                        app_policy.cursor_enabled,
                        Some(badge.as_str()),
                    )
                    .await;
            }
        }

        if stop_token.is_stopped() {
            log::info!(
                "[comu] stopped before backend dispatch: tool={tool_name} agent={agent_id} session={redacted_sid}"
            );
            return stopped_output();
        }

        let exec_outcome = self
            .dispatch_checked(session_id, agent_id, tool_name, args, &mut stop_token)
            .await;

        match exec_outcome {
            Some(out) => {
                log::info!(
                    "[comu] completed: tool={tool_name} ok={} agent={agent_id} session={redacted_sid}",
                    out.ok
                );
                out
            }
            None => {
                let gen_now = self.cursor_coord.bump(session_id);
                let lock = self.cursor_coord.lock_for(session_id);
                let _guard = lock.lock().await;
                if self.cursor_coord.current(session_id) == gen_now {
                    self.backend
                        .configure_session_cursor(session_id, false, None)
                        .await;
                }
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
        self.indicator.session_released(session_id);
        self.consent.cancel_pending_prompts(session_id);
        let gen_at_stop = self.cursor_coord.bump(session_id);
        self.spawn_cursor_disable(session_id, gen_at_stop);
        log::info!(
            "[comu] Stop latched for session {} (newly_stopped={newly_stopped})",
            crate::logging::redact_session_id(session_id)
        );
        newly_stopped
    }

    fn user_turn(&self, session_id: &str) {
        self.cursor_coord.bump(session_id);
        self.stop.resume_on_user_turn(session_id);
    }

    fn turn_ended(&self, session_id: &str) {
        self.stop.mark_inactive(session_id);
        self.indicator.session_released(session_id);
        let gen_at_end = self.cursor_coord.current(session_id);
        self.spawn_cursor_disable(session_id, gen_at_end);
    }

    fn session_ended(&self, session_id: &str) {
        self.stop.forget_session(session_id);
        self.indicator.session_released(session_id);
        self.consent.revoke_session(session_id);
        self.cursor_coord.forget(session_id);
        let backend = Arc::clone(&self.backend);
        let sid = session_id.to_string();
        self.spawn_host_task(async move {
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
            | "get_screen_size"
            | "comu_get_cursor_position"
            | "get_cursor_position"
            | "comu_check_permissions"
            | "check_permissions"
            | "comu_list_apps"
            | "list_apps"
            | "comu_clipboard_read"
            | "clipboard_read"
            | "comu_clipboard_write"
            | "clipboard_write"
    )
}

fn is_batch_call(tool_name: &str, args: &Value) -> bool {
    (tool_name == "comu_run_steps" || tool_name == "run_steps")
        && args.get("steps").is_some_and(Value::is_array)
}

fn is_passive_probe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "comu_check_permissions"
            | "check_permissions"
            | "comu_get_screen_size"
            | "get_screen_size"
            | "comu_get_cursor_position"
            | "get_cursor_position"
            | "comu_list_apps"
            | "list_apps"
            | "comu_list_windows"
            | "list_windows"
    )
}

fn uses_agent_cursor(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "comu_click"
            | "click"
            | "comu_double_click"
            | "double_click"
            | "comu_right_click"
            | "right_click"
            | "comu_drag"
            | "drag"
            | "comu_scroll"
            | "scroll"
            | "comu_move_cursor"
            | "move_cursor"
            | "comu_type_text"
            | "type_text"
            | "comu_press_key"
            | "press_key"
            | "comu_hotkey"
            | "hotkey"
            | "comu_run_steps"
            | "run_steps"
    )
}

/// Normalize a step tool name (either `comu_*` or raw driver name) to its curated
/// public `comu_*` tool name, rejecting nested `run_steps`/`comu_run_steps` and unknown tools.
fn resolve_public_step_tool(step_tool: &str) -> Option<&'static str> {
    if step_tool == "comu_run_steps" || step_tool == "run_steps" {
        return None;
    }
    super::COMU_TOOL_PAIRS
        .iter()
        .find_map(|(public_name, raw_name)| {
            (*public_name == step_tool || *raw_name == step_tool).then_some(*public_name)
        })
}

fn validate_run_steps_args(args: &Value) -> Result<(), ComuCallOutput> {
    let Some(steps) = args.get("steps").and_then(Value::as_array) else {
        return Ok(());
    };
    for (idx, step) in steps.iter().enumerate() {
        let Some(step_obj) = step.as_object() else {
            return Err(ComuCallOutput::error_text(
                "invalid_arguments",
                format!("step {idx} must be an object"),
            ));
        };
        let Some(tool_name) = step_obj.get("tool").and_then(Value::as_str) else {
            return Err(ComuCallOutput::error_text(
                "invalid_arguments",
                format!("step {idx} is missing a string `tool` name"),
            ));
        };
        if resolve_public_step_tool(tool_name).is_none() {
            return Err(ComuCallOutput::error_text(
                "disallowed_step_tool",
                format!("step {idx} references disallowed or unknown tool `{tool_name}`"),
            ));
        }
    }
    Ok(())
}
