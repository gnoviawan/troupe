//! Worker chats an agent starts with the `start_chat` host tool. Each is a
//! real, persisted chat in the caller's project and folder, on its own host
//! agent, started and prompted with no client connected. A managed Claude
//! caller's worker runs on the signed-in Account with the most room, written
//! as the chat's own Account, so the limit supervisor moves it like any chat.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use agent_client_protocol::schema::v1::{ContentBlock, SessionModeState, TextContent};
use parking_lot::Mutex;
use serde::Serialize;

use crate::acp::claude_agent::{ClaudeLaunchAuth, LaunchChat};
use crate::acp::claude_limit::{roomiest_account, AccountRef, CLAUDE_CONFIG_ID};
use crate::acp::config::{AgentId, SessionId};
use crate::acp::manager::{LaunchRecipe, SessionCreationContext};
use crate::acp::session_persistence::{SessionKind, UNASSIGNED_PROJECT_ID};
use crate::acp::{AcpManager, SignInCheck, SpawnScope};
use crate::logging::redact_session_id;
use crate::web::prompt_turn::{
    accept_prompt_turn, complete_prompt_turn, PromptAdmission, PromptTurnError, PromptTurnRequest,
};
use crate::web::WsRelaySink;

/// Worker chats one caller may have running at once.
pub const MAX_LIVE_CHILDREN: usize = 6;

/// The Claude Account a new worker chat runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountChoice {
    /// The signed-in Account with the most room.
    Auto,
    Account(String),
}

impl AccountChoice {
    /// No value, a blank one and `auto` choose automatically.
    #[must_use]
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            None | Some("" | "auto") => Self::Auto,
            Some(id) => Self::Account(id.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartChatRequest {
    pub caller: SessionId,
    pub prompt: String,
    pub title: Option<String>,
    pub account: AccountChoice,
}

/// The `start_chat` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedChat {
    pub chat_id: String,
    /// The chat's own Claude Account; `None` when it follows its Project.
    pub account: Option<AccountRef>,
    /// Why the chat follows its Project, or what did not apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerChatError {
    Unavailable,
    /// The caller is not open on a host agent the host launched for a client.
    NoRecipe,
    /// The caller is itself a worker chat.
    Grandchild,
    QuickChat,
    ProjectBusy,
    CapReached,
    UnknownAccount(String),
    /// The chat does not exist or another chat started it.
    NotYourChat(String),
    Busy(String),
    /// No live agent holds the chat.
    Stopped(String),
    Failed(String),
}

impl WorkerChatError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::NoRecipe => "no_launch_recipe",
            Self::Grandchild => "worker_chat",
            Self::QuickChat => "quick_chat",
            Self::ProjectBusy => "project_busy",
            Self::CapReached => "too_many_chats",
            Self::UnknownAccount(_) => "unknown_account",
            Self::NotYourChat(_) => "not_your_chat",
            Self::Busy(_) => "busy",
            Self::Stopped(_) => "stopped",
            Self::Failed(_) => "failed",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable => "Worker chats are not available on this Troupe host.".to_string(),
            Self::NoRecipe => "Troupe cannot start a chat from this one: the host did not launch \
                this chat's agent for a client, so it has nothing to start the new chat with. \
                Reopen this chat in Troupe, then try again."
                .to_string(),
            Self::Grandchild => "This chat was started by another chat, so it cannot start chats \
                of its own. Report back to the chat that started you instead."
                .to_string(),
            Self::QuickChat => "Quick chats cannot start other chats; use a project chat.".to_string(),
            Self::ProjectBusy => "This chat's project is being removed or archived.".to_string(),
            Self::CapReached => format!(
                "This chat already has {MAX_LIVE_CHILDREN} worker chats running, the most it may \
                 have. Wait for one to finish (read it with read_chat) or give more work to one \
                 with send_to_chat."
            ),
            Self::UnknownAccount(id) => format!(
                "No Claude account with ID {id} is signed in to Troupe. Pass \"auto\" to use the \
                 account with the most room."
            ),
            Self::NotYourChat(id) => format!(
                "Chat {id} was not started by this chat. send_to_chat only reaches chats this \
                 chat started with start_chat."
            ),
            Self::Busy(id) => format!(
                "Chat {id} is still working on its current message, so the new one was not sent. \
                 Check its progress with read_chat and send again when it has answered."
            ),
            Self::Stopped(id) => format!(
                "Chat {id} is not running anymore: its agent stopped. Start a new chat with \
                 start_chat and pass what it needs, or ask the user to reopen chat {id}."
            ),
            Self::Failed(reason) => reason.clone(),
        }
    }
}

/// One worker chat of a caller: reserved while it starts, then its id.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Starting(u64),
    Started(SessionId),
}

#[derive(Default)]
struct Children {
    next: u64,
    by_parent: HashMap<SessionId, Vec<Slot>>,
}

