//! Sending a prompt: the optimistic turn or the queue, the dispatch the app
//! runs through the client's command ledger, and what each outcome does.
//! Port of `runPromptTurn`, `sendPrompt`, `sendPromptBlocks`,
//! `sendQueuedPromptNow`, `cancelPrompt` and the queue actions in
//! `slices/prompt.ts`, and of `prompt-queue-orchestration.ts`.
//!
//! The TypeScript awaits the dispatch inside one async function. Here the
//! send is two steps: [`Effect::DispatchPrompt`] leaves the model, and the app
//! answers with [`Event::PromptSettled`](super::Event) when the command
//! settles. What the second step must remember sits in `Book::dispatches`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::types::{ChatMessage, Object, Role, SessionStatus};
use super::{Effect, Env, State};

/// `COMMAND_RESEND_DELAYS_MS`: the busy-retry ladder; the last delay repeats.
const RETRY_DELAYS_MS: [f64; 4] = [2_000.0, 4_000.0, 8_000.0, 15_000.0];

/// How the agent answered a dispatched prompt command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DispatchOutcome {
    /// A receipt (`accepted`, `in_progress`) or the stop reason the turn ran to.
    Answered(Value),
    /// The host already runs a turn on the session (`ACP_TURN_IN_PROGRESS`).
    Busy,
    /// A definite rejection with the error's message. `agent_dead` when the
    /// agent process went away, whose own crash event reports it.
    Failed { message: String, agent_dead: bool },
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSettled {
    pub session_id: String,
    pub turn_id: String,
    pub outcome: DispatchOutcome,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOptions {
    #[serde(default)]
    pub display_blocks: Option<Vec<Value>>,
}

/// Who started a send, which decides how a failure is reported.
#[derive(Clone, Debug, PartialEq)]
enum Caller {
    /// The composer: a failure rejects the action.
    User,
    /// The queue flush after a turn: a failure is logged.
    QueueFlush,
    /// Send-now of a queued item: a failure puts it back at the front.
    SendNow,
}

/// What the outcome of a dispatched prompt needs from when it was sent.
#[derive(Clone, Debug, PartialEq)]
pub struct Dispatch {
    caller: Caller,
    session_id: String,
    user_blocks: Vec<Value>,
    display_blocks: Option<Vec<Value>>,
    queued_origin: Option<Value>,
    previous_open_turn_id: Option<String>,
    open_turn_id: String,
    user_message_id: String,
}

/// A send-now waiting for the running turn to clear.
#[derive(Clone, Debug, PartialEq)]
pub struct SendNow {
    pub item: Value,
}

/// The busy-retry ladder of one session. `armed` names the timer running,
/// so a timer that fires after its retry was dropped does nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BusyRetry {
    attempts: usize,
    armed: Option<u64>,
}

struct Turn {
    caller: Caller,
    blocks: Vec<Value>,
    display: Option<Vec<Value>>,
    queued_origin: Option<Value>,
}

impl State {
    pub(crate) fn send_prompt(
        &mut self,
        session_id: &str,
        blocks: Vec<Value>,
        options: SendOptions,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let turn = Turn {
            caller: Caller::User,
            blocks,
            display: options.display_blocks,
            queued_origin: None,
        };
        self.run_prompt_turn(session_id, turn, env, fx);
    }

    /// The guard of a read-only GitHub chat, from the newest turn that
    /// carried it. The chat's link (`github-chat-links-store`) is not ported.
    fn read_only_guard(&self, session_id: &str) -> Option<String> {
        self.messages
            .get(session_id)?
            .iter()
            .rev()
            .find_map(|message| message.read_only_guard.clone())
    }

