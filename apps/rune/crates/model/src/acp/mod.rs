//! The ACP chat model: sessions, their transcripts, plans, commands and
//! usage. Port of `src/renderer/stores/acp-store/`, one module per slice;
//! `docs/rune/acp-model.md` maps each TypeScript module to its Rust one and
//! lists what is not ported yet.
//!
//! [`step`] applies one event and returns the effects the app runs. The model
//! reads no clock and draws no randomness: [`Env`] carries both.

mod agent_commands;
#[cfg(test)]
mod agent_commands_tests;
pub mod agents;
pub mod answer;
#[cfg(test)]
mod answer_tests;
pub mod approvals;
mod auth_retry;
#[cfg(test)]
mod auth_retry_tests;
mod clamp;
mod close;
pub mod controls;
mod create;
mod diff_stat;
mod discovery;
mod effect;
mod ensure;
mod event;
mod history;
mod history_install;
mod host_error;
mod import;
#[cfg(test)]
mod import_tests;
mod index;
pub mod js;
#[cfg(test)]
mod js_tests;
mod launch;
pub mod mcp;
mod mcp_install;
mod mcp_oauth;
#[cfg(test)]
mod mcp_tests;
mod older;
#[cfg(test)]
mod older_tests;
mod oneshot;
mod oneshot_reply;
#[cfg(test)]
mod oneshot_tests;
mod picks;
mod plan;
pub mod prompt;
mod reconcile;
#[cfg(test)]
mod reconcile_tests;
mod restore;
#[cfg(test)]
mod restore_tests;
mod reuse_key;
pub mod roster;
pub mod session;
#[cfg(test)]
mod session_tests;
mod setup_error;
#[cfg(test)]
mod setup_error_tests;
mod spawn_env;
pub mod subagent;
mod switch;
#[cfg(test)]
mod switch_tests;
mod tool_call;
mod transcript;
pub mod turn;
pub mod types;
mod warm;
pub mod window;
#[cfg(test)]
mod window_tests;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Ids;
pub use agent_commands::{FlightId, Killer};
use approvals::{PendingElicitation, PendingPermission, PendingQuestion};
pub use create::{CreateOptions, CreateRequest};
pub use effect::Effect;
pub use ensure::{EnsureId, Holder};
pub use event::Event;
pub use history::{OpenHistoryOptions, OpenHistoryRequest};
pub use import::RequestId;
use mcp::{McpBook, RegistryOp};
pub use plan::is_plan_tool;
use prompt::{BusyRetry, Dispatch, SendNow, SendOptions};
use restore::Run;
pub use restore::RunId;
pub use transcript::{
    CommandsUpdateEvent, CompactionNotice, HistoryPayload, MessageChunkEvent, PayloadMetadata,
    Plan, PlanUpdateEvent, RetryNotice, SessionInfoUpdateEvent, ToolCallEvent, ToolCallUpdateEvent,
    UsageUpdateEvent, UserPromptEvent,
};
use turn::HostReports;
use types::{ChatMessage, LogPoint, Object, PlanEntry, Session, Usage};