/// Where a new worker chat's agent gets its Claude Account.
enum ChildAccount {
    Pinned(AccountRef),
    FollowsProject { note: Option<String> },
}

pub struct WorkerChats {
    acp: Weak<AcpManager>,
    relay: Arc<WsRelaySink>,
    runtime: tokio::runtime::Handle,
    children: Mutex<Children>,
}

/// Install the worker-chat service on `acp`'s host tools.
pub fn install(
    acp: &Arc<AcpManager>,
    relay: &Arc<WsRelaySink>,
    runtime: &tokio::runtime::Handle,
) -> Arc<WorkerChats> {
    let chats = Arc::new(WorkerChats {
        acp: Arc::downgrade(acp),
        relay: Arc::clone(relay),
        runtime: runtime.clone(),
        children: Mutex::new(Children::default()),
    });
    acp.set_worker_chats(Arc::clone(&chats));
    chats
}

impl WorkerChats {
    /// Start a worker chat for `request.caller` and send it its first prompt.
    /// Runs on the host runtime, not the host tool listener's thread.
    pub async fn start(self: &Arc<Self>, request: StartChatRequest) -> Result<StartedChat, WorkerChatError> {
        let this = Arc::clone(self);
        self.runtime
            .spawn(async move { this.start_here(request).await })
            .await
            .map_err(|_| WorkerChatError::Failed("starting the chat stopped unexpectedly".to_string()))?
    }

    /// Send `prompt` to the worker chat `child` of `caller`.
    pub async fn send(
        self: &Arc<Self>,
        caller: SessionId,
        child: SessionId,
        prompt: String,
    ) -> Result<(), WorkerChatError> {
        let this = Arc::clone(self);
        self.runtime
            .spawn(async move { this.send_here(&caller, &child, prompt).await })
            .await
            .map_err(|_| WorkerChatError::Failed("sending the message stopped unexpectedly".to_string()))?
    }

    async fn start_here(&self, request: StartChatRequest) -> Result<StartedChat, WorkerChatError> {
        let acp = self.acp.upgrade().ok_or(WorkerChatError::Unavailable)?;
        let persistence = acp.persistence().ok_or(WorkerChatError::Unavailable)?;
        let caller = redact_session_id(&request.caller.0);
        let recipe = acp
            .launch_recipe(&request.caller)
            .ok_or(WorkerChatError::NoRecipe)?;
        let metadata = persistence
            .metadata(&request.caller.0)
            .map_err(|_| WorkerChatError::NoRecipe)?;
        if metadata.started_by.is_some() {
            return Err(WorkerChatError::Grandchild);
        }
        if metadata.kind == SessionKind::Quick {
            return Err(WorkerChatError::QuickChat);
        }
        let project_id = recipe.launch.project_id.clone();
        if project_id.as_deref().is_some_and(|project| {
            crate::web::project_registry::installed()
                .is_some_and(|registry| registry.is_removing(project))
        }) {
            return Err(WorkerChatError::ProjectBusy);
        }
        let slot = self.reserve(&acp, &request.caller)?;
        let account = child_account(&acp, &recipe, &request.account).await?;
        let chat = match &account {
            ChildAccount::Pinned(account) => LaunchChat::New {
                account_id: account.id.clone(),
            },
            ChildAccount::FollowsProject { .. } => LaunchChat::None,
        };
        let agent_id = acp
            .spawn_with_command_id(
                recipe.launch.config.clone(),
                None,
                SpawnScope {
                    project_id: project_id.clone(),
                    chat,
                },
            )
            .await
            .map_err(|error| {
                log::warn!("[worker-chats] chat {caller} could not start a worker agent: {error}");
                WorkerChatError::Failed(format!("The new chat's agent did not start: {error}"))
            })?
            .agent_id;
        let created = match acp
            .new_session_with_context(
                &agent_id,
                recipe.cwd.clone(),
                recipe.mcp_servers.clone().unwrap_or_default(),
                SessionCreationContext {
                    project_id: project_id.clone(),
                    additional_directories: recipe.additional_directories.clone(),
                    started_by: Some(request.caller.0.clone()),
                    ..SessionCreationContext::default()
                },
            )
            .await
        {
            Ok(created) => created,
            Err(error) => {
                log::warn!("[worker-chats] chat {caller}: the worker agent {agent_id} did not open a chat");
                let _ = acp.kill(&agent_id).await;
                return Err(WorkerChatError::Failed(format!(
                    "The new chat did not open: {error}"
                )));
            }
        };
        let child = created.session_id.clone();
        let mut note = None;
        let account = match account {
            ChildAccount::Pinned(account) => {
                let pinned = acp
                    .claude_agent()
                    .set_chat_account(
                        project_id.as_deref().unwrap_or(UNASSIGNED_PROJECT_ID),
                        &child.0,
                        Some(&account.id),
                        None,
                        SignInCheck::Skip,
                    )
                    .await;
                if let Err(error) = pinned {
                    log::warn!(
                        "[worker-chats] the account of worker chat {} was not saved: {error}",
                        redact_session_id(&child.0)
                    );
                    note = Some(
                        "The chat runs on this account now, but Troupe could not save it as the \
                         chat's account."
                            .to_string(),
                    );
                }
                Some(account)
            }
            ChildAccount::FollowsProject { note: why } => {
                note = why;
                None
            }
        };
        inherit_mode(&acp, &request.caller, &agent_id, &child, created.modes.as_ref()).await;
        if let Some(title) = request.title.filter(|title| !title.trim().is_empty()) {
            if let Err(error) = crate::acp::manager::record_local_title(
                &persistence,
                acp.sinks(),
                agent_id.clone(),
                child.0.clone(),
                title,
            )
            .await
            {
                log::warn!(
                    "[worker-chats] the title of worker chat {} was not saved: {error}",
                    redact_session_id(&child.0)
                );
            }
        }
        slot.started(child.clone());
        if let Err(error) = self.prompt(&acp, &agent_id, &child, request.prompt).await {
            return Err(WorkerChatError::Failed(format!(
                "Chat {} started, but its first message was not sent ({}). Send it again with \
                 send_to_chat.",
                child.0,
                error.message()
            )));
        }
        log::info!(
            "[worker-chats] chat {caller} started worker chat {} on agent {agent_id} ({})",
            redact_session_id(&child.0),
            if account.is_some() { "own account" } else { "project account" }
        );
        Ok(StartedChat {
            chat_id: child.0,
            account,
            note,
        })
    }