    /// `runPromptTurn`, up to the dispatch: queue the prompt behind a busy
    /// session, or open the turn with the user's bubble.
    fn run_prompt_turn(
        &mut self,
        session_id: &str,
        turn: Turn,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let reject = |fx: &mut Vec<Effect>, message: String| {
            if turn.caller == Caller::User {
                fx.push(Effect::rejected(message));
            }
        };
        let Some(session) = self.sessions.get(session_id) else {
            return reject(fx, format!("unknown session {session_id}"));
        };
        if session.status == SessionStatus::Closed {
            return reject(fx, "session is closed".into());
        }
        if turn.blocks.is_empty() {
            return reject(fx, "prompt content must not be empty".into());
        }
        let turn_id = env.ids.next_id();
        let busy = session.turn_busy();
        let awaiting_agent = session.agent_id.is_empty();
        let retry_armed = self
            .book
            .busy_retries
            .get(session_id)
            .is_some_and(|retry| retry.armed.is_some());
        let queued_before = self
            .prompt_queues
            .get(session_id)
            .is_some_and(|queue| !queue.is_empty());
        let hold = turn.queued_origin.is_none() && (queued_before || retry_armed);
        if busy || awaiting_agent || hold {
            let drain = !busy && !awaiting_agent && !retry_armed;
            let item = match turn.queued_origin {
                Some(origin) => Some((origin, true)),
                None => Some((queue_item(&turn.blocks, turn.display.as_ref(), env), false)),
            };
            if let Some((item, front)) = item {
                let queue = self.prompt_queues.entry(session_id.to_owned()).or_default();
                if front {
                    queue.insert(0, item);
                } else {
                    queue.push(item);
                }
            }
            if drain {
                self.flush_next_queued_prompt(session_id, env, fx);
            }
            return;
        }
        let open_turn_id = env.new_id("turn");
        let guard = self.read_only_guard(session_id);
        let message = ChatMessage {
            id: format!("turn:{turn_id}"),
            role: Role::User,
            blocks: turn.display.clone().unwrap_or_else(|| turn.blocks.clone()),
            streaming: false,
            timestamp: env.now,
            seq: Some(self.book.next_seq()),
            host_seq: None,
            message_id: None,
            read_only_guard: guard,
            rest: Object::from_iter([("sentHere".to_owned(), json!(true))]),
        };
        let user_message_id = message.id.clone();
        self.messages
            .entry(session_id.to_owned())
            .or_default()
            .push(message);
        let session = self.sessions.get_mut(session_id).expect("checked above");
        let previous_open_turn_id = session.open_turn_id.clone();
        session.active_turn = true;
        session.open_turn_id = Some(open_turn_id.clone());
        session
            .rest
            .insert("latestPromptTurnId".into(), json!(turn_id));
        session.last_error = None;
        session.last_error_code = Some(None);
        session.last_error_data = None;
        let agent_id = session.agent_id.clone();
        self.book
            .active_wire_turns
            .insert(session_id.to_owned(), turn_id.clone());
        fx.push(Effect::PersistSession {
            session_id: session_id.to_owned(),
        });
        let (blocks, display_blocks) = match self.read_only_guard(session_id) {
            Some(guard) => {
                let guard = json!({ "type": "text", "text": guard });
                let with_guard = |blocks: &[Value]| [std::slice::from_ref(&guard), blocks].concat();
                (
                    with_guard(&turn.blocks),
                    turn.display.as_deref().map(with_guard),
                )
            }
            None => (
                turn.blocks.clone(),
                // Only a send-now passes its display blocks to the host.
                turn.display
                    .clone()
                    .filter(|_| turn.caller == Caller::SendNow),
            ),
        };
        fx.push(Effect::DispatchPrompt {
            agent_id,
            session_id: session_id.to_owned(),
            turn_id: turn_id.clone(),
            blocks,
            display_blocks,
        });
        self.book.dispatches.insert(
            turn_id,
            Dispatch {
                caller: turn.caller,
                session_id: session_id.to_owned(),
                user_blocks: turn.blocks,
                display_blocks: turn.display,
                queued_origin: turn.queued_origin,
                previous_open_turn_id,
                open_turn_id,
                user_message_id,
            },
        );
    }

