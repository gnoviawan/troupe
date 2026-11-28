//! The subagent tools of the host `troupe` MCP server: what the child
//! advertises and forwards. The parent answers every call
//! (`parent/subagents.rs`) through the host's orchestrator engine, scoped to
//! the calling chat.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// The tool a `Subagent` frame serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentTool {
    ListAgents,
    Start,
    Send,
    Wait,
    Status,
    Stop,
    MessageParent,
    Answer,
}

impl SubagentTool {
    /// The MCP tool name, for logs.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ListAgents => "subagent_list_agents",
            Self::Start => "subagent_start",
            Self::Send => "subagent_send",
            Self::Wait => "subagent_wait",
            Self::Status => "subagent_status",
            Self::Stop => "subagent_stop",
            Self::MessageParent => "message_parent",
            Self::Answer => "subagent_answer",
        }
    }
}

/// Where a subagent works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubagentWorkspaceInput {
    /// The chat's own working directory, shared with you.
    Shared,
    /// A private git worktree on a snapshot of the chat's current state,
    /// uncommitted changes included.
    Worktree,
}

/// Input of `subagent_start`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentStartInput {
    /// The `agentId` of an allowed profile from `subagent_list_agents`.
    pub agent: String,
    /// The complete task for the subagent, 1 to 8000 characters. This
    /// chat's transcript is not included in its prompt.
    pub task: String,
    /// `worktree` (default) or `shared`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<SubagentWorkspaceInput>,
    /// A short title for the user's sub-agent panel. Defaults to the agent
    /// name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The model id to ask the agent for, when it offers a choice. Absent:
    /// the profile's `defaultModel` from `subagent_list_agents`, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Input of `subagent_wait`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentWaitInput {
    /// The id `subagent_start` returned.
    pub subagent_id: String,
    /// How long to wait, in milliseconds: 1000 to 240000, default 240000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Input of `subagent_status`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentStatusInput {
    /// The runs to report. Absent or empty: every run of this chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_ids: Option<Vec<String>>,
}

/// Input of `subagent_send`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSendInput {
    /// The id `subagent_start` returned.
    pub subagent_id: String,
    /// Follow-up message for the subagent, 1 to 8000 characters.
    #[schemars(length(min = 1, max = 8000))]
    pub message: String,
    /// Cancel the subagent's current turn first when it is running, so the
    /// message runs next right away instead of waiting for the turn to end.
    /// Ignored when the subagent is idle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<bool>,
}

/// Input of `subagent_stop`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentRefInput {
    /// The id `subagent_start` returned.
    pub subagent_id: String,
}

/// Input of `message_parent`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageParentInput {
    /// The message for the orchestrator, 1 to 8000 characters.
    pub message: String,
}

/// Input of `subagent_answer`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentAnswerInput {
    /// The id `subagent_start` returned.
    pub subagent_id: String,
    /// The question id the wake reported.
    pub question_id: String,
    /// The selected option labels, plus at most one free-text answer when the
    /// question allows one.
    pub answers: Vec<String>,
}

/// `subagent_wait`'s budget bounds and default.
pub const WAIT_MIN_MS: u64 = 1_000;
pub const WAIT_MAX_MS: u64 = 240_000;

pub const LIST_AGENTS_DESCRIPTION: &str = "List the sub-agent profiles you can start, and the \
limits that apply. Returns JSON {agents: [{agentId, name, agent, allowed, defaultModel, \
description}], enabled, maxRunning, maxDepth}. Each entry is a profile: agentId is the id to \
start, name the installed agent it runs (Codex, Gemini CLI, ...), and agent that agent's id. \
Several profiles can run the same agent with a different model and thinking level. Only \
profiles with allowed true can be started, and only while enabled is true; the user changes \
both in Preferences. defaultModel is the model a start without `model` asks for (null: the \
agent's own default). description is the user's note on what that profile is good at (null: \
no note); start the allowed profile whose description fits the task.";

