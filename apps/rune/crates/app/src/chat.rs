//! Chats in the workspace: a rail row opens its chat's tab, a visible chat tab reopens through
//! the model's history open (`history_host.rs`) and follows its live log (`subscribe`), and
//! each session event the link delivers becomes a `model::acp` event. A chat the launcher
//! started is installed here from the launch.

use std::collections::BTreeSet;

use client::cursor::Cursor;
use client::ws::Command;
use model::acp::{self, Effect, Env, Event, HistoryPayload};
use model::acp::types::{ChatMessage, Object, Session, SessionStatus};
use model::workspace::{Event as WorkspaceEvent, Tab};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsEvent, WsEventType};

use model::acp::prompt::{DispatchOutcome, PromptSettled};

use crate::State;
use crate::sync::{Pending, Uuid};

/// `AGENT_DEAD_MARKERS`: a refusal that means the agent process went away.
const AGENT_DEAD: [&str; 2] = ["agent thread is no longer running", "agent thread dropped the reply"];

/// `isPromptTurnInProgressError`: the relay reports a busy turn as `rate_limited`, its message
/// keeping the `ACP_TURN_IN_PROGRESS` prefix or not.
fn is_turn_in_progress(e: &WsError) -> bool {
    e.code == "rate_limited" || e.message.contains("ACP_TURN_IN_PROGRESS")
}

/// The chat side of the app state.
#[derive(Default)]
pub struct ChatUi {
    pub acp: acp::State,
    pub reader: ui::chat::Reader,
    /// Sessions whose payload was asked for (loaded or not).
    pub(crate) requested: BTreeSet<String>,
    /// Sessions whose payload is in.
    pub loaded: BTreeSet<String>,
    /// A chat to open once its project's workspace is showing.
    pub(crate) open_when_shown: Option<(String, String)>,
    /// The Markdown of the table the full-window view shows.
    pub zoomed_table: Option<String>,
    /// The link waiting for the user's "Open" (`ExternalLinkConfirm`); only one at a time.
    pub link_confirm: Option<String>,
    /// Turn ends the model asked for, run after the events already delivered.
    pub(crate) deferred: Vec<Event>,
    /// Busy-queue retries the model asked for: (due on the frame clock, session, generation).
    retries: Vec<(f64, String, u64)>,
    /// Waits for re-advertised options a model pick asked for: (due on the frame clock, run).
    pub(crate) readvertise: Vec<(f64, model::acp::RunId)>,
    /// Per agent config, the saved composer picks as last written, or the patches waiting
    /// for the first read (`persistComposerOptions`' per-key queue).
    pub(crate) saved_picks: std::collections::BTreeMap<String, crate::controls::SavedPicks>,
    /// Cancel-then-switches waiting for their cancelled turns to end: session to (config,
    /// when the wait gives up on the frame clock).
    pub(crate) pending_arm: std::collections::BTreeMap<String, (String, f64)>,
    /// Pictures for images in replies; set up with the app's waker.
    pub images: Option<std::rc::Rc<crate::chat_images::ChatImages>>,
}

/// `ACP_EVENTS` to store action, for the session events the model handles.
fn action(kind: WsEventType) -> Option<&'static str> {
    Some(match kind {
        WsEventType::UserPrompt => "_onUserPrompt",
        WsEventType::MessageChunk => "_onMessageChunk",
        WsEventType::ToolCall => "_onToolCall",
        WsEventType::ToolCallUpdate => "_onToolCallUpdate",
        WsEventType::PlanUpdate => "_onPlanUpdate",
        WsEventType::CommandsUpdate => "_onCommandsUpdate",
        WsEventType::SessionInfoUpdate => "_onSessionInfoUpdate",
        WsEventType::UsageUpdate => "_onUsageUpdate",
        WsEventType::ModeUpdate => "_onModeUpdate",
        WsEventType::ModelsUpdate => "_onModelsUpdate",
        WsEventType::ConfigOptionsUpdate => "_onConfigOptionsUpdate",
        WsEventType::PermissionRequest => "_onPermissionRequest",
        WsEventType::QuestionRequest => "_onQuestionRequest",
        WsEventType::QuestionCancelled => "_onQuestionCancelled",
        WsEventType::ElicitationRequest => "_onElicitationRequest",
        WsEventType::BrowserTicketClosed => "_onBrowserTicketClosed",
        WsEventType::PromptComplete => "_onPromptComplete",
        WsEventType::AgentWorking => "_onAgentWorking",
        WsEventType::BackgroundCommands => "_onBackgroundCommands",
        _ => return None,
    })
}