    /// The dispatched prompt command settled.
    pub(crate) fn on_prompt_settled(
        &mut self,
        settled: PromptSettled,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(dispatch) = self.book.dispatches.remove(&settled.turn_id) else {
            return;
        };
        let session_id = dispatch.session_id.clone();
        match settled.outcome {
            DispatchOutcome::Answered(result) => {
                let answer = match result.get("receipt").and_then(Value::as_str) {
                    Some(receipt @ ("accepted" | "in_progress")) => {
                        format!("with a {receipt} receipt")
                    }
                    _ => format!("with stop reason {}", super::js::string_of(Some(&result))),
                };
                fx.push(Effect::info(
                    "acp-store.promptReceipt",
                    format!(
                        "Prompt turn {} on session {session_id} answered {answer}; awaiting its prompt_complete",
                        settled.turn_id
                    ),
                ));
                self.reset_busy_retry(&session_id);
            }
            DispatchOutcome::Busy => {
                self.forget_wire_turn(&session_id);
                self.recover_to_queue(&dispatch, env);
                if !self
                    .sessions
                    .get(&session_id)
                    .is_some_and(|s| s.turn_busy())
                {
                    self.schedule_busy_retry(&session_id, fx);
                }
            }
            DispatchOutcome::Failed {
                message,
                agent_dead,
            } => {
                // `String(error)`, as the store keeps and shows it.
                let message = format!("Error: {message}");
                self.reset_busy_retry(&session_id);
                self.forget_wire_turn(&session_id);
                super::turn::finalize_streaming(self.messages.get_mut(&session_id));
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.active_turn = false;
                    session.open_turn_id = None;
                    if !agent_dead {
                        session.last_error = Some(message.clone());
                    }
                }
                match dispatch.caller {
                    Caller::User => fx.push(Effect::ActionRejected { message }),
                    Caller::SendNow => {
                        if let Some(origin) = dispatch.queued_origin.clone() {
                            self.prompt_queues
                                .entry(session_id.clone())
                                .or_default()
                                .insert(0, origin);
                        }
                        fx.push(Effect::ActionRejected { message });
                    }
                    Caller::QueueFlush if !agent_dead => {
                        fx.push(Effect::warn(
                            "acp-store.flushNextQueuedPrompt",
                            format!(
                                "Queued prompt on session {session_id} failed to send: {message}"
                            ),
                        ));
                        fx.push(Effect::Toast {
                            level: "error".into(),
                            message: format!("Failed to send queued message: {message}"),
                        });
                    }
                    Caller::QueueFlush => {}
                }
            }
        }
        if dispatch.caller == Caller::SendNow {
            self.suppress_queue_flush.remove(&session_id);
        }
    }

    fn forget_wire_turn(&mut self, session_id: &str) {
        self.book.active_wire_turns.remove(session_id);
    }

    /// `recoverPromptToQueue`: the host was busy, so the bubble comes off and
    /// the prompt waits at the end of the queue (a dequeued item at the front).
    fn recover_to_queue(&mut self, dispatch: &Dispatch, env: &mut Env<'_>) {
        let session_id = dispatch.session_id.as_str();
        let restored = self.sessions.get(session_id).map(|session| {
            if session.open_turn_id.as_deref() == Some(dispatch.open_turn_id.as_str()) {
                dispatch.previous_open_turn_id.clone()
            } else {
                session.open_turn_id.clone()
            }
        });
        let item = match &dispatch.queued_origin {
            Some(origin) => (origin.clone(), true),
            None => (
                queue_item(&dispatch.user_blocks, dispatch.display_blocks.as_ref(), env),
                false,
            ),
        };
        let queue = self.prompt_queues.entry(session_id.to_owned()).or_default();
        if item.1 {
            queue.insert(0, item.0);
        } else {
            queue.push(item.0);
        }
        self.messages
            .entry(session_id.to_owned())
            .or_default()
            .retain(|message| message.id != dispatch.user_message_id);
        if let (Some(session), Some(restored)) = (self.sessions.get_mut(session_id), restored) {
            session.active_turn = restored.is_some();
            session.open_turn_id = restored;
            session.last_error = None;
            session.last_error_code = Some(None);
            session.last_error_data = None;
        }
    }

    /// `scheduleBusyQueueRetry`: the host called the session busy while no
    /// turn runs here, so another client holds its gate; retry the queue.
    fn schedule_busy_retry(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        self.book.retry_generation += 1;
        let generation = self.book.retry_generation;
        let retry = self
            .book
            .busy_retries
            .entry(session_id.to_owned())
            .or_default();
        if retry.armed.is_some() {
            return;
        }
        let delay_ms = RETRY_DELAYS_MS[retry.attempts.min(RETRY_DELAYS_MS.len() - 1)];
        retry.attempts += 1;
        retry.armed = Some(generation);
        fx.push(Effect::RetryQueueAfter {
            session_id: session_id.to_owned(),
            delay_ms,
            generation,
        });
        fx.push(Effect::info(
            "acp-store.busyQueueRetry",
            format!(
                "Prompt on session {session_id} was rejected as busy with no turn running here; retrying the queue in {delay_ms}ms (attempt {})",
                retry.attempts
            ),
        ));
    }

    /// The busy-retry timer `generation` ran out.
    pub(crate) fn on_queue_retry_due(
        &mut self,
        session_id: &str,
        generation: u64,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(retry) = self.book.busy_retries.get_mut(session_id) else {
            return;
        };
        if retry.armed != Some(generation) {
            return;
        }
        retry.armed = None;
        if !self.sessions.contains_key(session_id) {
            self.book.busy_retries.remove(session_id);
            return;
        }
        self.flush_next_queued_prompt(session_id, env, fx);
    }

    /// `resetBusyQueueRetry`: an armed retry belongs to a later prompt.
    fn reset_busy_retry(&mut self, session_id: &str) {
        if !self
            .book
            .busy_retries
            .get(session_id)
            .is_some_and(|retry| retry.armed.is_some())
        {
            self.book.busy_retries.remove(session_id);
        }
    }

    /// `clearBusyQueueRetry`: a timer still running finds no retry and stops.
    pub(crate) fn clear_busy_retry(&mut self, session_id: &str) {
        self.book.busy_retries.remove(session_id);
    }

    /// `flushNextQueuedPrompt`: send the next queued prompt once the chat is
    /// idle.
    pub(crate) fn flush_next_queued_prompt(
        &mut self,
        session_id: &str,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        if self.suppress_queue_flush.contains_key(session_id) {
            return;
        }
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        if session.status == SessionStatus::Closed || session.turn_busy() {
            return;
        }
        let Some(queue) = self
            .prompt_queues
            .get_mut(session_id)
            .filter(|q| !q.is_empty())
        else {
            self.clear_busy_retry(session_id);
            return;
        };
        let next = queue.remove(0);
        let turn = Turn {
            caller: Caller::QueueFlush,
            blocks: item_blocks(&next, "blocks"),
            display: next.get("displayBlocks").and_then(Value::as_array).cloned(),
            queued_origin: Some(next),
        };
        self.run_prompt_turn(session_id, turn, env, fx);
    }

    pub(crate) fn remove_queued_prompt(&mut self, session_id: &str, queue_id: &Value) {
        let queue = self.prompt_queues.entry(session_id.to_owned()).or_default();
        queue.retain(|item| item.get("id") != Some(queue_id));
        if queue.is_empty() {
            self.clear_busy_retry(session_id);
        }
    }

    /// `reorderQueuedPrompt`: move one item to `to_index`, clamped.
    pub(crate) fn move_queued_prompt(
        &mut self,
        session_id: &str,
        queue_id: &Value,
        to_index: f64,
        fx: &mut Vec<Effect>,
    ) {
        let Some(queue) = self.prompt_queues.get_mut(session_id) else {
            return;
        };
        let Some(from) = queue
            .iter()
            .position(|item| item.get("id") == Some(queue_id))
        else {
            return;
        };
        let to = to_index.max(0.0).min((queue.len() - 1) as f64) as usize;
        if to == from {
            return;
        }
        let item = queue.remove(from);
        queue.insert(to, item);
        fx.push(Effect::info(
            "acp-store:prompt-queue",
            format!(
                "Chat {session_id} moved a queued prompt to place {}",
                crate::js_number(to_index + 1.0)
            ),
        ));
    }

    pub(crate) fn clear_queued_prompts(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        let count = self.prompt_queues.get(session_id).map_or(0, Vec::len);
        if count == 0 {
            return;
        }
        self.prompt_queues.insert(session_id.to_owned(), Vec::new());
        self.clear_busy_retry(session_id);
        fx.push(Effect::info(
            "acp-store:prompt-queue",
            format!(
                "Chat {session_id} cleared {count} queued prompt{}",
                if count == 1 { "" } else { "s" }
            ),
        ));
    }

    /// `sendQueuedPromptNow`: send one queued item ahead of the queue,
    /// stopping the running turn first.
    pub(crate) fn send_queued_prompt_now(
        &mut self,
        session_id: &str,
        queue_id: &Value,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let item = self
            .prompt_queues
            .get(session_id)
            .and_then(|queue| queue.iter().find(|item| item.get("id") == Some(queue_id)))
            .cloned();
        let Some(item) = item else {
            return fx.push(Effect::rejected("queued prompt not found"));
        };
        let Some(session) = self.sessions.get(session_id) else {
            return fx.push(Effect::rejected(format!("unknown session {session_id}")));
        };
        if session.status == SessionStatus::Closed {
            return fx.push(Effect::rejected("session is closed"));
        }
        let busy = session.turn_busy();
        let agent_id = session.agent_id.clone();
        if let Some(queue) = self.prompt_queues.get_mut(session_id) {
            queue.retain(|queued| queued.get("id") != Some(queue_id));
        }
        self.suppress_queue_flush
            .insert(session_id.to_owned(), json!(true));
        if busy {
            fx.push(Effect::CancelPrompt {
                agent_id,
                session_id: session_id.to_owned(),
            });
            self.book
                .send_now
                .insert(session_id.to_owned(), SendNow { item });
            return;
        }
        self.send_now(session_id, item, env, fx);
    }

    fn send_now(&mut self, session_id: &str, item: Value, env: &mut Env<'_>, fx: &mut Vec<Effect>) {
        let turn = Turn {
            caller: Caller::SendNow,
            blocks: item_blocks(&item, "blocks"),
            display: item.get("displayBlocks").and_then(Value::as_array).cloned(),
            queued_origin: Some(item),
        };
        let dispatches = self.book.dispatches.len();
        self.run_prompt_turn(session_id, turn, env, fx);
        if self.book.dispatches.len() == dispatches {
            // Queued again behind something else: nothing waits on an answer.
            self.suppress_queue_flush.remove(session_id);
        }
    }

    /// A send-now waiting on the turn it stopped goes out once that turn
    /// has cleared.
    pub(crate) fn resume_send_now(
        &mut self,
        session_id: &str,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        if self
            .sessions
            .get(session_id)
            .is_some_and(|session| session.turn_busy())
        {
            return;
        }
        if let Some(pending) = self.book.send_now.remove(session_id) {
            self.send_now(session_id, pending.item, env, fx);
        }
    }

    /// `cancelPrompt`: stop the turn, or the agent's own work outside one.
    pub(crate) fn cancel_prompt(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        if session.active_turn || session.agent_working == Some(true) {
            fx.push(Effect::CancelPrompt {
                agent_id: session.agent_id.clone(),
                session_id: session_id.to_owned(),
            });
        }
    }
}

fn item_blocks(item: &Value, key: &str) -> Vec<Value> {
    item.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// `appendQueuedPrompt`'s item.
fn queue_item(blocks: &[Value], display: Option<&Vec<Value>>, env: &mut Env<'_>) -> Value {
    let mut item = json!({ "id": env.new_id("queue"), "blocks": blocks, "createdAt": env.now });
    if let Some(display) = display {
        item["displayBlocks"] = json!(display);
    }
    item
}
