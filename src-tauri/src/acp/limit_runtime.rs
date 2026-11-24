//! Shared pieces of the Claude, Codex and Antigravity limit supervisors: the
//! turn clock, the set of accounts that just reached a limit, the roomiest
//! account for a new chat, and moving one chat onto a new agent. Each
//! supervisor keeps its own schedule and its own decision.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};

use crate::acp::claude_limit::decide::{first_with_room, rank_alternatives, Candidate};
use crate::acp::config::{AgentId, SessionId};
use crate::acp::manager::{HostMove, LaunchRecipe};
use crate::acp::{AcpManager, LaunchChat, SpawnScope};
use crate::logging::redact_session_id;
use crate::web::prompt_turn::{
    accept_prompt_turn, complete_prompt_turn, PromptAdmission, PromptTurnRequest,
};
use crate::web::WsRelaySink;

/// Numbers the turns of every chat. A supervisor acts on one input at a
/// time, so a `TurnStarted` can wait in its inbox while it resumes another
/// chat; it reads the clock instead, which the driver sets before the agent
/// sees the prompt.
#[derive(Default)]
pub(crate) struct TurnClock {
    /// The last turn number, and the last turn of each chat whose
    /// `TurnStarted` the supervisor has not handled yet.
    turns: parking_lot::Mutex<(u64, HashMap<String, u64>)>,
}

impl TurnClock {
    pub(crate) fn start(&self, session_id: &str) -> u64 {
        let mut turns = self.turns.lock();
        turns.0 += 1;
        let seq = turns.0;
        turns.1.insert(session_id.to_string(), seq);
        seq
    }

    /// The last turn number of any chat.
    pub(crate) fn now(&self) -> u64 {
        self.turns.lock().0
    }

    /// A turn of the chat started after `mark` (a [`Self::now`] reading).
    pub(crate) fn started_since(&self, session_id: &str, mark: u64) -> bool {
        self.turns
            .lock()
            .1
            .get(session_id)
            .is_some_and(|seq| *seq > mark)
    }

    /// The supervisor handled turn `seq`. A later turn keeps its entry until
    /// its own `TurnStarted` is handled.
    pub(crate) fn settle(&self, session_id: &str, seq: u64) {
        let mut turns = self.turns.lock();
        if turns.1.get(session_id) == Some(&seq) {
            turns.1.remove(session_id);
        }
    }
}

/// Accounts that reached a limit, until their reset. The supervisor marks
/// them; a new chat's Account pick reads them. A cached usage read can still
/// show room for an Account that just reached its limit.
#[derive(Default)]
pub(crate) struct LimitedAccounts(parking_lot::Mutex<HashMap<String, SystemTime>>);

impl LimitedAccounts {
    pub(crate) fn mark(&self, account_id: &str, until: SystemTime) {
        self.0.lock().insert(account_id.to_string(), until);
    }

    pub(crate) fn snapshot(&self) -> HashMap<String, SystemTime> {
        self.0.lock().clone()
    }
}

/// The Account of `candidates` with the most room for a new chat: the
/// ranking a chat at its limit moves by, skipping the Accounts in
/// `limited_until` whose limit has not reset at `now`. `None` when none has
/// room.
#[must_use]
pub(crate) fn roomiest_account(
    candidates: &[Candidate],
    limited_until: &HashMap<String, SystemTime>,
    now: SystemTime,
) -> Option<String> {
    first_with_room(&rank_alternatives(candidates), limited_until, now).cloned()
}

pub(crate) enum Relaunch {
    /// The chat runs on this new agent.
    Done(AgentId),
    /// Another agent than the one being replaced holds the chat now: a
    /// client opened it there.
    HeldElsewhere(AgentId),
}

/// `error` with the chat's id shortened, for the log.
pub(crate) fn scrub(error: &str, session_id: &SessionId) -> String {
    if session_id.0.is_empty() {
        return error.to_string();
    }
    error.replace(&session_id.0, &redact_session_id(&session_id.0))
}

/// The live agent that holds the chat, other than `except`.
pub(crate) async fn live_owner(
    acp: &AcpManager,
    session_id: &SessionId,
    except: Option<&AgentId>,
) -> Option<AgentId> {
    for agent in acp.list_agents() {
        if Some(&agent) == except {
            continue;
        }
        if acp
            .owns_session(&agent, session_id.clone())
            .await
            .unwrap_or(false)
        {
            return Some(agent);
        }
    }
    None
}