/// The model event for one delivered session event, with the event's seq where the action
/// takes one (`commands_update` takes a log point).
pub fn to_event(event: &WsEvent) -> Option<Result<Event, serde_json::Error>> {
    let name = action(event.kind)?;
    let seq = event.seq as f64;
    let args = match event.kind {
        WsEventType::CommandsUpdate => json!([event.payload, { "logEpoch": null, "seq": event.seq }]),
        WsEventType::UserPrompt
        | WsEventType::MessageChunk
        | WsEventType::ToolCall
        | WsEventType::ToolCallUpdate
        | WsEventType::PermissionRequest
        | WsEventType::QuestionRequest
        | WsEventType::ElicitationRequest
        | WsEventType::PromptComplete => json!([event.payload, seq]),
        _ => json!([event.payload]),
    };
    let args = match args {
        Value::Array(mut items) if items.len() == 1 => items.remove(0),
        other => other,
    };
    Some(serde_json::from_value(json!({ name: args })))
}

impl State {
    /// A rail chat row: show the chat's project, then its tab.
    pub fn open_chat(&mut self, session_id: &str) {
        let project = self.chats.get(session_id).and_then(|c| c.project_id.clone());
        match project {
            Some(project) if self.workspace.project() != Some(project.as_str()) => {
                self.chat.open_when_shown = Some((project.clone(), session_id.to_owned()));
                self.select_project(&project);
            }
            _ => self.add_chat_tab(session_id),
        }
    }

    fn add_chat_tab(&mut self, session_id: &str) {
        log::info!("[chat] open {session_id}");
        self.workspace.step(WorkspaceEvent::AddAgentChatTab(session_id.to_owned(), None));
    }

    /// Opens a deferred chat once its project shows, and loads every chat tab not loaded yet.
    pub(crate) fn sync_chats(&mut self) {
        if let Some((project, _)) = &self.chat.open_when_shown
            && self.workspace.project() == Some(project.as_str())
            && self.workspace.record().is_some()
            && let Some((_, session)) = self.chat.open_when_shown.take()
        {
            self.add_chat_tab(&session);
        }
        let tabs: Vec<String> = model::workspace::leaves(self.workspace.root())
            .into_iter()
            .flat_map(|leaf| leaf.tabs.iter())
            .filter_map(|tab| match tab {
                Tab::AgentChat { session_id, .. } => Some(session_id.clone()),
                _ => None,
            })
            .collect();
        // The model remaps a switched chat's tab only when it knows the tab is open.
        self.chat.acp.book.open_chat_tabs = tabs.iter().cloned().collect();
        // A closed chat's cancel-then-switch has no composer left to arm.
        self.chat.pending_arm.retain(|session, _| tabs.contains(session));
        for session in tabs {
            if self.host.is_some() && self.chat.requested.insert(session.clone()) {
                self.reopen_chat(&session);
            }
        }
    }

