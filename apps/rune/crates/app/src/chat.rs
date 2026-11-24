//! Chats in the workspace: a rail row opens its chat's tab, a visible chat tab loads the
//! chat's history from the host (`get_session_payload`) and follows its live log
//! (`subscribe`), and each session event the link delivers becomes a `model::acp` event.
//!
//! The model's history open (index, tail window, agent adoption) is a later slice of R6a. Until
//! it lands, this installs the whole payload and treats the chat as live, so a turn another
//! client sends streams in.

use std::collections::BTreeSet;

use client::cursor::Cursor;
use client::ws::Command;
use model::acp::{self, Effect, Env, Event, HistoryPayload};
use model::acp::types::{ChatMessage, Object, Session, SessionStatus};
use model::workspace::{Event as WorkspaceEvent, Tab};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsEvent, WsEventType};

use crate::State;
use crate::sync::{Pending, Uuid};

/// The chat side of the app state.
#[derive(Default)]
pub struct ChatUi {
    pub acp: acp::State,
    pub folds: ui::chat::Folds,
    /// Sessions whose payload was asked for (loaded or not).
    requested: BTreeSet<String>,
    /// Sessions whose payload is in.
    pub loaded: BTreeSet<String>,
    /// A chat to open once its project's workspace is showing.
    open_when_shown: Option<(String, String)>,
    /// The Markdown of the table the full-window view shows.
    pub zoomed_table: Option<String>,
    /// Turn ends the model asked for, run after the events already delivered.
    deferred: Vec<Event>,
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
        for session in tabs {
            if self.host.is_some() && self.chat.requested.insert(session.clone()) {
                self.call(
                    troupe_protocol::ws::WsRequestType::GetSessionPayload,
                    json!({ "sessionId": session }),
                    Pending::ChatPayload(session),
                );
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
            rest: Default::default(),
        };
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
                other => log::info!("[chat] effect not wired in the Rune client yet: {other:?}"),
            }
        }
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