/// What the store holds. Field names follow the TypeScript store's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub sessions: BTreeMap<String, Session>,
    pub messages: BTreeMap<String, Vec<ChatMessage>>,
    pub tool_calls: BTreeMap<String, Vec<Object>>,
    pub plans: BTreeMap<String, Vec<PlanEntry>>,
    pub commands: BTreeMap<String, Vec<Value>>,
    pub commands_seq: BTreeMap<String, LogPoint>,
    pub session_usage: BTreeMap<String, Usage>,
    /// The persisted chat index; the model reads which sessions it holds.
    pub session_index: Vec<Object>,
    /// Waiting on the user, keyed by request id.
    pub pending_permissions: BTreeMap<String, PendingPermission>,
    /// Waiting on the user, keyed by question id.
    pub pending_questions: BTreeMap<String, PendingQuestion>,
    pub pending_elicitations: BTreeMap<String, PendingElicitation>,
    /// The host's browser consent tickets, keyed by request id.
    pub pending_browser_consents: BTreeMap<String, Value>,
    /// Per agent: `idle`, `spawning`, `connected` or `error`.
    pub agent_status: BTreeMap<String, Value>,
    /// Per session, the count and stop reason of the turns that closed.
    pub turn_end_notices: BTreeMap<String, Value>,
    /// Per session, the prompts waiting for the turn to end, oldest first.
    pub prompt_queues: BTreeMap<String, Vec<Value>>,
    /// Sessions whose queue must not flush (a cancel and send-now).
    pub suppress_queue_flush: BTreeMap<String, Value>,
    /// Per agent config, the last models, modes and options a session showed.
    pub agent_options_cache: BTreeMap<String, Value>,
    /// Reuse key (config, cwd, project) to the live agent running it, in the
    /// order the keys were added.
    pub config_to_live_agent: Object,
    /// The chat the window shows.
    pub active_session_id: Option<String>,
    /// Warm-slot key to the prepared session it holds.
    pub prepared_sessions: BTreeMap<String, String>,
    /// Per session, the durable in-chat agent switch markers.
    pub agent_switches: BTreeMap<String, Vec<Value>>,
    /// Registered agent processes: capabilities, auth methods, host auth.
    pub agents: BTreeMap<String, Value>,
    /// Per agent, the sign-in the user is asked for (a browser page or an
    /// agent's own prompt).
    pub pending_sign_in: BTreeMap<String, Value>,
    /// Per agent, the sign-in URL the app may open without a click.
    pub auto_open_browser_url: BTreeMap<String, Value>,
    /// Sessions whose launch is in flight.
    pub launching_session_ids: BTreeMap<String, Value>,
    /// Per `agentId\0cwd`, a discovery in flight.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub discovering_keys: BTreeMap<String, Value>,
    /// Per `agentId\0cwd`, the agent's own sessions found on disk.
    pub discovered_sessions: BTreeMap<String, Value>,
    /// Per session, the prompt command waiting to be confirmed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prompt_command_states: BTreeMap<String, Value>,
    /// Warm-pool keys whose prepare is running.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preparing_chat_keys: BTreeMap<String, Value>,
    /// Warm-pool keys whose prepare failed, classified for the launcher.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prepare_chat_errors: BTreeMap<String, Value>,
    /// The agent config the one-shots run on (`selectedAgentConfigId`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_agent_config_id: Option<String>,
    /// The saved agent configs (`agentConfigs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_configs: Vec<Object>,
    /// Reuse keys whose prewarm spawn is in flight, for the warm-up UI.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub warming_configs: BTreeMap<String, Value>,
    /// The user's MCP servers, as the registry saved them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<Object>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mcp_servers_loaded: bool,
    /// The agent's MCP install requests, by request id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pending_mcp_installs: BTreeMap<String, Value>,
    /// Per server id, what Troupe's own probe reached.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp_probe_status: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp_tools: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp_tools_loaded: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp_probing: BTreeMap<String, Value>,
    /// Per server id, the redacted reason a probe could not connect.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp_probe_error: BTreeMap<String, Value>,
    #[serde(
        default,
        rename = "mcpOAuthConnecting",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub mcp_oauth_connecting: BTreeMap<String, Value>,
    #[serde(
        default,
        rename = "mcpOAuthConnected",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub mcp_oauth_connected: BTreeMap<String, Value>,
    /// Per server id, the web sign-in whose callback the user pastes back.
    #[serde(
        default,
        rename = "mcpOAuthPasteBack",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub mcp_oauth_paste_back: BTreeMap<String, Value>,
    pub book: Book,
}

