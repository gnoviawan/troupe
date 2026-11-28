//! `Subagent` frames: the `subagent_*` tools, answered by the host
//! orchestrator engine for the calling chat. A run of another chat is
//! `not_found`. Logs carry the tool, the run id and the code, never the
//! task text or a result.

use std::future::Future;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::HostPlanServer;
use crate::acp::events::{self, QuestionCancelledEvent};
use crate::acp::host_mcp::subagent_tools::{
    MessageParentInput, SubagentAnswerInput, SubagentRefInput, SubagentStartInput,
    SubagentStatusInput, SubagentTool, SubagentWaitInput, SubagentWorkspaceInput,
    BACKGROUND_WAKE_HINT, WAIT_MAX_MS, WAIT_MIN_MS,
};
use crate::acp::host_mcp::FrameReply;
use crate::acp::orchestrator::{
    CallerRef, MessageParentError, OrchestratorEngine, RunId, RunState, RunSummary, StartError,
    StartRequest, StopError, StopOutcome, WaitOutcome,
};
use crate::acp::subagents::SubagentWorkspaceMode;
use crate::acp::config::{AgentId, SessionId};

/// A refused call: the wire code and its agent-facing message, sent as
/// `{code, message}` (the automation tools' refusal shape).
fn refusal(code: &'static str, message: impl Into<String>) -> FrameReply {
    let message = message.into();
    FrameReply {
        result: Some(json!({ "code": code, "message": message })),
        ..FrameReply::err_code(code, message)
    }
}

fn not_found() -> FrameReply {
    refusal("not_found", "no such subagent in this chat")
}

/// The tool's arguments, or the `invalid_input` message.
fn parse<T: DeserializeOwned>(input: Option<Value>) -> Result<T, String> {
    // A tool with only optional fields may arrive with no arguments.
    let input = input.unwrap_or_else(|| json!({}));
    serde_json::from_value(input).map_err(|error| format!("invalid arguments: {error}"))
}

fn start_refusal(error: &StartError) -> FrameReply {
    let code = match error {
        StartError::Disabled => "disabled",
        StartError::UnknownAgent { .. } => "unknown_agent",
        StartError::AgentNotAllowed(_) => "agent_not_allowed",
        StartError::Capacity { .. } => "capacity",
        StartError::Depth { .. } => "depth",
        StartError::InvalidInput(_) => "invalid_input",
        StartError::WorkspaceUnavailable(_) => "workspace_failed",
        StartError::SpawnFailed(_) => "spawn_failed",
    };
    refusal(code, error.to_string())
}

fn state_name(state: RunState) -> &'static str {
    match state {
        RunState::Running => "running",
        RunState::RequiresAction => "requires_action",
        RunState::Completed => "completed",
        RunState::Cancelled => "cancelled",
        RunState::Failed => "failed",
    }
}

/// One run as `subagent_status` and a settled `subagent_wait` report it.
pub(super) fn summary_json(summary: &RunSummary) -> Value {
    let mut value = json!({
        "subagentId": summary.run.0,
        "state": state_name(summary.state),
        "title": summary.title,
    });
    if let Some(ended_at) = summary.ended_at {
        value["endedAt"] = json!(ended_at);
    }
    if let Some(result) = &summary.result {
        value["result"] = json!(result);
    }
    if let Some(error) = &summary.error {
        value["error"] = json!(error);
    }
    if let Some(workspace) = &summary.workspace {
        value["workspace"] = json!(workspace);
    }
    if let Some(usage) = &summary.usage {
        value["usage"] = json!(usage);
    }
    value
}

/// Result text one `subagent_status` entry carries: 64 settled runs of full
/// results would overflow the child's 1 MiB frame, so the listing previews
/// and `subagent_wait` returns the whole result.
const STATUS_RESULT_CHARS: usize = 1_000;

/// One run as `subagent_status` reports it: [`summary_json`] with the result
/// cut to [`STATUS_RESULT_CHARS`], flagged `resultTruncated` when cut.
pub(super) fn status_json(summary: &RunSummary) -> Value {
    let mut value = summary_json(summary);
    if let Some(result) = &summary.result {
        if let Some((cut, _)) = result.char_indices().nth(STATUS_RESULT_CHARS) {
            value["result"] = json!(&result[..cut]);
            value["resultTruncated"] = json!(true);
        }
    }
    value
}

