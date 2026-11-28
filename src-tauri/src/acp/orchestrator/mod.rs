//! The subagent orchestrator: lets a chat's agent start other installed ACP
//! agents as subagents, each on a private ephemeral session routed under
//! the chat.
//!
//! - [`run`] holds the run types, the state machine and the ledger
//!   (pure transitions).
//! - [`engine`] owns the ledger and every side effect: spawn, announce,
//!   supervise, settle, stop.
//! - [`settings`] is the host-side truth for the feature gates and the
//!   sub-agent profiles; [`profiles`] resolves and applies the profiles,
//!   and [`agents`] lists the launchable agents.
//! - [`sinks`] holds the child's filter and watcher sinks.
//! - [`wake`] holds the per-chat wake mailbox and delivers settled sub-agent
//!   results to idle root chats.
//! - [`worktree`] snapshots the parent's state and manages the per-run
//!   worktree.
//!
//! The engine is reached through the process-wide [`installed`] handle
//! (the automation-service pattern), installed at startup by BOTH binaries
//! where the `AcpManager` is built.

mod agents;
mod engine;
mod lifecycle;
mod profiles;
mod run;
mod settings;
mod sinks;
mod wake;
mod worktree;

pub use engine::OrchestratorEngine;
#[cfg(test)]
pub(crate) use engine::engine_with_installed;
#[allow(unused_imports)]
pub use run::{
    AgentListing, CallerRef, MessageParentError, RunId, RunLookupError, RunState, RunSummary,
    StartError, StartRequest, StopError, StopOutcome, WaitOutcome,
};
#[allow(unused_imports)]
pub use profiles::{Launchable, ResolvedProfile, SubagentApprovals, SubagentProfile};
#[allow(unused_imports)]
pub use settings::{OrchestratorSettings, SettingsInput};
pub use wake::WakeSink;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tokio::runtime::Handle;

use crate::acp::install::AcpInstallService;
use crate::acp::manager::AcpManager;
use crate::web::WsRelaySink;

static INSTALLED: OnceLock<Arc<OrchestratorEngine>> = OnceLock::new();

/// Build the host event sink that wakes parent chats when their sub-agents
/// settle. Add it to `AcpManager`'s sinks before calling [`install`].
#[must_use]
pub fn wake_sink() -> Arc<WakeSink> {
    WakeSink::new()
}

/// Build and install the host's engine, and return it. The first call
/// wins.
///
/// `data_dir` holds the settings file and the known-agents registry.
/// `installed_agents` feeds the launch listing and resolution for agents
/// the host installed from the catalog. `handle` spawns the engine's tasks
/// from contexts that hold no runtime (driver threads, `Drop`).
/// `project_roots` scopes the startup sweep of abandoned subagent
/// worktrees.
pub fn install(
    manager: Arc<AcpManager>,
    relay: Arc<WsRelaySink>,
    wake_sink: Arc<WakeSink>,
    data_dir: PathBuf,
    installed_agents: Option<Arc<AcpInstallService>>,
    project_roots: Vec<PathBuf>,
    handle: Handle,
) -> Arc<OrchestratorEngine> {
    let host_plan_server = manager.host_plan_server();
    let engine = Arc::new(
        OrchestratorEngine::new(manager, data_dir, installed_agents, handle).with_relay(relay),
    );
    wake_sink.attach(&engine);
    host_plan_server.set_orchestrator(Arc::clone(&engine));
    worktree::sweep(&engine.handle, project_roots);
    if INSTALLED.set(engine.clone()).is_err() {
        log::warn!("[orchestrator] an engine was already installed");
    }
    engine
}

/// The host's engine, when the binary installed one.
pub fn installed() -> Option<Arc<OrchestratorEngine>> {
    INSTALLED.get().cloned()
}
