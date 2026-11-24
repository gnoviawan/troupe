//! Session events that build the transcript: the user's prompt echo, the
//! agent's streamed chunks, and the plan, command, title and usage updates.
//! Port of the handlers in `slices/transcript.ts`; tool calls are in
//! `tool_call.rs`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::types::{
    ChatMessage, Compaction, LogPoint, Object, OpencodeNotice, PlanEntry, Replaying, Role, Session,
    SessionStatus, Usage, UsageCost, nullable,
};
use super::{Effect, Env, State, clamp, js, subagent};

/// Id prefix of records spliced in from the session a chat switched away
/// from. They are history, never this session's live tail.
const SWITCH_SPLICE_ID_PREFIX: &str = "switch-splice:";
const READ_ONLY_GUARD_OPEN: &str = "<<<TERMUL_READ_ONLY_GUARD>>>";
const READ_ONLY_GUARD_CLOSE: &str = "<<<END_TERMUL_READ_ONLY_GUARD>>>";
const HANDOFF_PREFIX: &str = "# Conversation handoff\n\n";
const HANDOFF_DRAFT_SEP: &str = "\n\n---\n\n";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptEvent {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub content: Option<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageChunkEvent {
    pub session_id: String,
    pub role: Role,
    pub content: Value,
    #[serde(default)]
    pub message_id: Option<String>,
    /// The subagent dispatch that wrote this chunk.
    #[serde(default)]
    pub parent_tool_call_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEvent {
    pub session_id: String,
    pub tool_call: Object,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdateEvent {
    pub session_id: String,
    pub update: Object,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub entries: Option<Vec<PlanEntry>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanUpdateEvent {
    pub session_id: String,
    pub plan: Plan,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandsUpdateEvent {
    pub session_id: String,
    #[serde(default)]
    pub available_commands: Option<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionNotice {
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryNotice {
    pub attempt: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoUpdateEvent {
    pub session_id: String,
    /// Absent: no change. `null`: the agent cleared it.
    #[serde(default, with = "nullable")]
    pub title: Option<Option<String>>,
    #[serde(default)]
    pub compaction: Option<CompactionNotice>,
    /// Absent: no change. `null`: clear the retry line.
    #[serde(default, with = "nullable")]
    pub retry: Option<Option<RetryNotice>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageUpdateEvent {
    pub session_id: String,
    pub used: f64,
    pub size: f64,
    #[serde(default)]
    pub cost: Option<Value>,
    #[serde(default)]
    pub rate_limit: Option<Value>,
    #[serde(default)]
    pub limit_reached: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadMetadata {
    #[serde(default)]
    pub last_seq: Option<f64>,
}

/// The parts of an installed history payload the watermark reads.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPayload {
    pub metadata: PayloadMetadata,
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub tool_calls: Option<Value>,
}

impl HistoryPayload {
    /// The payload's restorable tool calls: objects with a non-empty id.
    fn tool_calls(&self) -> impl Iterator<Item = &Object> {
        self.tool_calls
            .as_ref()
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .filter(|call| {
                call.get("toolCallId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty())
            })
    }

    /// `maxPayloadSeq`, without the switch markers (ported with the switch).
    fn max_seq(&self) -> f64 {
        let messages = self
            .messages
            .iter()
            .filter_map(|m| js::finite(m.get("seq")));
        let calls = self.tool_calls().filter_map(|c| js::finite(c.get("seq")));
        messages.chain(calls).fold(0.0, f64::max)
    }

    /// `payloadSeqFloor`: above every timeline seq and subagent message seq.
    fn seq_floor(&self) -> f64 {
        let children = self
            .tool_calls()
            .filter_map(|call| call.get("subagentMessages")?.as_array())
            .flatten()
            .filter_map(|message| js::finite(message.get("seq")));
        children.fold(self.max_seq(), f64::max)
    }
}

/// `splitReadOnlyGuard`: the CAP-6 guard text off the front of a user turn,
/// and the blocks the user wrote.
fn split_read_only_guard(mut blocks: Vec<Value>) -> (Option<String>, Vec<Value>) {
    let Some(text) = blocks
        .first()
        .filter(|first| first.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|first| first.get("text"))
        .and_then(Value::as_str)
    else {
        return (None, blocks);
    };
    if !text.starts_with(READ_ONLY_GUARD_OPEN) {
        return (None, blocks);
    }
    let Some(end) = text.find(READ_ONLY_GUARD_CLOSE) else {
        return (None, blocks);
    };
    let cut = end + READ_ONLY_GUARD_CLOSE.len();
    let guard = text[..cut].to_owned();
    let rest = text[cut..]
        .trim_start_matches(|c: char| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
        .to_owned();
    if rest.is_empty() {
        blocks.remove(0);
    } else {
        blocks[0]["text"] = json!(rest);
    }
    (Some(guard), blocks)
}

/// `stripHandoffPreamble`: the draft after a handoff summary, `None` when
/// the text is the summary alone. Other text comes back unchanged.
fn strip_handoff_preamble(text: &str) -> Option<String> {
    if !text.starts_with(HANDOFF_PREFIX) {
        return Some(text.to_owned());
    }
    let sep = text.find(HANDOFF_DRAFT_SEP)?;
    let draft = crate::js_trim(&text[sep + HANDOFF_DRAFT_SEP.len()..]);
    (!draft.is_empty()).then(|| draft.to_owned())
}

fn is_text(block: &Value) -> bool {
    block.get("type").and_then(Value::as_str) == Some("text")
}

fn text_of(block: &Value) -> &str {
    block
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn is_splice(message: &ChatMessage) -> bool {
    message.id.starts_with(SWITCH_SPLICE_ID_PREFIX)
}

/// `appendBlocks`: a text chunk grows a trailing text block.
fn append_block(blocks: &mut Vec<Value>, incoming: Value) {
    if is_text(&incoming)
        && let Some(last) = blocks.last_mut().filter(|last| is_text(last))
    {
        let merged = format!("{}{}", text_of(last), text_of(&incoming));
        last["text"] = json!(merged);
        return;
    }
    blocks.push(incoming);
}

/// `toolIntervened`: a tool call landed after `message`.
fn tool_intervened(tools: &[Object], message: &ChatMessage) -> bool {
    let Some(seq) = message.seq else {
        return false;
    };
    tools.iter().any(|tool| {
        tool.get("seq")
            .and_then(Value::as_f64)
            .is_some_and(|t| t > seq)
    })
}

/// `hasActiveAssistantTail`: the list ends with a reply to the latest user
/// message, still open for late chunks.
fn has_active_assistant_tail(messages: &[ChatMessage], role: Role) -> bool {
    if role == Role::User {
        return false;
    }
    let Some(last) = messages.last().filter(|last| last.role == role) else {
        return false;
    };
    if last.streaming {
        return true;
    }
    messages
        .iter()
        .rposition(|message| message.role == Role::User)
        .is_some_and(|user| messages.len() - 1 > user)
}

/// `mayStartChunkMessage`.
fn may_start_chunk_message(session: &Session, role: Role) -> bool {
    session
        .open_turn_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
        || session.replaying().is_some()
        || role != Role::User
}

/// `nextOpencodeNotice`: apply one session-info marker. Absent fields keep
/// the previous notice.
fn next_notice(
    prev: Option<&OpencodeNotice>,
    event: &SessionInfoUpdateEvent,
) -> Option<OpencodeNotice> {
    let mut compaction = prev.and_then(|notice| notice.compaction);
    let mut retry_attempt = prev.and_then(|notice| notice.retry_attempt);
    let mut changed = false;
    if let Some(notice) = &event.compaction {
        changed = true;
        match notice.status.as_str() {
            "completed" => compaction = None,
            "started" => compaction = Some(Compaction::Started),
            "failed" => compaction = Some(Compaction::Failed),
            _ => {}
        }
    }
    match &event.retry {
        Some(None) => {
            changed = true;
            retry_attempt = None;
        }
        Some(Some(retry)) => {
            changed = true;
            retry_attempt = Some(retry.attempt);
        }
        None => {}
    }
    if !changed {
        return prev.cloned();
    }
    if compaction.is_none() && retry_attempt.is_none() {
        return None;
    }
    Some(OpencodeNotice {
        compaction,
        retry_attempt,
    })
}

impl State {
    pub(crate) fn on_user_prompt(
        &mut self,
        event: UserPromptEvent,
        event_seq: Option<f64>,
        env: &mut Env<'_>,
    ) {
        let session_id = event.session_id;
        if self.book.history_covered(&session_id, event_seq) {
            return;
        }
        let (guard, blocks) = split_read_only_guard(event.content.unwrap_or_default());
        let mut content: Vec<Value> = blocks.into_iter().map(clamp::content_block).collect();
        if let Some(first) = content
            .first()
            .filter(|first| is_text(first) && first.get("text").is_some_and(Value::is_string))
        {
            let text = text_of(first).to_owned();
            match strip_handoff_preamble(&text) {
                None => {
                    if content.len() <= 1 {
                        return;
                    }
                    content.remove(0);
                }
                Some(stripped) if stripped != text => content[0]["text"] = json!(stripped),
                Some(_) => {}
            }
        }
        let Some(session) = self.sessions.get(&session_id) else {
            return;
        };
        let list = self
            .messages
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let turn_id = event.turn_id.filter(|id| !id.is_empty());
        let turn_message_id = turn_id.as_ref().map(|id| format!("turn:{id}"));
        let trailing_user = list
            .iter()
            .rev()
            .find(|message| message.role == Role::User && !is_splice(message));
        let duplicate = turn_message_id
            .as_ref()
            .is_some_and(|id| list.iter().any(|message| message.id == *id))
            || trailing_user.is_some_and(|user| user.blocks == content);
        let mark_turn_active = !duplicate
            && !session.turn_busy()
            && session.replaying().is_none()
            && session.status != SessionStatus::Closed;
        let message = ChatMessage {
            id: turn_message_id.clone().unwrap_or_else(|| env.new_id("msg")),
            role: Role::User,
            blocks: content,
            streaming: false,
            timestamp: env.now,
            seq: Some(self.book.next_seq()),
            host_seq: event_seq,
            message_id: None,
            read_only_guard: guard,
            rest: Object::new(),
        };
        if duplicate {
            return;
        }
        self.messages
            .entry(session_id.clone())
            .or_default()
            .push(message);
        if mark_turn_active && let Some(session) = self.sessions.get_mut(&session_id) {
            session.active_turn = true;
            if turn_message_id.is_some() {
                session.open_turn_id = turn_message_id;
            }
        }
    }

    pub(crate) fn on_message_chunk(
        &mut self,
        event: MessageChunkEvent,
        event_seq: Option<f64>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        if event
            .parent_tool_call_id
            .as_deref()
            .is_some_and(|id| !id.is_empty())
        {
            self.on_subagent_chunk(event, event_seq, env, fx);
            return;
        }
        let session_id = event.session_id;
        if self.book.history_covered(&session_id, event_seq) {
            return;
        }
        let Some(session) = self.sessions.get(&session_id) else {
            return;
        };
        if session.status == SessionStatus::Closed && session.replaying().is_none() {
            return;
        }
        let role = event.role;
        let content = clamp::content_block(event.content);
        let server_replay_window = env.server_history && session.replaying().is_some();
        if session.replaying() == Some(Replaying::Pending) && !env.server_history {
            if is_text(&content) && crate::js_trim(text_of(&content)).is_empty() {
                return;
            }
            let message = ChatMessage {
                id: env.new_id("msg"),
                role,
                blocks: vec![content],
                streaming: true,
                timestamp: env.now,
                seq: Some(self.book.next_seq()),
                host_seq: None,
                message_id: event.message_id,
                read_only_guard: None,
                rest: Object::new(),
            };
            let messages = self.messages.entry(session_id.clone()).or_default();
            messages.retain(is_splice);
            messages.push(message);
            let calls = self.tool_calls.entry(session_id.clone()).or_default();
            calls.retain(|call| {
                call.get("toolCallId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.starts_with(SWITCH_SPLICE_ID_PREFIX))
            });
            if let Some(session) = self.sessions.get_mut(&session_id) {
                session.replaying = Some(Some(Replaying::Streaming));
            }
            return;
        }
        let list = self
            .messages
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let tools = self
            .tool_calls
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let last = list.last();
        let tagged = event.message_id.as_deref().is_some_and(|id| !id.is_empty());
        let last_id = last.and_then(|last| last.message_id.as_ref());
        let same_message_id = tagged && last_id == event.message_id.as_ref();
        let id_blocks_heuristic = tagged && last_id != event.message_id.as_ref();
        let mergeable = last
            .filter(|last| last.role == role && !is_splice(last) && !tool_intervened(tools, last));
        let heuristic_merge = !id_blocks_heuristic
            && mergeable.is_some_and(|last| {
                last.streaming || (!server_replay_window && has_active_assistant_tail(list, role))
            });
        if mergeable.is_some() && (same_message_id || heuristic_merge) {
            let last = self
                .messages
                .get_mut(&session_id)
                .and_then(|list| list.last_mut())
                .expect("the merge target exists");
            append_block(&mut last.blocks, content);
            last.streaming = true;
            if last.message_id.is_none() {
                last.message_id = event.message_id;
            }
            return;
        }
        if !may_start_chunk_message(session, role) {
            return;
        }
        if is_text(&content) && text_of(&content).is_empty() {
            return;
        }
        let message = ChatMessage {
            id: env.new_id("msg"),
            role,
            blocks: vec![content],
            streaming: true,
            timestamp: env.now,
            seq: Some(self.book.next_seq()),
            host_seq: event_seq,
            message_id: event.message_id,
            read_only_guard: None,
            rest: Object::new(),
        };
        self.messages.entry(session_id).or_default().push(message);
    }

    /// `appendSubagentChunk`: a subagent's text joins its dispatch call,
    /// never the main reply. It follows the tool-event coverage rule, since
    /// the open turn's attach replay rebuilds the dispatch card.
    fn on_subagent_chunk(
        &mut self,
        event: MessageChunkEvent,
        event_seq: Option<f64>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id;
        let parent = event.parent_tool_call_id.unwrap_or_default();
        if self.book.history_covered_tool(&session_id, event_seq) {
            return;
        }
        let replay_seq = self.book.live_turn_replay_seq(&session_id, event_seq);
        let Some(session) = self.sessions.get(&session_id) else {
            return;
        };
        if session.status == SessionStatus::Closed && session.replaying().is_none() {
            return;
        }
        let Some(text) = subagent::chunk_text(event.role == Role::Agent, &event.content) else {
            return;
        };
        let calls = self
            .tool_calls
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let Some(idx) = calls.iter().position(|call| {
            call.get("toolCallId").and_then(Value::as_str) == Some(parent.as_str())
        }) else {
            subagent::log_orphan(&mut self.book, fx, &parent, &session_id);
            return;
        };
        let seq = replay_seq.unwrap_or_else(|| self.book.next_seq());
        let mut call = calls[idx].clone();
        let message_id = event.message_id.map(Value::String);
        subagent::with_text(&mut call, text, (seq, env.now, message_id.as_ref()), calls);
        if let Some(calls) = self.tool_calls.get_mut(&session_id) {
            calls[idx] = call;
        }
    }

    fn accepts(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .is_some_and(Session::accepts_transcript_events)
    }

    pub(crate) fn on_plan_update(&mut self, event: PlanUpdateEvent) {
        let entries = event.plan.entries.unwrap_or_default();
        if entries.is_empty() {
            self.plans.remove(&event.session_id);
        } else if self.accepts(&event.session_id) {
            self.plans.insert(event.session_id, entries);
        }
    }

    /// `withCommands`: install the list unless a newer record of the same
    /// log incarnation set the one held.
    pub(crate) fn on_commands_update(
        &mut self,
        event: CommandsUpdateEvent,
        point: Option<LogPoint>,
    ) {
        let session_id = event.session_id;
        if !self.accepts(&session_id) {
            return;
        }
        if let (Some(point), Some(held)) = (&point, self.commands_seq.get(&session_id))
            && !point.is_after(held)
            && point.seq < held.seq
        {
            return;
        }
        self.commands.insert(
            session_id.clone(),
            event.available_commands.unwrap_or_default(),
        );
        match point {
            Some(point) => {
                self.commands_seq.insert(session_id, point);
            }
            None => {
                self.commands_seq.remove(&session_id);
            }
        }
    }

    pub(crate) fn on_session_info_update(
        &mut self,
        event: SessionInfoUpdateEvent,
        fx: &mut Vec<Effect>,
    ) {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return;
        };
        let next = next_notice(session.opencode_notice.as_ref(), &event);
        let notice_changed = session.opencode_notice != next;
        let Some(title) = event.title.clone() else {
            if notice_changed {
                session.opencode_notice = next;
            }
            return;
        };
        session.title = title;
        session.opencode_notice = next;
        let indexed = self.session_index.iter().any(|entry| {
            entry.get("id").and_then(Value::as_str) == Some(event.session_id.as_str())
        });
        if indexed {
            fx.push(Effect::PersistSession {
                session_id: event.session_id,
            });
        }
    }

    pub(crate) fn on_usage_update(&mut self, event: UsageUpdateEvent, env: &mut Env<'_>) {
        let reports_tokens = event.used.is_finite()
            && event.size.is_finite()
            && event.size > 0.0
            && event.used > 0.0;
        // `rateLimit ?? limitReached`, then JavaScript truthiness.
        let selected = [&event.rate_limit, &event.limit_reached]
            .into_iter()
            .find_map(|limit| limit.as_ref().filter(|limit| !limit.is_null()))
            .cloned();
        let reported_limit = selected.clone().filter(js::truthy);
        if !reports_tokens && reported_limit.is_none() {
            return;
        }
        if !self.accepts(&event.session_id) {
            return;
        }
        let prev = self.session_usage.get(&event.session_id);
        let next = match reported_limit {
            Some(limit) if !reports_tokens => {
                let base = prev.cloned().unwrap_or(Usage {
                    used: 0.0,
                    size: 0.0,
                    baseline_used: 0.0,
                    cost: None,
                    rate_limit: None,
                    updated_at: None,
                    source: "reported".into(),
                });
                Usage {
                    rate_limit: Some(limit),
                    updated_at: Some(env.now),
                    ..base
                }
            }
            _ => {
                let baseline_used = match prev {
                    Some(prev) if prev.used > 0.0 => prev.baseline_used,
                    _ => event.used,
                };
                let cost = event.cost.as_ref().and_then(|cost| {
                    let amount = js::finite(cost.get("amount")).filter(|amount| *amount > 0.0)?;
                    let currency = cost.get("currency")?.as_str().filter(|c| !c.is_empty())?;
                    Some(UsageCost {
                        amount,
                        currency: currency.to_owned(),
                    })
                });
                Usage {
                    used: event.used,
                    size: event.size,
                    baseline_used,
                    cost,
                    rate_limit: match selected {
                        Some(limit) => js::truthy(&limit).then_some(limit),
                        None => prev.and_then(|prev| prev.rate_limit.clone()),
                    },
                    updated_at: Some(env.now),
                    source: "reported".into(),
                }
            }
        };
        self.session_usage.insert(event.session_id, next);
    }

    /// `noteHistoryWatermark`: the installed payload covers every live event
    /// up to its last seq, and the local counter moves above it.
    pub(crate) fn note_history_watermark(&mut self, session_id: &str, payload: &HistoryPayload) {
        let watermark = payload
            .metadata
            .last_seq
            .unwrap_or(0.0)
            .max(payload.max_seq());
        if watermark > 0.0 {
            self.book
                .history_watermarks
                .insert(session_id.to_owned(), watermark);
        }
        self.book.live_turn_replay_floors.remove(session_id);
        self.book.rebase_seq(watermark.max(payload.seq_floor()));
    }
}