impl HostPlanServer {
    pub(super) async fn subagent_call(
        &self,
        agent_id: &str,
        session_id: &str,
        tool: Option<SubagentTool>,
        input: Option<Value>,
        hung_up: impl Future<Output = ()>,
    ) -> FrameReply {
        let Some(tool) = tool else {
            return FrameReply::err("a subagent frame needs a tool");
        };
        let session = crate::logging::redact_session_id(session_id);
        let Some(engine) = crate::acp::orchestrator::installed() else {
            log::warn!(
                "[host-mcp] {} refused for session {session}: no orchestrator",
                tool.name()
            );
            return refusal("unavailable", "subagents are not available on this host");
        };
        let caller = CallerRef {
            agent: AgentId(agent_id.to_string()),
            session: SessionId(session_id.to_string()),
        };
        let reply = match tool {
            SubagentTool::ListAgents => list_agents(&engine),
            SubagentTool::Start => match parse::<SubagentStartInput>(input) {
                Ok(input) => start(engine, caller, input, hung_up).await,
                Err(message) => refusal("invalid_input", message),
            },
            SubagentTool::Wait => match parse::<SubagentWaitInput>(input) {
                Ok(input) => wait(&engine, caller, input, hung_up).await,
                Err(message) => refusal("invalid_input", message),
            },
            SubagentTool::Status => match parse::<SubagentStatusInput>(input) {
                Ok(input) => status(&engine, caller, input),
                Err(message) => refusal("invalid_input", message),
            },
            SubagentTool::Stop => match parse::<SubagentRefInput>(input) {
                Ok(input) => stop(&engine, caller, input).await,
                Err(message) => refusal("invalid_input", message),
            },
            SubagentTool::MessageParent => match parse::<MessageParentInput>(input) {
                Ok(input) => message_parent(&engine, caller, input),
                Err(message) => refusal("invalid_input", message),
            },
            SubagentTool::Answer => match parse::<SubagentAnswerInput>(input) {
                Ok(input) => self.answer_subagent_question(&engine, caller, input),
                Err(message) => refusal("invalid_input", message),
            },
        };
        if !reply.ok {
            log::info!(
                "[host-mcp] {} for session {session} answered {}",
                tool.name(),
                reply.code.as_deref().unwrap_or("error")
            );
        }
        reply
    }
}

fn list_agents(engine: &OrchestratorEngine) -> FrameReply {
    let settings = engine.settings_view();
    let agents: Vec<Value> = engine
        .list_agents()
        .into_iter()
        .map(|agent| {
            json!({
                "agentId": agent.agent_id,
                "name": agent.name,
                "agent": agent.agent,
                "allowed": agent.allowed,
                "defaultModel": agent.default_model,
                "description": agent.description,
            })
        })
        .collect();
    FrameReply::ok_with(json!({
        "agents": agents,
        "enabled": settings.enabled,
        "maxRunning": settings.max_running,
        "maxDepth": settings.max_depth,
    }))
}

/// The start runs on its own task, so this caller going away never drops
/// it halfway. A hang-up (the client cancelled the tool call) cancels it
/// instead: the engine kills the child and removes its worktree, since the
/// agent will never learn the run's id.
async fn start(
    engine: std::sync::Arc<OrchestratorEngine>,
    caller: CallerRef,
    input: SubagentStartInput,
    hung_up: impl Future<Output = ()>,
) -> FrameReply {
    let request = StartRequest {
        agent: input.agent,
        task: input.task,
        workspace: match input.workspace {
            Some(SubagentWorkspaceInput::Shared) => SubagentWorkspaceMode::Shared,
            Some(SubagentWorkspaceInput::Worktree) | None => SubagentWorkspaceMode::Worktree,
        },
        title: input.title,
        model: input.model,
    };
    let abandoned = tokio_util::sync::CancellationToken::new();
    let mut task = {
        let abandoned = abandoned.clone();
        tokio::spawn(async move { engine.start_until(caller, request, abandoned).await })
    };
    let started = tokio::select! {
        biased;
        started = &mut task => started,
        () = hung_up => {
            abandoned.cancel();
            return FrameReply::err("the caller hung up");
        }
    };
    match started {
        Ok(Ok(summary)) => FrameReply::ok_with(json!({
            "subagentId": summary.run.0,
            "state": state_name(summary.state),
            "title": summary.title,
            "workspace": summary.workspace,
            "note": BACKGROUND_WAKE_HINT,
        })),
        Ok(Err(error)) => start_refusal(&error),
        Err(error) => {
            log::error!("[host-mcp] subagent_start task failed: {error}");
            refusal("internal", "the host could not start the subagent")
        }
    }
}

