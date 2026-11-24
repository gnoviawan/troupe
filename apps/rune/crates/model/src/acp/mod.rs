//! The ACP chat model: sessions, their transcripts, plans, commands and
//! usage. Port of `src/renderer/stores/acp-store/`, one module per slice;
//! `docs/rune/acp-model.md` maps each TypeScript module to its Rust one and
//! lists what is not ported yet.
//!
//! [`step`] applies one event and returns the effects the app runs. The model
//! reads no clock and draws no randomness: [`Env`] carries both.

pub mod approvals;
mod clamp;
pub mod js;
#[cfg(test)]
mod js_tests;
mod plan;
pub mod subagent;
mod tool_call;
mod transcript;
pub mod types;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Ids, Level};
use approvals::{
    BrowserTicketClosedEvent, ElicitationRequestEvent, PendingElicitation, PendingPermission,
    PendingQuestion, PermissionRequestEvent, QuestionCancelledEvent, QuestionRequestEvent,
};
pub use transcript::{
    CommandsUpdateEvent, CompactionNotice, HistoryPayload, MessageChunkEvent, PayloadMetadata,
    Plan, PlanUpdateEvent, RetryNotice, SessionInfoUpdateEvent, ToolCallEvent, ToolCallUpdateEvent,
    UsageUpdateEvent, UserPromptEvent,
};
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
}

impl Book {
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
    /// A history payload was installed for the session.
    #[serde(rename = "noteHistoryWatermark")]
    NoteHistoryWatermark(String, HistoryPayload),
    /// The live-turn reattach lowered the session's subscribe cursor.
    #[serde(rename = "noteLiveTurnReplayFloor")]
    NoteLiveTurnReplayFloor(String, f64),
}

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
}

impl Effect {
    pub(crate) fn warn(source: &str, message: String) -> Self {
        Self::Log {
            level: Level::Warn,
            source: source.to_owned(),
            message,
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

/// Apply one event.
pub fn step(state: &mut State, event: Event, env: &mut Env<'_>) -> Vec<Effect> {
    let mut fx = Vec::new();
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
        Event::NoteHistoryWatermark(session_id, payload) => {
            state.note_history_watermark(&session_id, &payload);
        }
        Event::NoteLiveTurnReplayFloor(session_id, floor) => {
            state.book.live_turn_replay_floors.insert(session_id, floor);
        }
    }
    fx
}