    /// The payload reply: install the transcript, then follow the log after it.
    pub(crate) fn chat_payload(&mut self, session_id: String, result: Result<Option<Value>, WsError>) {
        let payload = match result {
            Ok(Some(payload)) => payload,
            Ok(None) => {
                log::warn!("[chat] no history for {session_id}");
                return;
            }
            Err(e) => {
                log::warn!("[chat] reading the history of {session_id} failed: {}: {}", e.code, e.message);
                self.chat.requested.remove(&session_id);
                return;
            }
        };
        let messages: Vec<ChatMessage> = payload
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| serde_json::from_value(m.clone()).ok())
            .collect();
        let tools: Vec<Object> = payload
            .get("toolCalls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_object().cloned())
            .collect();
        let meta = payload.get("metadata").cloned().unwrap_or(Value::Null);
        let text = |k: &str| meta.get(k).and_then(Value::as_str).map(str::to_owned);
        if let Some(ns) = text("stableAgentNamespace") {
            self.composer.namespaces.insert(session_id.clone(), ns);
        }
        log::info!("[chat] history of {session_id}: {} messages, {} tool calls", messages.len(), tools.len());
        let session = Session {
            id: session_id.clone(),
            agent_id: text("agentId").unwrap_or_default(),
            status: SessionStatus::Active,
            title: text("title"),
            active_turn: meta.get("turnActive").and_then(Value::as_bool).unwrap_or(false),
            open_turn_id: None,
            agent_working: None,
            replaying: None,
            opencode_notice: None,
            last_error: None,
            last_error_code: None,
            last_error_data: None,
            background_commands: None,
            rest: ["cwd", "projectId"]
                .into_iter()
                .filter_map(|k| Some((k.to_owned(), meta.get(k).filter(|v| !v.is_null())?.clone())))
                .collect(),
        };
        self.index_chat(&session_id, &meta);
        let acp = &mut self.chat.acp;
        acp.sessions.insert(session_id.clone(), session);
        acp.messages.insert(session_id.clone(), messages);
        acp.tool_calls.insert(session_id.clone(), tools);
        let watermark = match serde_json::from_value::<HistoryPayload>(payload) {
            Ok(history) => {
                self.acp_step(Event::NoteHistoryWatermark(session_id.clone(), history));
                self.chat.acp.book.history_watermarks.get(&session_id).copied().unwrap_or(0.0)
            }
            Err(e) => {
                log::warn!("[chat] history of {session_id} has no readable watermark: {e}");
                0.0
            }
        };
        self.chat.loaded.insert(session_id.clone());
        let cursor = Cursor { last_seq: watermark as u64, head_seq: watermark as u64, ..Cursor::default() };
        self.send(Command::Subscribe { session_id, cursor });
    }

    /// A session the model created holds its transcript already: follow its log from the
    /// start instead of reading its history.
    pub(crate) fn follow_session(&mut self, session_id: &str) {
        self.chat.requested.insert(session_id.to_owned());
        if self.chat.loaded.insert(session_id.to_owned()) {
            log::info!("[chat] following {session_id}");
            self.send(Command::Subscribe { session_id: session_id.to_owned(), cursor: Cursor::default() });
        }
    }

    /// The index row the model reads a chat's agent config from (`useCurrentAgentConfigId`'s
    /// index fallback), for a chat the app installed itself.
    fn index_chat(&mut self, session_id: &str, meta: &Value) {
        let index = &mut self.chat.acp.session_index;
        if index.iter().any(|e| e.get("id").and_then(Value::as_str) == Some(session_id)) {
            return;
        }
        let mut entry = serde_json::Map::new();
        entry.insert("id".into(), json!(session_id));
        let config = meta.get("agentConfigId").cloned().or_else(|| {
            let ns = meta.get("stableAgentNamespace").and_then(Value::as_str)?;
            ns.strip_prefix("config:").map(|id| json!(id))
        });
        for (key, value) in [("agentConfigId", config), ("projectId", meta.get("projectId").cloned()), ("cwd", meta.get("cwd").cloned())] {
            if let Some(value) = value.filter(|v| !v.is_null()) {
                entry.insert(key.into(), value);
            }
        }
        index.push(entry);
    }

    /// A chat the launcher just created: an empty transcript on `agent_id`, followed live.
    #[cfg(test)]
    pub(crate) fn install_new_chat(&mut self, session_id: &str, agent_id: &str) {
        self.install_chat(session_id, json!({ "agentId": agent_id, "turnActive": false }));
    }

    /// A launched chat: its folder, project and agent config come from the launch.
    pub(crate) fn install_launched_chat(&mut self, session_id: &str, agent_id: &str, launch: &crate::launch::Launch) {
        let meta = json!({
            "agentId": agent_id,
            "turnActive": false,
            "cwd": launch.cwd,
            "projectId": launch.project_id,
            "agentConfigId": launch.config_id,
        });
        self.install_chat(session_id, meta);
    }

    fn install_chat(&mut self, session_id: &str, meta: Value) {
        self.chat.requested.insert(session_id.to_owned());
        let payload = json!({ "messages": [], "toolCalls": [], "metadata": meta });
        self.chat_payload(session_id.to_owned(), Ok(Some(payload)));
    }