async fn wait(
    engine: &OrchestratorEngine,
    caller: CallerRef,
    input: SubagentWaitInput,
    hung_up: impl Future<Output = ()>,
) -> FrameReply {
    let budget = input
        .timeout_ms
        .unwrap_or(WAIT_MAX_MS)
        .clamp(WAIT_MIN_MS, WAIT_MAX_MS);
    let run = RunId(input.subagent_id);
    let outcome = tokio::select! {
        outcome = engine.wait(caller, run.clone(), Duration::from_millis(budget)) => outcome,
        () = hung_up => return FrameReply::err("the caller hung up"),
    };
    match outcome {
        Ok(WaitOutcome::Settled(summary)) => FrameReply::ok_with(summary_json(&summary)),
        Ok(WaitOutcome::TimedOut { state }) => FrameReply::ok_with(json!({
            "subagentId": run.0,
            "state": state_name(state),
            "timedOut": true,
        })),
        Err(_) => not_found(),
    }
}

fn status(engine: &OrchestratorEngine, caller: CallerRef, input: SubagentStatusInput) -> FrameReply {
    let runs = input
        .subagent_ids
        .filter(|ids| !ids.is_empty())
        .map(|ids| ids.into_iter().map(RunId).collect());
    match engine.status(caller, runs) {
        Ok(summaries) => FrameReply::ok_with(json!({
            "runs": summaries.iter().map(status_json).collect::<Vec<_>>(),
        })),
        Err(_) => not_found(),
    }
}

async fn stop(
    engine: &std::sync::Arc<OrchestratorEngine>,
    caller: CallerRef,
    input: SubagentRefInput,
) -> FrameReply {
    match engine.stop(caller, RunId(input.subagent_id)).await {
        Ok(StopOutcome::Sent) => FrameReply::ok_with(json!({ "outcome": "sent" })),
        Ok(StopOutcome::AlreadyEnded) => FrameReply::ok_with(json!({ "outcome": "already_ended" })),
        Err(StopError::NotFound) => not_found(),
    }
}

/// `message_parent`: wake the calling depth-1 sub-agent's orchestrator with
/// its text. The reply does not wait for the wake; the orchestrator reads the
/// message when its chat next takes a turn.
pub(crate) fn message_parent(
    engine: &std::sync::Arc<OrchestratorEngine>,
    caller: CallerRef,
    input: MessageParentInput,
) -> FrameReply {
    let chars = input.message.chars().count();
    if chars == 0 || chars > 8_000 {
        return refusal("invalid_input", "message must be 1 to 8000 characters");
    }
    match engine.message_parent(caller, input.message) {
        Ok(()) => FrameReply::ok_with(json!({ "outcome": "sent" })),
        Err(error @ MessageParentError::NotASubagent) => {
            refusal("not_a_subagent", error.to_string())
        }
        Err(error @ MessageParentError::Nested) => refusal("nested", error.to_string()),
    }
}

impl HostPlanServer {
    /// `subagent_answer`: resolve an open `ask_user` question of one of the
    /// caller's own runs, first answer against the user wins. The question's
    /// card closes exactly as if the user had answered: the fanout drops it in
    /// every renderer and the relay settles its ticket on the same event.
    pub(crate) fn answer_subagent_question(
        &self,
        engine: &std::sync::Arc<OrchestratorEngine>,
        caller: CallerRef,
        input: SubagentAnswerInput,
    ) -> FrameReply {
        if input.answers.is_empty() {
            return refusal("invalid_input", "answers must not be empty");
        }
        let run = RunId(input.subagent_id);
        let Some(child_session) = engine.child_session_for_caller(&caller, &run) else {
            return not_found();
        };
        let question_id = input.question_id.clone();
        let Some(answered) =
            self.answer_ask_for_session(&child_session.0, &input.question_id, input.answers)
        else {
            return not_found();
        };
        log::info!(
            "[host-mcp] subagent_answer resolved a question of run {} for session {}",
            crate::logging::redact_session_id(&run.0),
            crate::logging::redact_session_id(&child_session.0)
        );
        let event = QuestionCancelledEvent {
            agent_id: AgentId(answered.agent_id),
            session_id: SessionId(answered.card_session_id),
            question_id,
        };
        let card_session = event.session_id.0.clone();
        events::fan_out(
            &self.sinks,
            Some(card_session.as_str()),
            events::EVENT_QUESTION_CANCELLED,
            &event,
        );
        FrameReply::ok_with(json!({ "outcome": "answered" }))
    }
}

#[cfg(test)]
#[path = "subagents_tests.rs"]
mod subagents_tests;