/// Hint returned in `subagent_start`'s result and description so the parent
/// agent ends its turn instead of polling or blocking needlessly.
pub const BACKGROUND_WAKE_HINT: &str = "It runs in the background. When it finishes, Troupe \
sends its result to this chat as a new message, even after your turn ends. Do not poll it or \
wait on it unless you need the result before you can go on; end your turn when no other work is \
left.";

pub const START_DESCRIPTION: &str = "Start another installed agent (Codex, Gemini CLI, ...) as \
a subagent on a task, in the background. `agent` is the agentId of a profile from \
subagent_list_agents; the profile sets the agent, its model, thinking level and other session \
settings. `task` must be complete on its own, because this chat's transcript is not included \
in the subagent's prompt. \
`workspace` is `worktree` (default: a private git worktree on a snapshot of the current \
state, uncommitted changes included, so its edits never touch yours) or `shared` (this chat's \
directory). The user sees it as a sub-agent row with its live transcript and a Stop button, \
and approves its tool permissions in this chat unless the user set its profile to allow \
them. Returns JSON {subagentId, state, title, workspace, note} once it is running. \
It runs in the background. When it finishes, Troupe sends its result to this chat as a new \
message, even after your turn ends. Do not poll it or wait on it unless you need the result \
before you can go on; end your turn when no other work is left. Send follow-ups with \
subagent_send. Errors: disabled, unknown_agent, agent_not_allowed, capacity, depth, \
invalid_input, workspace_failed, spawn_failed.";

pub const SEND_DESCRIPTION: &str = "Send a follow-up message to a subagent this chat started. \
While the run is working, the message is queued as the session's next prompt and delivered \
when the current turn ends; pass `interrupt: true` to cancel the current turn first. After \
the run has completed or been cancelled, this relaunches the agent, reloads its kept session \
and sends the message, returning the run to running; its next result again arrives in this \
chat as a new message. `message` is 1 to 8000 characters. Returns JSON {subagentId, state, \
queued}. Errors: not_found, invalid_input, disabled, unknown_agent, agent_not_allowed, \
capacity, busy, not_resumable, session_unloadable, spawn_failed.";

pub const WAIT_DESCRIPTION: &str = "Wait for a subagent to finish, up to `timeoutMs` (default \
and maximum 240000). Waiting is optional: when a subagent finishes, Troupe sends its result to \
this chat as a new message, even after your turn ends, so only call this when you need the \
result before you can go on (or to fetch a full result that was truncated). Returns JSON \
{subagentId, state, result?, error?, usage?} once it is completed, cancelled or failed. If it \
is still working when the time is up, returns {subagentId, state, timedOut: true}; that is not \
an error, call subagent_wait again. State requires_action means it waits for the user to \
approve a tool in this chat: tell the user, then wait again. After it settles, use \
subagent_send to resume it with a follow-up.";

pub const STATUS_DESCRIPTION: &str = "Report this chat's subagents without waiting: every run, \
or the ones named in `subagentIds`. Returns JSON {runs: [{subagentId, state, title, endedAt?, \
result?, resultTruncated?, error?, workspace?, usage?}]}. A result longer than 1000 characters \
is cut, with resultTruncated true; subagent_wait on that run returns the whole result.";

pub const STOP_DESCRIPTION: &str = "Stop a running subagent. Returns JSON {outcome: \"sent\"} \
or {outcome: \"already_ended\"} when it had already finished. Its partial work stays in its \
workspace, and you can resume it later with subagent_send.";

pub const MESSAGE_PARENT_DESCRIPTION: &str = "Send a message to the agent that started you \
(your orchestrator): a progress report, a question, a decision you need, or work to hand back. \
It does not wait for a reply. If you need the answer before you can go on, end your turn after \
sending: the orchestrator can resume you with its reply through subagent_send.";

pub const ANSWER_DESCRIPTION: &str = "Answer an open ask_user question of one of your subagents. \
`subagentId` is the run that asked it, `questionId` the id its wake reported, and `answers` the \
selected option labels (plus at most one free-text answer when the question allows one). The \
first answer wins. Returns JSON {outcome: \"answered\"}. Error not_found means the run belongs \
to another chat or the question is unknown or already closed.";