    /// One session event from the link.
    pub(crate) fn chat_event(&mut self, event: WsEvent) {
        let Some(session) = event.sid.as_deref() else { return };
        if !self.chat.loaded.contains(session) {
            return;
        }
        match to_event(&event) {
            Some(Ok(e)) => self.acp_step(e),
            Some(Err(e)) => log::warn!("[chat] unreadable {:?} event for {session}: {e}", event.kind),
            None => {}
        }
    }

    /// Applies one model event and runs its effects.
    pub fn acp_step(&mut self, event: Event) {
        let mut ids = Uuid;
        let mut env = Env { now: crate::ids::now_ms(), ids: &mut ids, server_history: true };
        for effect in acp::step(&mut self.chat.acp, event, &mut env) {
            match effect {
                Effect::Log { level, source, message } => match level {
                    model::Level::Warn | model::Level::Error => log::warn!("[{source}] {message}"),
                    _ => log::info!("[{source}] {message}"),
                },
                Effect::EndTurn(end) => self.chat.deferred.push(Event::TurnEnd(end)),
                Effect::PersistSession { .. } => {}
                Effect::DispatchPrompt { agent_id, session_id, turn_id, blocks, display_blocks } => {
                    self.dispatch_prompt(agent_id, session_id, turn_id, blocks, display_blocks)
                }
                Effect::CancelPrompt { agent_id, session_id } => self.command(
                    troupe_protocol::ws::WsRequestType::CancelPrompt,
                    json!({ "agentId": agent_id, "sessionId": session_id }),
                    Pending::Host("Could not stop the turn"),
                ),
                Effect::ActionRejected { message } if self.mcp_rejected(&message) => {}
                Effect::ActionRejected { message } => {
                    self.composer.rejected = true;
                    self.toast_error("Could not send your message", &message);
                }
                Effect::RetryQueueAfter { session_id, delay_ms, generation } => {
                    self.chat.retries.push((self.now + delay_ms / 1000.0, session_id, generation));
                }
                Effect::Answer(answer) => self.send_answer(answer),
                Effect::PersistComposerOptions { config_id, patch } => self.save_picks(config_id, patch),
                Effect::PersistModelCatalog { config_id, .. } => {
                    log::info!("[chat] model list of {config_id} not saved: the Rune client has no catalog store yet");
                }
                other => {
                    if let Some(other) = self
                        .run_control(other)
                        .and_then(|other| self.run_agent_command(other))
                        .and_then(|other| self.run_history_effect(other))
                        .and_then(|other| self.run_mcp_effect(other))
                    {
                        log::info!("[chat] effect not wired in the Rune client yet: {other:?}");
                    }
                }
            }
        }
        self.arm_when_turn_cleared();
    }

    /// The prompt command's reply: the receipt, a busy host, or a refusal.
    pub(crate) fn prompt_settled(&mut self, session_id: String, turn_id: String, result: Result<Option<Value>, WsError>) {
        let outcome = match result {
            Ok(reply) => DispatchOutcome::Answered(reply.unwrap_or(Value::Null)),
            Err(e) if is_turn_in_progress(&e) => DispatchOutcome::Busy,
            Err(e) => {
                log::warn!("[chat] the prompt for {session_id} was refused: {}: {}", e.code, e.message);
                let agent_dead = AGENT_DEAD.iter().any(|m| e.message.contains(m));
                DispatchOutcome::Failed { message: e.message, agent_dead }
            }
        };
        self.acp_step(Event::PromptSettled(PromptSettled { session_id, turn_id, outcome }));
    }

    /// Fires the busy-queue retries that came due; returns when the next one is due.
    pub(crate) fn fire_retries(&mut self, now: f64) -> (bool, Option<f64>) {
        let (due, waiting): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.chat.retries).into_iter().partition(|r| r.0 <= now);
        self.chat.retries = waiting;
        let fired = !due.is_empty();
        for (_, session_id, generation) in due {
            self.acp_step(Event::QueueRetryDue(session_id, generation));
        }
        (fired, self.chat.retries.iter().map(|r| r.0).reduce(f64::min))
    }

    /// Runs the turn ends the events of this pump asked for.
    pub(crate) fn flush_chat(&mut self) -> bool {
        let deferred = std::mem::take(&mut self.chat.deferred);
        let changed = !deferred.is_empty();
        for event in deferred {
            self.acp_step(event);
        }
        changed
    }
}