/// What the TypeScript store keeps in module-level maps beside its state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    /// The monotonic arrival counter every message and tool call is stamped
    /// with (`nextSeq`).
    pub seq: f64,
    /// Per session, the highest host seq the installed history covers. A
    /// live event at or below it is a replay of what already shows.
    pub history_watermarks: BTreeMap<String, f64>,
    /// Per session, the cursor the live-turn reattach lowered to just before
    /// the open turn's prompt: covered tool events above it restore cards.
    #[serde(skip)]
    pub live_turn_replay_floors: BTreeMap<String, f64>,
    /// Tool call ids whose clamp was logged, oldest first, capped.
    #[serde(skip)]
    pub clamp_logged: VecDeque<String>,
    /// Malformed subagent records already logged, by call id and key.
    #[serde(skip)]
    pub subagent_warned: VecDeque<String>,
    /// Unknown dispatch ids whose subagent text was dropped and logged.
    #[serde(skip)]
    pub orphans_logged: BTreeSet<String>,
    /// Turn ids whose end already applied, oldest first, capped.
    #[serde(skip)]
    pub ended_prompt_turns: VecDeque<String>,
    /// The newest host report per (session, agent).
    #[serde(skip)]
    pub host_reports: BTreeMap<(String, String), HostReports>,
    #[serde(skip)]
    pub report_seq: u64,
    /// Per session, the wire id of the prompt turn this client has open.
    #[serde(skip)]
    pub active_wire_turns: BTreeMap<String, String>,
    /// Dispatched prompts waiting for their command to settle, by turn id.
    #[serde(skip)]
    pub dispatches: BTreeMap<String, Dispatch>,
    #[serde(skip)]
    pub busy_retries: BTreeMap<String, BusyRetry>,
    #[serde(skip)]
    pub retry_generation: u64,
    /// Send-nows waiting for the turn they stopped, by session.
    #[serde(skip)]
    pub send_now: BTreeMap<String, SendNow>,
    /// Sessions a chat tab shows, in any project's tree. The app keeps this
    /// equal to the workspace; a closed session it lists keeps its transcript.
    #[serde(skip)]
    pub open_chat_tabs: BTreeSet<String>,
    /// Browser-open URLs caught before their agent registered, oldest first.
    #[serde(skip)]
    pub early_browser_opens: VecDeque<(String, String)>,
    /// The app shows native panes, such as an agent's browser tab
    /// (`isTauriContext`). The app sets it once.
    #[serde(skip)]
    pub desktop: bool,
    /// Option restores in flight: model picks and heals.
    #[serde(skip)]
    pub(crate) runs: BTreeMap<RunId, Run>,
    #[serde(skip)]
    pub(crate) run_seq: RunId,
    /// Per session, the newest model pick still running (`latestModelPick`).
    #[serde(skip)]
    pub(crate) latest_model_pick: BTreeMap<String, RunId>,
    /// Sessions whose heal was asked for again while one ran.
    #[serde(skip)]
    pub(crate) heal_rerun: BTreeSet<String>,
    /// `session\0key` of the restore warns already logged.
    #[serde(skip)]
    pub(crate) warned_once: BTreeSet<String>,
    /// Agents whose process signed in.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub authenticated_agents: BTreeSet<String>,
    /// Per agent config, the auth method that last signed in; saved, never a
    /// credential.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub remembered_auth_methods: BTreeMap<String, String>,
    /// `agent\0method` to the authenticate in flight.
    #[serde(skip)]
    pub(crate) auth_flights: BTreeMap<String, FlightId>,
    #[serde(skip)]
    pub(crate) flight_seq: FlightId,
    /// Clicks that share a flight, which fail with it.
    #[serde(skip)]
    pub(crate) auth_waiters: BTreeMap<FlightId, usize>,
    /// Per agent, the method of the last authenticate sent.
    #[serde(skip)]
    pub(crate) last_auth_attempt: BTreeMap<String, String>,
    /// Per agent, the flight whose first browser open was let open by itself.
    #[serde(skip)]
    pub(crate) auto_open_granted: BTreeMap<String, FlightId>,
    /// Per session, the older messages the reader paged in, which the live
    /// window keeps on top of its bound.
    #[serde(skip)]
    pub(crate) backfill_counts: BTreeMap<String, usize>,
    /// Sessions whose `loadOlderMessages` is in flight.
    #[serde(skip)]
    pub(crate) loading_older: BTreeSet<String>,
    /// Sessions whose history is known durable, so their window may trim.
    #[serde(skip)]
    pub(crate) durable_history: BTreeSet<String>,
    /// Sessions with no durable history: their window never trims.
    #[serde(skip)]
    pub(crate) untrimmable: BTreeSet<String>,
    /// Sessions whose durability probe is in flight.
    #[serde(skip)]
    pub(crate) durability_probes: BTreeSet<String>,
    /// The user's Projects (`useProjectStore`), which the app keeps current:
    /// a spawn takes its Project, its env and its reuse key from them.
    #[serde(skip)]
    pub projects: Vec<Value>,
    /// The spawns `ensureLiveAgent` runs and what it remembers about them.
    #[serde(skip)]
    pub(crate) ensure: ensure::EnsureBook,
    /// The `session/list` discoveries in flight, by key.
    #[serde(skip)]
    pub(crate) discoveries: BTreeMap<String, discovery::Discovery>,
    /// The `session/new` calls in flight.
    #[serde(skip)]
    pub(crate) creates: create::CreateBook,
    /// Prompts waiting to be confirmed and their resends.
    #[serde(skip)]
    pub(crate) reconcile: reconcile::ReconcileBook,
    /// The warm-pool prepares in flight.
    #[serde(skip)]
    pub(crate) warm: warm::WarmBook,
    /// Sessions not yet a chat, which a close or disconnect drops instead of
    /// keeping (`ephemeralSessionIds`).
    #[serde(skip)]
    pub(crate) ephemeral_sessions: BTreeSet<String>,
    /// Per session, the registry MCP servers it took, as a fingerprint.
    #[serde(skip)]
    pub(crate) registry_mcp: BTreeMap<String, String>,
    /// The one-shot runs in flight.
    #[serde(skip)]
    pub(crate) oneshots: oneshot::OneShotBook,
    /// The history opens in flight and the reopen generations.
    #[serde(skip)]
    pub(crate) history: history::HistoryBook,
    /// The import listings and imports in flight.
    #[serde(skip)]
    pub(crate) imports: import::ImportBook,
    /// Session calls waiting on an authenticate on demand.
    #[serde(skip)]
    pub(crate) auth_retry: auth_retry::AuthRetryBook,
    /// The shared MCP registry watch started; the store starts it once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mcp_shared_sync: bool,
    #[serde(skip)]
    pub(crate) mcp: McpBook,
}

