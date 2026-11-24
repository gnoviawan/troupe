//! What a step applies: the store actions and the answers to effects.

use serde::Deserialize;
use serde_json::Value;

use super::agents::{
    AgentDisconnectedEvent, AgentFailureEvent, AgentSignInRequestEvent, AgentSignInResolvedEvent,
    AgentSpawnedEvent, BrowserOpenRequestEvent,
};
use super::answer::Answer;
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
use super::{Effect, RunId, import, launch};

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
    /// The agent rejected a set command with the error's message and the
    /// host's error code.
    #[serde(skip_deserializing)]
    ControlFailed(Effect, String, Option<String>),
    /// What an [`Effect::ReadFactoryKeyStatus`] read; `None` when the host
    /// could not say.
    #[serde(skip_deserializing)]
    FactoryKeyStatus(Effect, Option<bool>),
    /// The saved picks an [`Effect::ReadComposerOptions`] read; null when
    /// none are saved.
    #[serde(skip_deserializing)]
    ComposerOptionsRead(Effect, Value),
    /// The wait an [`Effect::WaitForReadvertisedOptions`] started ran out.
    #[serde(skip_deserializing)]
    ReadvertiseWaitOver(RunId),
    #[serde(rename = "spawnAgent")]
    SpawnAgent(Value, Value, #[serde(default)] Option<String>),
    #[serde(rename = "generateCommitMessage")]
    GenerateCommitMessage(String, String, #[serde(default)] super::import::RequestId),
    #[serde(rename = "assistTerminal")]
    AssistTerminal(
        String,
        String,
        String,
        #[serde(default)] Value,
        #[serde(default)] super::import::RequestId,
    ),
    #[serde(rename = "draftBugReport")]
    DraftBugReport(
        super::oneshot::BugReportInput,
        #[serde(default)] super::import::RequestId,
    ),
    /// A timer an [`Effect::StartOneShotTimer`] started ran out.
    #[serde(skip_deserializing)]
    OneShotTimerDue(Effect),
    #[serde(rename = "openHistorySession")]
    OpenHistorySession(super::history::OpenHistoryRequest),
    /// The replay window an [`Effect::EndReplay`] asked to close.
    #[serde(skip_deserializing)]
    ReplayEnd(Effect),
    #[serde(rename = "_onAgentSwitch")]
    AgentSwitch(
        super::switch::AgentSwitchEvent,
        #[serde(default)] Option<f64>,
    ),
    #[serde(rename = "cancelAgentSwitch")]
    CancelAgentSwitch(String),
    /// Answered by `actionResolved` with whether the switch armed.
    #[serde(rename = "armAgentSwitch")]
    ArmAgentSwitch(String, String, #[serde(default)] super::import::RequestId),
    #[serde(rename = "prepareChat")]
    PrepareChat(super::warm::PrepareRequest),
    #[serde(rename = "cancelPreparedChat")]
    CancelPreparedChat(String),
    /// Answered by `actionResolved` with the claimed session id, or null.
    #[serde(rename = "claimPreparedChat")]
    ClaimPreparedChat(String, String, #[serde(default)] super::import::RequestId),
    #[serde(rename = "createSession")]
    CreateSession(super::create::CreateRequest),
    #[serde(rename = "killAgent")]
    KillAgent(String),
    #[serde(rename = "discoverSessions")]
    DiscoverSessions(String, String),
    /// Answered by `actionResolved` with `{ status, agentId?, sessions? }`
    /// and the trailing request id.
    #[serde(rename = "listImportableSessions")]
    ListImportableSessions(
        String,
        String,
        #[serde(default)] Option<String>,
        #[serde(default)] import::RequestId,
    ),
    /// Each session's outcome is an `importProgress`; the outcomes are
    /// answered once the reloaded index arrives.
    #[serde(rename = "importAgentSessions")]
    ImportAgentSessions(import::ImportRequest),
    /// The running imports start no other session.
    #[serde(rename = "stopImport")]
    StopImport,
    /// What an import's [`Effect::RefreshSessionIndex`] read.
    #[serde(skip_deserializing)]
    SessionIndexReloaded(Effect, Vec<Object>),
    /// An import's [`Effect::RefreshSessionIndex`] failed with this message.
    #[serde(skip_deserializing)]
    SessionIndexReloadFailed(Effect, String),
    #[serde(rename = "prewarmAgent")]
    PrewarmAgent(String, String),
    /// `renewAgentCredentials`: the Project's account for the config changed.
    #[serde(rename = "renewAgentCredentials")]
    RenewAgentCredentials(String, String),
    /// The user's Projects changed (`useProjectStore`).
    #[serde(rename = "setProjects")]
    SetProjects(Vec<Value>),
    /// The saved agent configs changed; stands in for the config slice
    /// until it is ported (PR 8).
    #[serde(rename = "agentConfigsChanged")]
    AgentConfigsChanged(Vec<Object>),
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
    AgentCommandFailed(Effect, String, #[serde(default)] Option<String>),
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
    /// The reader scrolled up past the window's head: page in `count` older
    /// messages.
    #[serde(rename = "loadOlderMessages")]
    LoadOlderMessages(String, usize),
    /// The user asked whether a prompt waiting to be confirmed went through.
    #[serde(rename = "recheckPromptCommand")]
    RecheckPromptCommand(String),
    /// The timer an [`Effect::PromptResendAfter`] started ran out.
    #[serde(rename = "promptResendDue")]
    PromptResendDue(String, u64),
    /// The transport started (`true`) or ended a reconnect.
    #[serde(rename = "transportReconnecting")]
    TransportReconnecting(bool),
    /// The transport recovered: the waiting prompts, or one session's, go
    /// out again.
    #[serde(rename = "resendPendingPrompts")]
    ResendPendingPrompts(#[serde(default)] Option<String>),
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
    /// The user picked a permission option; none cancels the request.
    #[serde(rename = "respondPermission")]
    RespondPermission(String, #[serde(default)] Option<String>),
    #[serde(rename = "answerQuestion")]
    AnswerQuestion(String, Vec<String>),
    /// The user answered an elicitation: its action and the form's content.
    #[serde(rename = "respondElicitation")]
    RespondElicitation(String, String, #[serde(default)] Option<Value>),
    /// The host refused an [`Effect::Answer`]: its message and code.
    #[serde(skip_deserializing)]
    AnswerFailed(Answer, String, Option<String>),
}
