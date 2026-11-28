//! Troupe Comu — local Computer Use for AI agents.
//!
//! Exposes a runtime-neutral [`ComuHost`] trait mirroring `browser_automation`:
//! the Tauri desktop app registers [`host::DesktopComuHost`] during startup
//! (serving both local desktop chats and remote shared-live clients), whereas
//! the headless standalone `troupe-server` leaves `COMU_HOST` unset so every
//! call fails closed with `capability_unavailable` and `troupe-server` links
//! zero desktop GUI / X11 / Wayland libraries.

pub mod consent;
pub mod host;
pub mod indicator;
pub mod policy;
pub mod self_guard;
pub mod stop;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};
pub use troupe_comu_engine::{
    ComuCallOutput, ComuContentBlock, OsPermissionKind, OsPermissionsStatus, COMU_TOOL_PAIRS,
    DESCRIPTION_PREFIX,
};

pub type ComuFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Runtime-neutral host capability for Troupe Comu.
pub trait ComuHost: Send + Sync {
    fn tool_definitions(&self) -> Vec<Value>;
    fn execute<'a>(
        &'a self,
        session_id: &'a str,
        agent_id: &'a str,
        tool_name: &'a str,
        args: Value,
    ) -> ComuFuture<'a, ComuCallOutput>;
    fn os_permissions_status(&self) -> OsPermissionsStatus;
    fn request_os_permission(&self, kind: OsPermissionKind) -> OsPermissionsStatus;
    fn stop(&self, _session_id: &str) -> bool {
        false
    }
    fn user_turn(&self, _session_id: &str) {}
    fn turn_ended(&self, _session_id: &str) {}
    fn session_ended(&self, _session_id: &str) {}
    fn name_agent(&self, _agent_id: &str, _name: &str) {}
    fn forget_agent(&self, _agent_id: &str) {}
}

static COMU_HOST: RwLock<Option<Arc<dyn ComuHost>>> = RwLock::new(None);

/// Register the desktop Comu host (called once from `lib.rs` setup).
pub fn set_comu_host(host: Arc<dyn ComuHost>) {
    *COMU_HOST.write().unwrap_or_else(|e| e.into_inner()) = Some(host);
}

#[must_use]
pub fn comu_host() -> Option<Arc<dyn ComuHost>> {
    COMU_HOST.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Return MCP `Tool` JSON definitions for the `comu_*` tool family.
///
/// When a desktop [`ComuHost`] is registered, returns the full driver schemas.
/// When no host is registered (`troupe-server`, unit tests), returns minimal
/// fail-closed tool definitions built purely from [`COMU_TOOL_PAIRS`] without
/// referencing any platform windowing symbols.
#[must_use]
pub fn describe_tools() -> Vec<Value> {
    match comu_host() {
        Some(host) => host.tool_definitions(),
        None => fallback_tool_definitions(),
    }
}

fn fallback_tool_definitions() -> Vec<Value> {
    COMU_TOOL_PAIRS
        .iter()
        .map(|(public_name, _)| {
            json!({
                "name": public_name,
                "description": format!(
                    "{DESCRIPTION_PREFIX}Unavailable on headless troupe-server (desktop app required)."
                ),
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": true
                }
            })
        })
        .collect()
}

/// Dispatch one `comu_*` tool call from `acp::host_mcp::parent`.
pub async fn dispatch(
    session_id: &str,
    agent_id: &str,
    tool_name: &str,
    args: Value,
) -> ComuCallOutput {
    match comu_host() {
        Some(host) => host.execute(session_id, agent_id, tool_name, args).await,
        None => no_host_error(),
    }
}

/// Fail-closed reply returned when no desktop Comu host is registered.
#[must_use]
pub fn no_host_error() -> ComuCallOutput {
    ComuCallOutput::error_text(
        "capability_unavailable",
        "Computer Use is only available on the Troupe desktop app.",
    )
}

/// Snapshot OS-level desktop permissions (`None` on headless `troupe-server`).
#[must_use]
pub fn os_permissions_status() -> Option<OsPermissionsStatus> {
    comu_host().map(|host| host.os_permissions_status())
}

/// Trigger a user-initiated OS permission request (`None` on headless `troupe-server`).
#[must_use]
pub fn request_os_permission(kind: OsPermissionKind) -> Option<OsPermissionsStatus> {
    comu_host().map(|host| host.request_os_permission(kind))
}

/// Latch Stop for `session_id` (`None` on headless `troupe-server`).
#[must_use]
pub fn stop_session(session_id: &str) -> Option<bool> {
    comu_host().map(|host| host.stop(session_id))
}

/// Clear Stop on the user's next prompt turn.
pub fn user_turn(session_id: &str) {
    if let Some(host) = comu_host() {
        host.user_turn(session_id);
    }
}

/// Hide the virtual agent cursor when a prompt turn ends.
pub fn turn_ended(session_id: &str) {
    if let Some(host) = comu_host() {
        host.turn_ended(session_id);
    }
}

/// Clean up session grants, Stop latch, and driver session on chat close.
pub fn session_ended(session_id: &str) {
    if let Some(host) = comu_host() {
        host.session_ended(session_id);
    }
}

/// Record an agent's human-readable display name for virtual cursor badges.
pub fn name_agent(agent_id: &str, name: &str) {
    if let Some(host) = comu_host() {
        host.name_agent(agent_id, name);
    }
}

/// Drop an agent's display name when it exits.
pub fn forget_agent(agent_id: &str) {
    if let Some(host) = comu_host() {
        host.forget_agent(agent_id);
    }
}

#[cfg(test)]
pub fn clear_comu_host() {
    *COMU_HOST.write().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(test)]
pub(crate) static TEST_COMU_HOST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests;