impl Book {
    pub(crate) fn next_report(&mut self) -> u64 {
        self.report_seq += 1;
        self.report_seq
    }

    pub(crate) fn next_flight(&mut self) -> FlightId {
        self.flight_seq += 1;
        self.flight_seq
    }

    pub(crate) fn next_run(&mut self) -> RunId {
        self.run_seq += 1;
        self.run_seq
    }

    pub(crate) fn next_seq(&mut self) -> f64 {
        self.seq += 1.0;
        self.seq
    }

    /// `rebaseSeqCounter`: never move the counter back.
    pub(crate) fn rebase_seq(&mut self, floor: f64) {
        if floor > self.seq {
            self.seq = floor;
        }
    }

    /// `isHistoryCoveredEvent`.
    pub(crate) fn history_covered(&self, session_id: &str, event_seq: Option<f64>) -> bool {
        let Some(seq) = event_seq.filter(|seq| seq.is_finite() && *seq > 0.0) else {
            return false;
        };
        seq <= self
            .history_watermarks
            .get(session_id)
            .copied()
            .unwrap_or(0.0)
    }

    /// `liveTurnReplaySeq`: the host seq of a covered tool event that the
    /// open turn's attach replay re-delivers.
    pub(crate) fn live_turn_replay_seq(
        &self,
        session_id: &str,
        event_seq: Option<f64>,
    ) -> Option<f64> {
        let floor = *self.live_turn_replay_floors.get(session_id)?;
        let seq = event_seq?;
        (seq > floor && self.history_covered(session_id, Some(seq))).then_some(seq)
    }

    /// `isHistoryCoveredToolEvent`.
    pub(crate) fn history_covered_tool(&self, session_id: &str, event_seq: Option<f64>) -> bool {
        self.history_covered(session_id, event_seq)
            && self.live_turn_replay_seq(session_id, event_seq).is_none()
    }
}

/// What a step may read from outside the model.
pub struct Env<'a> {
    /// `Date.now()`, epoch ms.
    pub now: f64,
    pub ids: &'a mut dyn Ids,
    /// The host owns the history (`historyMode` is `server`): the web client.
    pub server_history: bool,
}

impl Env<'_> {
    /// `newId(prefix)`.
    pub(crate) fn new_id(&mut self, prefix: &str) -> String {
        format!("{prefix}-{}", self.ids.next_id())
    }
}