    async fn send_here(
        &self,
        caller: &SessionId,
        child: &SessionId,
        prompt: String,
    ) -> Result<(), WorkerChatError> {
        let acp = self.acp.upgrade().ok_or(WorkerChatError::Unavailable)?;
        let persistence = acp.persistence().ok_or(WorkerChatError::Unavailable)?;
        let started_by = persistence
            .metadata(&child.0)
            .ok()
            .and_then(|metadata| metadata.started_by);
        if started_by.as_deref() != Some(caller.0.as_str()) {
            log::info!(
                "[worker-chats] chat {} may not send to chat {}",
                redact_session_id(&caller.0),
                redact_session_id(&child.0)
            );
            return Err(WorkerChatError::NotYourChat(child.0.clone()));
        }
        let agent_id = live_owner(&acp, child)
            .await
            .ok_or_else(|| WorkerChatError::Stopped(child.0.clone()))?;
        self.prompt(&acp, &agent_id, child, prompt).await?;
        log::info!(
            "[worker-chats] chat {} sent a message to worker chat {}",
            redact_session_id(&caller.0),
            redact_session_id(&child.0)
        );
        Ok(())
    }

    /// Start a turn of `prompt` and return once the agent accepted it.
    async fn prompt(
        &self,
        acp: &Arc<AcpManager>,
        agent_id: &AgentId,
        session_id: &SessionId,
        prompt: String,
    ) -> Result<(), WorkerChatError> {
        let blocks = vec![ContentBlock::Text(TextContent::new(prompt))];
        let request = PromptTurnRequest {
            agent_id: agent_id.clone(),
            session_id: session_id.clone(),
            content: blocks.clone(),
            record_content: blocks,
            turn_id: Some(format!("worker-chat-{}", uuid::Uuid::new_v4())),
        };
        let chat = redact_session_id(&session_id.0);
        match accept_prompt_turn(acp, &self.relay, request).await {
            Ok(PromptAdmission::Running(running)) => {
                let acp = Arc::clone(acp);
                tokio::spawn(async move {
                    match complete_prompt_turn(&acp, running).await {
                        Ok(reason) => log::info!("[worker-chats] worker chat {chat} turn ended ({reason:?})"),
                        Err(_) => log::warn!("[worker-chats] the turn of worker chat {chat} failed"),
                    }
                });
                Ok(())
            }
            Ok(PromptAdmission::Receipt(_)) => Ok(()),
            Err(error) => {
                log::warn!("[worker-chats] worker chat {chat} refused a message ({})", error.kind());
                Err(match error {
                    PromptTurnError::Busy { .. } => WorkerChatError::Busy(session_id.0.clone()),
                    PromptTurnError::NotOwned => WorkerChatError::Stopped(session_id.0.clone()),
                    PromptTurnError::Failed(reason) => WorkerChatError::Failed(reason),
                })
            }
        }
    }

