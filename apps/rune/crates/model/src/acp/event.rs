//! What a step applies: the store actions and the answers to effects.

use serde::Deserialize;
use serde_json::Value;

use super::agents::{
    AgentDisconnectedEvent, AgentFailureEvent, AgentSignInRequestEvent, AgentSignInResolvedEvent,
    AgentSpawnedEvent, BrowserOpenRequestEvent,
};
use super::approvals::{
    BrowserTicketClosedEvent, ElicitationRequestEvent, PermissionRequestEvent,
    QuestionCancelledEvent, QuestionRequestEvent,
};
use super::controls::{ConfigOptionsUpdateEvent, ModeUpdateEvent};
use super::mcp::McpError;
use super::picks::PickOptions;
use super::prompt::{PromptSettled, SendOptions};
use super::roster::{AgentBrowserTabClosedEvent, BrowserAgentTabEvent};
use super::session::{SessionClosedEvent, SessionCreatedEvent};
use super::transcript::{
    CommandsUpdateEvent, HistoryPayload, MessageChunkEvent, PlanUpdateEvent,
    SessionInfoUpdateEvent, ToolCallEvent, ToolCallUpdateEvent, UsageUpdateEvent, UserPromptEvent,
};
use super::turn::{AgentWorkingEvent, BackgroundCommandsEvent, PromptCompleteEvent, TurnEnd};
use super::types::{LogPoint, Object};
use super::window::PayloadReadOutcome;
use super::{Effect, RunId, launch};

