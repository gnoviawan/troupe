//! What a step asks the world to do: the [`Effect`] the app runs.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use super::FlightId;
use super::auth_retry::{AuthRetryId, OnDemand};
use super::ensure::EnsureId;
use super::mcp::TokenCheck;
use super::restore::RunId;
use super::turn::TurnEnd;
use super::types::Object;
use super::window::PayloadRead;
use crate::Level;

/// What a step asks the world to do. The caller runs these in order.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Effect {
    /// A boundary log line for the host log.
    Log {
        level: Level,
        source: String,
        message: String,
    },
    /// Write the session's index entry and payload (`persistSession`).
    PersistSession { session_id: String },
    /// Bring the session's chat tab forward if another tab in its pane
    /// hides it, so a consent card is on screen.
    RevealChatTab { session_id: String },
    /// Send `Event::TurnEnd` with this close once the events already queued
    /// have been applied, so chunks racing the terminator join the turn.
    EndTurn(TurnEnd),
    /// Send the prompt command with this turn id through the client's
    /// ledger, then send `Event::PromptSettled` with its outcome.
    DispatchPrompt {
        agent_id: String,
        session_id: String,
        turn_id: String,
        blocks: Vec<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_blocks: Option<Vec<Value>>,
    },
    /// Send `session/cancel` for the session's turn or the agent's own work.
    CancelPrompt {
        agent_id: String,
        session_id: String,
    },
    /// The action the user took failed; show this.
    ActionRejected { message: String },
    /// Send `Event::QueueRetryDue` with this generation after `delay_ms`.
    RetryQueueAfter {
        session_id: String,
        delay_ms: f64,
        #[serde(skip_serializing)]
        generation: u64,
    },
    /// Save the config's model list, so the pickers show it after a restart.
    PersistModelCatalog { config_id: String, catalog: Value },
    /// Send `session/set_mode`, then `Event::ControlSet` with this effect.
    SetMode {
        agent_id: String,
        session_id: String,
        mode_id: String,
        /// Sent again after an authenticate on demand (`withAuthRetry`).
        #[serde(skip_serializing)]
        retried: bool,
    },
    /// Send `session/set_model`, then `Event::ControlSet` with this effect.
    SetModel {
        agent_id: String,
        session_id: String,
        model_id: String,
        #[serde(skip_serializing)]
        persist: bool,
        /// The id of the session's model-category option when it was sent.
        #[serde(skip_serializing)]
        model_option: Option<Value>,
        /// The restore that sent it, which its outcome moves on.
        #[serde(skip_serializing)]
        run: Option<RunId>,
        /// Sent again after an authenticate on demand (`withAuthRetry`).
        #[serde(skip_serializing)]
        retried: bool,
    },
    /// Send `session/set_config_option`, then `Event::ControlSet` with this
    /// effect and the agent's option snapshot.
    SetConfigOption {
        agent_id: String,
        session_id: String,
        config_id: String,
        value: Value,
        #[serde(skip_serializing)]
        persist: bool,
        /// The model the session ran when the value was sent.
        #[serde(skip_serializing)]
        model_at_request: Option<String>,
        #[serde(skip_serializing)]
        is_model: bool,
        #[serde(skip_serializing)]
        run: Option<RunId>,
    },
    /// Free the session's transcript once the `PersistSession` before this
    /// effect has read it: send `Event::DropSessionTranscript` before the
    /// next event.
    DropSessionTranscript { session_id: String },
    /// Close the chat tab of this session in every project's tree: the chat
    /// is neither live nor in the index, so the tab could show nothing.
    CloseChatTab { session_id: String },
    /// Merge this patch into the config's saved composer picks.
    PersistComposerOptions { config_id: String, patch: Value },
    /// Reload the chat index from the desktop's history store, then send
    /// what it read as `applySessionIndexSnapshot`.
    RefreshSessionIndex,
    /// Read the user's saved picks for the config, then send
    /// `Event::ComposerOptionsRead` with this effect and the record.
    ReadComposerOptions {
        config_id: String,
        #[serde(skip_serializing)]
        run: RunId,
    },
    /// Start an agent process for the config; then `Event::AgentCommandDone`
    /// with the spawn reply, or `AgentCommandFailed`.
    SpawnAgent {
        config: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        project_id: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        /// The `ensureLiveAgent` waiting on this spawn.
        #[serde(skip_serializing)]
        ensure: Option<EnsureId>,
    },
    /// Stop the agent process; then `AgentCommandDone` or `AgentCommandFailed`.
    KillAgent {
        agent_id: String,
        /// The `ensureLiveAgent` whose spawn this kill cleans up.
        #[serde(skip_serializing)]
        ensure: Option<EnsureId>,
    },
    /// Send `authenticate`; then `AgentCommandDone` or `AgentCommandFailed`.
    Authenticate {
        agent_id: String,
        method_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
        #[serde(skip_serializing)]
        flight: FlightId,
        /// Sent by `authenticateBeforeSession`, not by a click.
        #[serde(skip_serializing)]
        on_demand: Option<OnDemand>,
    },
    /// Sign the agent out; then `AgentCommandDone` or `AgentCommandFailed`.
    Logout { agent_id: String },
    /// Answer the agent's own sign-in prompt; then `AgentCommandDone` or
    /// `AgentCommandFailed`.
    RespondElicitation {
        agent_id: String,
        request_id: String,
        action: String,
    },
    /// Save the remembered auth method per config.
    PersistAuthMethods { methods: BTreeMap<String, String> },
    /// Show a toast; `level` is `error`, `warning` or `success`.
    Toast { level: String, message: String },
    /// Ask the browser policy views to re-read: a hint may have been lost.
    RequestBrowserPolicyResync,
    /// Hand a `browser_policy_changed` hint to the policy views.
    PublishBrowserPolicyChange { payload: Value },
    /// Ask the automation views to re-read: a change may have been lost.
    RequestAutomationResync,
    /// Hand an `automation_changed` hint to the automation views.
    PublishAutomationChange { payload: Value },
    /// The session's agent drives this browser tab: its Stop stops the tab.
    TrackAgentTab { tab_id: String, session_id: String },
    /// No session drives this browser tab any more.
    UntrackAgentTab { tab_id: String },
    /// Open the agent's browser tab in the workspace (desktop only).
    OpenAgentBrowserTab {
        tab_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// Close the agent's browser tab and its browser session (desktop only).
    CloseAgentBrowserTab { tab_id: String },
    /// End the agent's session; then `Event::SessionCommandDone` or
    /// `SessionCommandFailed`.
    CloseAgentSession {
        agent_id: String,
        session_id: String,
    },
    /// Delete the files the app staged for the session (pasted images).
    DeleteSessionTempFiles { session_id: String },
    /// Ask the agent to delete its own session (`session/delete`); then
    /// `SessionCommandDone` or `SessionCommandFailed`.
    DeleteAgentSession {
        agent_id: String,
        session_id: String,
    },
    /// Delete the session's durable history and index row; then
    /// `SessionCommandDone` or `SessionCommandFailed`.
    DeleteSessionPayload { session_id: String },
    /// Read the MCP registry, then send `Event::McpCommandDone` with the
    /// list as the app normalized it, or `McpCommandFailed`.
    LoadMcpServers {
        #[serde(skip_serializing)]
        reload: bool,
    },
    /// Write the MCP registry; then `McpCommandDone` or `McpCommandFailed`.
    SaveMcpServers { servers: Vec<Object> },
    /// Watch the shared MCP registry key; send each change as
    /// `Event::McpSharedChanged`.
    WatchSharedMcpServers,
    /// Probe the server's config; then `McpCommandDone` with the probe
    /// result, or `McpCommandFailed`.
    ProbeMcpServer {
        config: Object,
        #[serde(skip_serializing)]
        server: Object,
        #[serde(skip_serializing)]
        deletions: u64,
    },
    /// Run an OAuth sign-in; send `Event::McpOAuthAuthUrl` once a web flow
    /// knows its link, then `McpCommandDone` once the token is stored, or
    /// `McpCommandFailed`.
    StartMcpOAuth {
        project_id: String,
        url: String,
        #[serde(skip_serializing)]
        server_id: String,
        #[serde(skip_serializing)]
        server_name: String,
        #[serde(skip_serializing)]
        flow: u64,
        #[serde(skip_serializing)]
        deletions: u64,
    },
    /// Stop the running OAuth sign-in for this URL; the model already ended
    /// it, so no answer is needed.
    AbortMcpOAuth { url: String },
    /// Hand the pasted OAuth callback to the host; then `McpCommandDone`
    /// or `McpCommandFailed` with the `McpOAuthError` code.
    CompleteMcpOAuth {
        project_id: String,
        callback_url: String,
        #[serde(skip_serializing)]
        server_id: String,
        #[serde(skip_serializing)]
        server_name: String,
        #[serde(skip_serializing)]
        server_url: String,
    },
    /// Whether a token is stored for the URL; then `McpCommandDone` with
    /// the answer, or `McpCommandFailed`.
    HasMcpOAuthToken {
        project_id: String,
        url: String,
        #[serde(skip_serializing)]
        check: TokenCheck,
    },
    /// Delete the stored token; then `McpCommandDone` or `McpCommandFailed`.
    DisconnectMcpOAuth {
        project_id: String,
        url: String,
        #[serde(skip_serializing)]
        server_id: String,
        #[serde(skip_serializing)]
        server_name: String,
        #[serde(skip_serializing)]
        deletions: u64,
    },
    /// Answer the agent's MCP install request; then `McpCommandDone` or
    /// `McpCommandFailed`.
    AnswerMcpInstall {
        request: Value,
        #[serde(skip_serializing)]
        decision: String,
    },
    /// Read the session's durable payload (the cache, else the history
    /// store), then send `Event::SessionPayloadRead` with this effect and
    /// the payload, `None` when the session has none, or the error.
    ReadSessionPayload {
        session_id: String,
        #[serde(skip_serializing)]
        read: PayloadRead,
    },
    /// Whether the host stores a Factory API key; then
    /// `Event::FactoryKeyStatus` with this effect and the answer, or false
    /// with `failed` when it could not be read.
    ReadFactoryKeyStatus {
        #[serde(skip_serializing)]
        retry: AuthRetryId,
    },
    /// Send `Event::ReadvertiseWaitOver` with `run` after `delay_ms`, unless
    /// the agent re-advertised the session's options first.
    WaitForReadvertisedOptions {
        session_id: String,
        delay_ms: f64,
        #[serde(skip_serializing)]
        run: RunId,
    },
}

impl Effect {
    pub(crate) fn warn(source: &str, message: String) -> Self {
        Self::Log {
            level: Level::Warn,
            source: source.to_owned(),
            message,
        }
    }

    /// A set command sent again after an authenticate on demand.
    pub(crate) fn retried(&self) -> bool {
        matches!(
            self,
            Self::SetMode { retried: true, .. } | Self::SetModel { retried: true, .. }
        )
    }

    /// The restore a set command belongs to.
    pub(crate) fn run(&self) -> Option<RunId> {
        match self {
            Self::SetModel { run, .. } | Self::SetConfigOption { run, .. } => *run,
            _ => None,
        }
    }

    pub(crate) fn error(source: &str, message: String) -> Self {
        Self::Log {
            level: Level::Error,
            source: source.to_owned(),
            message,
        }
    }

    /// The user's action failed with this error message; shown the way
    /// `String(error)` prints an `Error`.
    pub(crate) fn rejected(message: impl std::fmt::Display) -> Self {
        Self::ActionRejected {
            message: format!("Error: {message}"),
        }
    }

    pub(crate) fn info(source: &str, message: String) -> Self {
        Self::Log {
            level: Level::Info,
            source: source.to_owned(),
            message,
        }
    }
}