/// Take the chat off `old_agent` and open it on a new agent started from its
/// recipe. `moving` holds the chat. `label` is the log prefix (`claude-limit`
/// or `codex-limit`).
pub(crate) async fn relaunch(
    acp: &AcpManager,
    moving: &HostMove,
    session_id: &SessionId,
    old_agent: Option<&AgentId>,
    label: &str,
) -> Result<Relaunch, String> {
    let recipe = acp
        .launch_recipe(session_id)
        .ok_or_else(|| "the host has no launch recipe for this chat".to_string())?;
    if let Some(holder) = live_owner(acp, session_id, old_agent).await {
        return Ok(Relaunch::HeldElsewhere(holder));
    }
    if let Some(old) = old_agent {
        release(acp, old, session_id, label).await?;
    }
    let spawned = acp
        .spawn_with_command_id(
            recipe.launch.config.clone(),
            None,
            SpawnScope {
                project_id: recipe.launch.project_id.clone(),
                chat: LaunchChat::Existing(session_id.clone()),
            },
        )
        .await?;
    // `release` stops the old agent. The other supervisor may then forget the
    // stored recipe. Reopen from the copy taken before that stop.
    reopen(acp, moving, &spawned.agent_id, session_id, &recipe, label).await?;
    Ok(Relaunch::Done(spawned.agent_id))
}

/// Cancel the chat's held turn on `old`, then stop `old` when no other
/// durable chat runs on it, or close only this chat there so the others
/// keep their agent and their turns.
async fn release(
    acp: &AcpManager,
    old: &AgentId,
    session_id: &SessionId,
    label: &str,
) -> Result<(), String> {
    if acp
        .is_turn_active(old, session_id.clone())
        .await
        .unwrap_or(false)
    {
        if let Err(error) = acp.cancel_prompt(old, session_id.clone()).await {
            log::warn!(
                "[{label}] cancelling the held turn failed: {}",
                scrub(&error, session_id)
            );
        }
    }
    let mut shared = false;
    for other in acp.agent_session_ids(old).await.unwrap_or_default() {
        let other = SessionId(other);
        if other != *session_id
            && !acp
                .is_ephemeral_session(old, other.clone())
                .await
                .unwrap_or(true)
        {
            shared = true;
            break;
        }
    }
    if !shared {
        if let Err(error) = acp.kill(old).await {
            log::warn!("[{label}] stopping agent {old} failed: {error}");
        }
        return Ok(());
    }
    log::info!(
        "[{label}] agent {old} keeps its other chats; closing only chat {} on it",
        redact_session_id(&session_id.0)
    );
    acp.close_session_keeping_recipe(old, session_id.clone())
        .await
        .map_err(|error| format!("the chat did not close on its old agent ({error})"))
}

async fn reopen(
    acp: &AcpManager,
    moving: &HostMove,
    agent_id: &AgentId,
    session_id: &SessionId,
    recipe: &LaunchRecipe,
    label: &str,
) -> Result<(), String> {
    let resumed = acp
        .resume_session_in_move(
            moving,
            agent_id,
            session_id.clone(),
            recipe.cwd.clone(),
            recipe.additional_directories.clone(),
            recipe.mcp_servers.clone(),
        )
        .await;
    let Err(resume_error) = resumed else {
        return Ok(());
    };
    log::info!(
        "[{label}] resuming chat {} failed ({}); loading it instead",
        redact_session_id(&session_id.0),
        scrub(&resume_error, session_id)
    );
    acp.load_session_in_move(
        moving,
        agent_id,
        session_id.clone(),
        recipe.cwd.clone(),
        recipe.additional_directories.clone(),
        recipe.mcp_servers.clone(),
    )
    .await
    .map(|_| ())
    .map_err(|load_error| format!("resume: {resume_error}; load: {load_error}"))
}

/// Start the continuation turn and return once the agent accepted it, so a
/// client that waited for the move finds the chat busy on the new agent.
pub(crate) async fn send_notice(
    acp: &Arc<AcpManager>,
    relay: &Arc<WsRelaySink>,
    agent_id: &AgentId,
    session_id: &SessionId,
    notice: &str,
    label: &str,
) {
    let chat = redact_session_id(&session_id.0);
    let blocks = vec![ContentBlock::Text(TextContent::new(notice))];
    let request = PromptTurnRequest {
        agent_id: agent_id.clone(),
        session_id: session_id.clone(),
        content: blocks.clone(),
        record_content: blocks,
        turn_id: Some(format!("{label}-{}", uuid::Uuid::new_v4())),
    };
    match accept_prompt_turn(acp, relay, request).await {
        Ok(PromptAdmission::Running(running)) => {
            let acp = Arc::clone(acp);
            let label = label.to_string();
            tokio::spawn(async move {
                match complete_prompt_turn(&acp, running).await {
                    Ok(reason) => {
                        log::info!("[{label}] chat {chat} continued; turn ended ({reason:?})")
                    }
                    Err(_) => log::warn!("[{label}] the continuation turn of chat {chat} failed"),
                }
            });
        }
        Ok(PromptAdmission::Receipt(_)) => log::info!("[{label}] chat {chat} continued"),
        Err(error) => log::warn!(
            "[{label}] the continuation prompt of chat {chat} failed ({})",
            error.kind()
        ),
    }
}

#[cfg(test)]
#[path = "limit_runtime_tests.rs"]
mod tests;