    /// Hold a place for one more worker chat of `parent`, or refuse at the cap.
    fn reserve(&self, acp: &AcpManager, parent: &SessionId) -> Result<Reservation<'_>, WorkerChatError> {
        let mut children = self.children.lock();
        children.next += 1;
        let id = children.next;
        let slots = children.by_parent.entry(parent.clone()).or_default();
        let live = slots
            .iter()
            .filter(|slot| match slot {
                Slot::Starting(_) => true,
                Slot::Started(child) => is_live(acp, child),
            })
            .count();
        if live >= MAX_LIVE_CHILDREN {
            log::info!(
                "[worker-chats] chat {} is at its {MAX_LIVE_CHILDREN} worker chats",
                redact_session_id(&parent.0)
            );
            return Err(WorkerChatError::CapReached);
        }
        slots.push(Slot::Starting(id));
        Ok(Reservation {
            chats: self,
            parent: parent.clone(),
            id,
            started: false,
        })
    }
}

/// A held place for one worker chat; dropped unstarted, it frees the place.
struct Reservation<'a> {
    chats: &'a WorkerChats,
    parent: SessionId,
    id: u64,
    started: bool,
}

impl Reservation<'_> {
    fn started(mut self, child: SessionId) {
        self.set(Some(child));
        self.started = true;
    }

    fn set(&self, child: Option<SessionId>) {
        let mut children = self.chats.children.lock();
        let Some(slots) = children.by_parent.get_mut(&self.parent) else {
            return;
        };
        if let Some(index) = slots.iter().position(|slot| *slot == Slot::Starting(self.id)) {
            match child {
                Some(child) => slots[index] = Slot::Started(child),
                None => {
                    slots.remove(index);
                }
            }
        }
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.started {
            self.set(None);
        }
    }
}

/// A live agent the host launched holds the chat.
fn is_live(acp: &AcpManager, chat: &SessionId) -> bool {
    acp.launch_recipe(chat)
        .is_some_and(|recipe| acp.agent_launch(&recipe.agent_id).is_some())
}

async fn live_owner(acp: &AcpManager, chat: &SessionId) -> Option<AgentId> {
    for agent in acp.list_agents() {
        if acp.owns_session(&agent, chat.clone()).await.unwrap_or(false) {
            return Some(agent);
        }
    }
    None
}

/// A managed Claude caller on an Account gets the chosen Account; any other
/// caller's worker follows its Project.
async fn child_account(
    acp: &AcpManager,
    recipe: &LaunchRecipe,
    choice: &AccountChoice,
) -> Result<ChildAccount, WorkerChatError> {
    let managed = recipe.launch.config.config_id.as_deref() == Some(CLAUDE_CONFIG_ID)
        && matches!(recipe.launch.claude, ClaudeLaunchAuth::Account(_));
    if !managed {
        return Ok(ChildAccount::FollowsProject {
            note: Some(
                "This chat does not run on a Claude account Troupe manages, so the new chat uses \
                 its project's agent setup and `account` was ignored."
                    .to_string(),
            ),
        });
    }
    let snapshot = acp
        .claude_agent()
        .profile_list()
        .await
        .map_err(WorkerChatError::Failed)?;
    let id = match choice {
        AccountChoice::Auto => match roomiest_account(acp, &snapshot).await {
            Some(id) => id,
            None => {
                return Ok(ChildAccount::FollowsProject {
                    note: Some(
                        "No signed-in Claude account has room now, so the new chat uses its \
                         project's account."
                            .to_string(),
                    ),
                })
            }
        },
        AccountChoice::Account(id) => id.clone(),
    };
    let Some(account) = snapshot.profiles.iter().find(|account| account.id == id) else {
        return Err(WorkerChatError::UnknownAccount(id));
    };
    if matches!(choice, AccountChoice::Account(_)) && account.signed_in != Some(true) {
        return Err(WorkerChatError::UnknownAccount(id));
    }
    Ok(ChildAccount::Pinned(AccountRef {
        id,
        name: account.name.clone(),
    }))
}

/// Start the worker in the mode its caller runs in now, so a caller that
/// allowed its tools does not leave its worker waiting on a permission no
/// client is there to answer.
async fn inherit_mode(
    acp: &AcpManager,
    caller: &SessionId,
    agent_id: &AgentId,
    child: &SessionId,
    modes: Option<&SessionModeState>,
) {
    let (Some(modes), Some(persistence)) = (modes, acp.persistence()) else {
        return;
    };
    let Some(mode) = persistence.current_mode_id(&caller.0).await else {
        return;
    };
    if *modes.current_mode_id.0 == *mode
        || !modes.available_modes.iter().any(|available| *available.id.0 == *mode)
    {
        return;
    }
    if let Err(error) = acp.set_mode(agent_id, child.clone(), mode).await {
        log::warn!(
            "[worker-chats] worker chat {} kept its own mode: {error}",
            redact_session_id(&child.0)
        );
    }
}

#[cfg(test)]
#[path = "real_loop_tests.rs"]
mod real_loop_tests;