/// Apply one event.
pub fn step(state: &mut State, event: Event, env: &mut Env<'_>) -> Vec<Effect> {
    let mut fx = Vec::new();
    let flushed = event.coalesced_session(state);
    match event {
        Event::UserPrompt(event, seq) => state.on_user_prompt(event, seq, env),
        Event::MessageChunk(event, seq) => state.on_message_chunk(event, seq, env, &mut fx),
        Event::ToolCall(event, seq) => state.on_tool_call(event, seq, env, &mut fx),
        Event::ToolCallUpdate(event, seq) => state.on_tool_call_update(event, seq, env, &mut fx),
        Event::PlanUpdate(event) => state.on_plan_update(event),
        Event::CommandsUpdate(event, point) => state.on_commands_update(event, point),
        Event::SessionInfoUpdate(event) => state.on_session_info_update(event, &mut fx),
        Event::UsageUpdate(event) => state.on_usage_update(event, env),
        Event::PermissionRequest(event, seq) => state.on_permission_request(event, seq, &mut fx),
        Event::QuestionRequest(event, seq) => state.on_question_request(event, seq),
        Event::QuestionCancelled(event) => state.on_question_cancelled(event),
        Event::ElicitationRequest(event, seq) => state.on_elicitation_request(event, seq),
        Event::BrowserTicketClosed(event) => state.on_browser_ticket_closed(event),
        Event::ModeUpdate(event) => state.on_mode_update(event, env, &mut fx),
        Event::ConfigOptionsUpdate(event) => state.on_config_options_update(event, env, &mut fx),
        Event::PromptComplete(event, seq) => state.on_prompt_complete(event, seq, env, &mut fx),
        Event::AgentWorking(event) => state.on_agent_working(event, env, &mut fx),
        Event::BackgroundCommands(event) => state.on_background_commands(event),
        Event::SessionCreated(event) => state.on_session_created(event, env, &mut fx),
        Event::SessionClosed(event) => state.on_session_closed(event, env, &mut fx),
        Event::AgentSpawned(event) => state.on_agent_spawned(event),
        Event::AgentError(event) => state.on_agent_error(event, env, &mut fx),
        Event::AgentCrashed(event) => state.on_agent_crashed(event, env, &mut fx),
        Event::AgentDisconnected(event) => state.on_agent_disconnected(event, env, &mut fx),
        Event::BrowserOpenRequest(event) => state.on_browser_open_request(event, &mut fx),
        Event::AgentSignInRequest(event) => state.on_agent_sign_in_request(event, &mut fx),
        Event::AgentSignInResolved(event) => state.on_agent_sign_in_resolved(event),
        Event::BrowserAgentTab(event, seq) => state.on_browser_agent_tab(event, seq, &mut fx),
        Event::AgentBrowserTabClosed(event) => state.on_browser_tab_closed(event, &mut fx),
        Event::AgentsWatchState(state_) => state.on_agents_watch_state(state_, env, &mut fx),
        Event::AgentsWatchDelta(delta) => state.on_agents_watch_delta(delta, env, &mut fx),
        Event::DropSessionTranscript(session_id) => state.drop_session_transcript(&session_id),
        Event::ApplySessionIndexSnapshot(entries) => state.apply_session_index(entries, &mut fx),
        Event::SessionIndexReloaded(reload, entries) => {
            state.apply_session_index(entries, &mut fx);
            state.on_import_reloaded(&reload, None, &mut fx);
        }
        Event::SetMode(session_id, mode_id) => state.set_mode(session_id, mode_id, &mut fx),
        Event::SetModel(session_id, model_id, options) => {
            state.set_model(session_id, model_id, options, None, &mut fx);
        }
        Event::SetConfigOption(session_id, config_id, value, options) => {
            state.set_config_option(session_id, config_id, value, options, None, &mut fx);
        }
        Event::ControlSet(command, reply) => {
            auth_retry::note_retry_succeeded(&command, &mut fx);
            let run = command.run();
            state.on_control_set(command, reply, env, &mut fx);
            if let Some(run) = run {
                state.on_run_command(run, None, &mut fx);
            }
        }
        Event::ControlFailed(command, message, code) => {
            let error = host_error::HostError::new(message, code);
            state.on_session_call_failed(command, error, &mut fx)
        }
        Event::DiscoverSessions(agent_id, cwd) => state.discover_sessions(agent_id, cwd, &mut fx),
        Event::ListImportableSessions(config_id, cwd, project_id, request) => {
            state.list_importable_sessions(&config_id, &cwd, project_id, request, &mut fx);
        }
        Event::ImportAgentSessions(request) => state.import_agent_sessions(request, &mut fx),
        Event::StopImport => state.stop_imports(),
        Event::SessionIndexReloadFailed(reload, message) => {
            state.on_import_reloaded(&reload, Some(&message), &mut fx);
        }
        Event::FactoryKeyStatus(read, stored) => state.on_factory_key_status(read, stored, &mut fx),
        Event::ComposerOptionsRead(read, saved) => {
            if let Effect::ReadComposerOptions { run, .. } = read {
                state.on_composer_options_read(run, saved, &mut fx);
            }
        }
        Event::ReadvertiseWaitOver(run) => state.poll_readvertise(Some(run), &mut fx),
        Event::SpawnAgent(config, project_id, session_id) => {
            state.spawn_agent(config, Some(project_id), session_id, None, &mut fx);
        }
        Event::KillAgent(agent_id) => state.kill_agent(agent_id, Killer::User, &mut fx),
        Event::CreateSession(request) => state.create_session(request, &mut fx),
        Event::PrepareChat(request) => state.prepare_chat(request, &mut fx),
        Event::CancelPreparedChat(key) => state.cancel_prepared_chat(&key, &mut fx),
        Event::ClaimPreparedChat(key, project_id, request) => {
            state.claim_prepared_chat(&key, &project_id, request, &mut fx);
        }
        Event::AgentSwitch(event, seq) => state.on_agent_switch(event, seq, env),
        Event::CancelAgentSwitch(session_id) => state.cancel_agent_switch(&session_id),
        Event::SetSwitchPendingOption(session_id, patch) => {
            state.set_switch_pending_option(&session_id, patch, &mut fx);
        }
        Event::ArmAgentSwitch(session_id, to_config_id, request) => {
            state.arm_agent_switch(&session_id, &to_config_id, request, &mut fx);
        }
        Event::OpenHistorySession(request) => state.open_history_session(request, &mut fx),
        Event::GenerateCommitMessage(cwd, diff, request) => {
            state.generate_commit_message(cwd, &diff, request, &mut fx);
        }
        Event::AssistTerminal(kind, cwd, selection, exit_code, request) => {
            state.assist_terminal(&kind, &cwd, &selection, &exit_code, request, &mut fx);
        }
        Event::DraftBugReport(input, request) => state.draft_bug_report(input, request, &mut fx),
        Event::OneShotTimerDue(timer) => state.on_oneshot_timer(&timer, &mut fx),
        Event::ReplayEnd(end) => state.on_replay_end(&end, &mut fx),
        Event::PrewarmAgent(config_id, cwd) => {
            let options = ensure::EnsureOptions {
                register_warm_ui: true,
                silent: true,
                ..Default::default()
            };
            state.ensure_live_agent(&config_id, &cwd, options, ensure::Caller::Prewarm, &mut fx);
        }
        Event::RenewAgentCredentials(config_id, project_id) => {
            state.renew_agent_credentials(&config_id, &project_id);
        }
        Event::SetProjects(projects) => state.book.projects = projects,
        Event::AgentConfigsChanged(configs) => state.agent_configs = configs,
        Event::AuthenticateAgent(agent_id, method_id, input) => {
            state.authenticate_agent(agent_id, &method_id, input, &mut fx);
        }
        Event::LogoutAgent(agent_id) => state.logout_agent(agent_id, &mut fx),
        Event::RespondAgentSignIn(agent_id, action) => {
            state.respond_agent_sign_in(agent_id, action, &mut fx);
        }
        Event::CompleteBrowserAuth(agent_id) => state.complete_browser_auth(&agent_id, &mut fx),
        Event::AgentCommandDone(command, reply) => {
            state.on_agent_command_done(command, reply, env, &mut fx);
        }
        Event::AgentCommandFailed(command, message, code) => {
            state.on_agent_command_failed(command, message, code, &mut fx);
        }
        Event::PickModel(session_id, model_id, values) => {
            state.pick_model(session_id, model_id, values, &mut fx);
        }
        Event::ClearPendingSignIn(agent_id) => state.clear_pending_sign_in(&agent_id),
        Event::SetActiveSession(session_id) => state.active_session_id = session_id,
        Event::MarkResumeFailed(session_id, reason) => {
            state.mark_resume_failed(&session_id, &reason);
        }
        Event::TurnEnd(end) => state.on_turn_end(end, env, &mut fx),
        Event::SendPrompt(session_id, text) => {
            let blocks = vec![serde_json::json!({ "type": "text", "text": text })];
            state.send_prompt(&session_id, blocks, SendOptions::default(), env, &mut fx);
        }
        Event::SendPromptBlocks(session_id, blocks, options) => {
            state.send_prompt(
                &session_id,
                blocks,
                options.unwrap_or_default(),
                env,
                &mut fx,
            );
        }
        Event::CancelPrompt(session_id) => state.cancel_prompt(&session_id, &mut fx),
        Event::RemoveQueuedPrompt(session_id, id) => state.remove_queued_prompt(&session_id, &id),
        Event::MoveQueuedPrompt(session_id, id, to) => {
            state.move_queued_prompt(&session_id, &id, to, &mut fx);
        }
        Event::ClearQueuedPrompts(session_id) => state.clear_queued_prompts(&session_id, &mut fx),
        Event::SendQueuedPromptNow(session_id, id) => {
            state.send_queued_prompt_now(&session_id, &id, env, &mut fx);
        }
        Event::PromptSettled(settled) => {
            if !state.on_oneshot_sent(&settled.turn_id, &settled.outcome, &mut fx) {
                state.on_prompt_settled(settled, env, &mut fx);
            }
        }
        Event::QueueRetryDue(session_id, generation) => {
            state.on_queue_retry_due(&session_id, generation, env, &mut fx);
        }
        Event::LoadOlderMessages(session_id, count) => {
            state.load_older_messages(session_id, count, &mut fx);
        }
        Event::RecheckPromptCommand(session_id) => {
            state.recheck_prompt_command(&session_id, &mut fx);
        }
        Event::PromptResendDue(session_id, generation) => {
            state.on_prompt_resend_due(&session_id, generation, &mut fx);
        }
        Event::TransportReconnecting(reconnecting) => {
            state.book.reconcile.reconnecting = reconnecting;
        }
        Event::ResendPendingPrompts(session_id) => {
            state.resend_pending_prompts(session_id, &mut fx)
        }
        Event::NoteHistoryWatermark(session_id, payload) => {
            state.note_history_watermark(&session_id, &payload);
        }
        Event::NoteLiveTurnReplayFloor(session_id, floor) => {
            state.book.live_turn_replay_floors.insert(session_id, floor);
        }
        Event::ClearSessionBackfill(session_id) => {
            state.clear_session_backfill(&session_id, &mut fx);
        }
        Event::CloseSession(session_id) => state.close_session(session_id, &mut fx),
        Event::DeleteHistorySession(session_id) => {
            state.delete_history_session(session_id, &mut fx);
        }
        Event::SessionCommandDone(command, _) => state.on_session_command(command, None, &mut fx),
        Event::SessionCommandFailed(command, message) => {
            state.on_session_command(command, Some(message), &mut fx);
        }
        Event::CreateLaunchPlaceholder(launch) => state.create_launch_placeholder(launch, env),
        Event::DiscardLaunchPlaceholder(session_id) => {
            state.discard_launch_placeholder(&session_id);
        }
        Event::SeedLaunchUserMessage(session_id, blocks) => {
            state.seed_launch_user_message(&session_id, blocks, env);
        }
        Event::ClearLaunchingSession(session_id) => {
            state.launching_session_ids.remove(&session_id);
        }
        Event::LoadMcpServers(_) => state.load_mcp_servers(&mut fx),
        Event::SaveMcpServer(server) => {
            state.queue_mcp_write(RegistryOp::Save(server), env, &mut fx);
        }
        Event::ImportMcpServers(servers) => {
            state.queue_mcp_write(RegistryOp::Import(servers), env, &mut fx);
        }
        Event::SaveCatalogInstall(result) => {
            state.queue_mcp_write(RegistryOp::Catalog(result), env, &mut fx);
        }
        Event::SetMcpServerEnabled(id, enabled) => {
            state.queue_mcp_write(RegistryOp::SetEnabled(id, enabled), env, &mut fx);
        }
        Event::DeleteMcpServer(id) => state.queue_mcp_write(RegistryOp::Delete(id), env, &mut fx),
        Event::McpRegistryChanged(_) => state.queue_mcp_write(RegistryOp::Reload, env, &mut fx),
        Event::ProbeMcpServer(id) => state.probe_mcp_server(&id, &mut fx),
        Event::LoadMcpTools(id) => state.load_mcp_tools(&id, &mut fx),
        Event::ConnectMcpOAuth(project_id, id) => state.connect_mcp_oauth(project_id, id, &mut fx),
        Event::CompleteMcpOAuthPaste(id, callback_url) => {
            state.complete_mcp_oauth_paste(id, callback_url, &mut fx);
        }
        Event::CancelMcpOAuth(id) => state.cancel_mcp_oauth(&id, &mut fx),
        Event::CheckMcpOAuthStatus(project_id, id) => {
            state.check_mcp_oauth_status(project_id, id, &mut fx);
        }
        Event::DisconnectMcpOAuth(project_id, id) => {
            state.disconnect_mcp_oauth(project_id, id, &mut fx);
        }
        Event::AnswerMcpInstall(request_id, decision, inputs, secrets, plan_id, digest) => {
            let answer = mcp::InstallAnswer {
                request_id,
                decision,
                inputs,
                secrets,
                plan_id,
                digest,
            };
            state.answer_mcp_install(answer, &mut fx);
        }
        Event::DismissMcpInstall(request_id) => {
            state.pending_mcp_installs.remove(&request_id);
        }
        Event::McpInstallRequest(event) => state.on_mcp_install_request(event),
        Event::McpInstallResolved(event) => state.on_mcp_install_resolved(event),
        Event::McpSharedChanged(change) => state.on_mcp_shared_changed(change, &mut fx),
        Event::McpCommandDone(command, reply) => {
            state.on_mcp_command(command, Ok(reply), env, &mut fx);
        }
        Event::McpCommandFailed(command, error) => {
            state.on_mcp_command(command, Err(error), env, &mut fx);
        }
        Event::McpOAuthAuthUrl(start, auth_url, redirect_uri) => {
            state.on_mcp_oauth_auth_url(start, auth_url, redirect_uri);
        }
        Event::SessionPayloadRead(read, outcome) => {
            state.on_session_payload_read(read, outcome, &mut fx);
        }
        Event::RespondPermission(request_id, option_id) => {
            state.respond_permission(&request_id, option_id, &mut fx);
        }
        Event::AnswerQuestion(question_id, values) => {
            state.answer_question(&question_id, values, &mut fx);
        }
        Event::RespondElicitation(request_id, action, content) => {
            state.respond_elicitation(&request_id, action, content, &mut fx);
        }
        Event::AnswerFailed(answer, message, code) => {
            state.on_answer_failed(answer, &message, code.as_deref(), &mut fx);
        }
    }
    if let Some(session_id) = flushed {
        state.trim_flushed(&session_id, &mut fx);
    }
    // A one-shot whose collector settled moves on after the handler, as the
    // TypeScript promise chain does.
    state.advance_settled_oneshots(&mut fx);
    // A create the host answered from its records answers once its session
    // opened.
    for (create, session_id) in std::mem::take(&mut state.book.history.created) {
        state.resolve_create(create, session_id, env, &mut fx);
    }
    // A prompt typed while a history open ran goes out once it ends.
    for session_id in std::mem::take(&mut state.book.history.flush) {
        state.flush_next_queued_prompt(&session_id, env, &mut fx);
    }
    // A run waiting for re-advertised options moves on once an event changed
    // them, as the TypeScript poll would see.
    state.poll_readvertise(None, &mut fx);
    fx
}