/// One store action. The variant names are the TypeScript action names; an
/// optional trailing argument is the event's host seq or log point.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Event {
    #[serde(rename = "_onUserPrompt")]
    UserPrompt(UserPromptEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onMessageChunk")]
    MessageChunk(MessageChunkEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onToolCall")]
    ToolCall(ToolCallEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onToolCallUpdate")]
    ToolCallUpdate(ToolCallUpdateEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onPlanUpdate")]
    PlanUpdate(PlanUpdateEvent),
    #[serde(rename = "_onCommandsUpdate")]
    CommandsUpdate(CommandsUpdateEvent, #[serde(default)] Option<LogPoint>),
    #[serde(rename = "_onSessionInfoUpdate")]
    SessionInfoUpdate(SessionInfoUpdateEvent),
    #[serde(rename = "_onUsageUpdate")]
    UsageUpdate(UsageUpdateEvent),
    #[serde(rename = "_onPermissionRequest")]
    PermissionRequest(PermissionRequestEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onQuestionRequest")]
    QuestionRequest(QuestionRequestEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onQuestionCancelled")]
    QuestionCancelled(QuestionCancelledEvent),
    #[serde(rename = "_onElicitationRequest")]
    ElicitationRequest(ElicitationRequestEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onBrowserTicketClosed")]
    BrowserTicketClosed(BrowserTicketClosedEvent),
    #[serde(rename = "_onModeUpdate")]
    ModeUpdate(ModeUpdateEvent),
    #[serde(rename = "_onConfigOptionsUpdate")]
    ConfigOptionsUpdate(ConfigOptionsUpdateEvent),
    #[serde(rename = "_onPromptComplete")]
    PromptComplete(PromptCompleteEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onAgentWorking")]
    AgentWorking(AgentWorkingEvent),
    #[serde(rename = "_onBackgroundCommands")]
    BackgroundCommands(BackgroundCommandsEvent),
    #[serde(rename = "_onSessionCreated")]
    SessionCreated(SessionCreatedEvent),
    #[serde(rename = "_onSessionClosed")]
    SessionClosed(SessionClosedEvent),
    #[serde(rename = "_onAgentSpawned")]
    AgentSpawned(AgentSpawnedEvent),
    #[serde(rename = "_onAgentError")]
    AgentError(AgentFailureEvent),
    #[serde(rename = "_onAgentCrashed")]
    AgentCrashed(AgentFailureEvent),
    #[serde(rename = "_onAgentDisconnected")]
    AgentDisconnected(AgentDisconnectedEvent),
    #[serde(rename = "_onBrowserOpenRequest")]
    BrowserOpenRequest(BrowserOpenRequestEvent),
    #[serde(rename = "_onAgentSignInRequest")]
    AgentSignInRequest(AgentSignInRequestEvent),
    #[serde(rename = "_onAgentSignInResolved")]
    AgentSignInResolved(AgentSignInResolvedEvent),
    #[serde(rename = "_onBrowserAgentTab")]
    BrowserAgentTab(BrowserAgentTabEvent, #[serde(default)] Option<f64>),
    #[serde(rename = "_onBrowserTabClosed")]
    AgentBrowserTabClosed(AgentBrowserTabClosedEvent),
    /// The full state of the host's `agents` watch, sent on every subscribe.
    #[serde(rename = "agentsWatchState")]
    AgentsWatchState(Value),
    /// One change of the host's `agents` watch.
    #[serde(rename = "agentsWatchDelta")]
    AgentsWatchDelta(Value),
    /// The persist a [`Effect::DropSessionTranscript`] waited for ran.
    #[serde(rename = "dropSessionTranscript")]
    DropSessionTranscript(String),
    #[serde(rename = "applySessionIndexSnapshot")]
    ApplySessionIndexSnapshot(Vec<Object>),
    #[serde(rename = "setMode")]
    SetMode(String, String),
    #[serde(rename = "setModel")]
    SetModel(String, String, #[serde(default)] Option<PickOptions>),
    #[serde(rename = "setConfigOption")]
    SetConfigOption(String, String, Value, #[serde(default)] Option<PickOptions>),
    /// The agent acknowledged a set command an `Effect::SetMode`, `SetModel`
    /// or `SetConfigOption` sent; the reply is a `SetConfigOption`'s option
    /// snapshot, when the agent sent one.
    #[serde(skip_deserializing)]
    ControlSet(Effect, Option<Vec<Value>>),
    /// The agent rejected a set command; the text is the error's message.
    #[serde(skip_deserializing)]
    ControlFailed(Effect, String),
    /// The saved picks an [`Effect::ReadComposerOptions`] read; null when
    /// none are saved.
    #[serde(skip_deserializing)]
    ComposerOptionsRead(Effect, Value),
    /// The wait an [`Effect::WaitForReadvertisedOptions`] started ran out.
    #[serde(skip_deserializing)]
    ReadvertiseWaitOver(RunId),
    #[serde(rename = "spawnAgent")]
    SpawnAgent(Value, Value, #[serde(default)] Option<String>),
    #[serde(rename = "killAgent")]
    KillAgent(String),
    #[serde(rename = "authenticateAgent")]
    AuthenticateAgent(String, String, #[serde(default)] Option<Value>),
    #[serde(rename = "logoutAgent")]
    LogoutAgent(String),
    #[serde(rename = "respondAgentSignIn")]
    RespondAgentSignIn(String, String),
    #[serde(rename = "completeBrowserAuth")]
    CompleteBrowserAuth(String),
    /// The command an agent command effect sent answered with this reply.
    #[serde(skip_deserializing)]
    AgentCommandDone(Effect, Value),
    /// The command an agent command effect sent failed; the text is the
    /// error's message.
    #[serde(skip_deserializing)]
    AgentCommandFailed(Effect, String),
    #[serde(rename = "pickModel")]
    PickModel(
        String,
        String,
        #[serde(default)] Option<serde_json::Map<String, Value>>,
    ),
    #[serde(rename = "clearPendingSignIn")]
    ClearPendingSignIn(String),
    #[serde(rename = "setActiveSession")]
    SetActiveSession(Option<String>),
    #[serde(rename = "markResumeFailed")]
    MarkResumeFailed(String, String),
    #[serde(rename = "sendPrompt")]
    SendPrompt(String, String),
    #[serde(rename = "sendPromptBlocks")]
    SendPromptBlocks(String, Vec<Value>, #[serde(default)] Option<SendOptions>),
    #[serde(rename = "cancelPrompt")]
    CancelPrompt(String),
    #[serde(rename = "removeQueuedPrompt")]
    RemoveQueuedPrompt(String, Value),
    #[serde(rename = "moveQueuedPrompt")]
    MoveQueuedPrompt(String, Value, f64),
    #[serde(rename = "clearQueuedPrompts")]
    ClearQueuedPrompts(String),
    #[serde(rename = "sendQueuedPromptNow")]
    SendQueuedPromptNow(String, Value),
    /// The command an [`Effect::DispatchPrompt`] sent has settled.
    #[serde(rename = "promptSettled")]
    PromptSettled(PromptSettled),
    /// The timer an [`Effect::RetryQueueAfter`] started ran out.
    #[serde(rename = "queueRetryDue")]
    QueueRetryDue(String, u64),
    /// The deferred close an [`Effect::EndTurn`] asked for.
    #[serde(rename = "turnEnd")]
    TurnEnd(TurnEnd),
    /// A history payload was installed for the session.
    #[serde(rename = "noteHistoryWatermark")]
    NoteHistoryWatermark(String, HistoryPayload),
    /// The live-turn reattach lowered the session's subscribe cursor.
    #[serde(rename = "noteLiveTurnReplayFloor")]
    NoteLiveTurnReplayFloor(String, f64),
    #[serde(rename = "clearSessionBackfill")]
    ClearSessionBackfill(String),
    #[serde(rename = "closeSession")]
    CloseSession(String),
    #[serde(rename = "deleteHistorySession")]
    DeleteHistorySession(String),
    /// A close or delete command answered.
    #[serde(skip_deserializing)]
    SessionCommandDone(Effect, Value),
    /// A close or delete command failed; the text is the error's message.
    #[serde(skip_deserializing)]
    SessionCommandFailed(Effect, String),
    #[serde(rename = "createLaunchPlaceholder")]
    CreateLaunchPlaceholder(launch::LaunchPlaceholder),
    #[serde(rename = "discardLaunchPlaceholder")]
    DiscardLaunchPlaceholder(String),
    #[serde(rename = "seedLaunchUserMessage")]
    SeedLaunchUserMessage(String, Vec<Value>),
    #[serde(rename = "clearLaunchingSession")]
    ClearLaunchingSession(String),
    #[serde(rename = "loadMcpServers")]
    LoadMcpServers(#[serde(default)] Vec<Value>),
    #[serde(rename = "saveMcpServer")]
    SaveMcpServer(Object),
    #[serde(rename = "importMcpServers")]
    ImportMcpServers(Vec<Object>),
    #[serde(rename = "saveCatalogInstall")]
    SaveCatalogInstall(Value),
    #[serde(rename = "setMcpServerEnabled")]
    SetMcpServerEnabled(String, Value),
    #[serde(rename = "deleteMcpServer")]
    DeleteMcpServer(String),
    #[serde(rename = "_onMcpRegistryChanged")]
    McpRegistryChanged(#[serde(default)] Value),
    #[serde(rename = "probeMcpServer")]
    ProbeMcpServer(String),
    #[serde(rename = "loadMcpTools")]
    LoadMcpTools(String),
    #[serde(rename = "connectMcpOAuth")]
    ConnectMcpOAuth(String, String),
    #[serde(rename = "completeMcpOAuthPaste")]
    CompleteMcpOAuthPaste(String, String),
    #[serde(rename = "cancelMcpOAuth")]
    CancelMcpOAuth(String),
    #[serde(rename = "checkMcpOAuthStatus")]
    CheckMcpOAuthStatus(String, String),
    #[serde(rename = "disconnectMcpOAuth")]
    DisconnectMcpOAuth(String, String),
    #[serde(rename = "answerMcpInstall")]
    AnswerMcpInstall(
        String,
        String,
        #[serde(default)] Option<Value>,
        #[serde(default)] Option<Value>,
        #[serde(default)] Option<Value>,
        #[serde(default)] Option<Value>,
    ),
    #[serde(rename = "dismissMcpInstall")]
    DismissMcpInstall(String),
    #[serde(rename = "_onMcpInstallRequest")]
    McpInstallRequest(Value),
    #[serde(rename = "_onMcpInstallResolved")]
    McpInstallResolved(Value),
    /// Another browser or device saved the shared MCP registry; `data` is
    /// the list as the app normalized it.
    #[serde(rename = "mcpSharedChanged")]
    McpSharedChanged(Value),
    /// An MCP command effect answered with this reply.
    #[serde(skip_deserializing)]
    McpCommandDone(Effect, Value),
    /// An MCP command effect failed.
    #[serde(skip_deserializing)]
    McpCommandFailed(Effect, McpError),
    /// A web OAuth start knows its sign-in link and redirect address.
    #[serde(skip_deserializing)]
    McpOAuthAuthUrl(Effect, String, String),
    /// What an [`Effect::ReadSessionPayload`] read.
    #[serde(skip_deserializing)]
    SessionPayloadRead(Effect, PayloadReadOutcome),
}
