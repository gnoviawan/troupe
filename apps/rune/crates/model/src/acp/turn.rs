//! The end of a prompt turn and the agent's work outside one: the host's
//! `prompt_complete`, the deferred close it schedules, `agent_working` and
//! `background_commands`. Port of those handlers in `slices/prompt.ts`, of
//! the turn-end helpers in `helpers.ts`, and of `host-session-reports.ts`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::transcript::{split_read_only_guard, strip_handoff_preamble};
use super::types::{ChatMessage, PlanEntry, Role};
use super::{Effect, Env, State, js};

/// `ENDED_PROMPT_TURN_LIMIT`.
const ENDED_PROMPT_TURNS: usize = 256;
const SWITCH_SPLICE_ID_PREFIX: &str = "switch-splice:";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCompleteEvent {
    pub agent_id: String,
    pub session_id: String,
    pub stop_reason: String,
    #[serde(default)]
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkingEvent {
    pub agent_id: String,
    pub session_id: String,
    pub working: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCommandsEvent {
    pub agent_id: String,
    pub session_id: String,
    pub revision: f64,
    pub commands: Value,
}

/// The close a `prompt_complete` schedules once the events already queued
/// have landed, so chunks racing the terminator still join the turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEnd {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// The turn to close; `None` closes an `activeTurn`-only session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

/// The newest `agent_working` and `background_commands` report of one agent
/// in one session, each with the report counter when it arrived.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostReports {
    pub working: Option<(bool, u64)>,
    pub background_commands: Option<(Value, u64)>,
}

/// `noteForStopReason`: the line a stop reason other than a normal end shows.
pub(crate) fn note_for_stop_reason(reason: &str) -> Option<String> {
    Some(match reason {
        "refusal" => "The agent refused to continue.".into(),
        "max_tokens" => "Response stopped: token limit reached.".into(),
        "max_turn_requests" => "Response stopped: too many tool-call rounds.".into(),
        "end_turn" | "cancelled" => return None,
        "interrupted" => "Interrupted by server restart.".into(),
        "error" => "The turn ended with an error.".into(),
        "timeout" => "The turn timed out.".into(),
        "crashed" => "The agent stopped unexpectedly.".into(),
        other => format!("Response stopped: {other}"),
    })
}

/// `settleRunningPlan`: nothing runs once the turn is over.
pub(crate) fn settle_running_plan(entries: &[PlanEntry]) -> Option<Vec<PlanEntry>> {
    let running = |entry: &PlanEntry| entry.text("status") == Some("in_progress");
    if !entries.iter().any(running) {
        return None;
    }
    Some(
        entries
            .iter()
            .map(|entry| {
                let mut entry = entry.clone();
                if running(&entry) {
                    entry.0.insert("status".into(), json!("pending"));
                }
                entry
            })
            .collect(),
    )
}

/// `isTermulPlanFenceBlock`: the whole text is one `termul-plan` fence.
fn is_plan_fence(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("```termul-plan") else {
        return false;
    };
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .is_some_and(|rest| rest.ends_with("\n```"))
}

/// `appendPlanSnapshot`: put the plan, as a `termul-plan` fence, on the
/// turn's last agent message, so a reopened chat restores the panel.
fn append_plan_snapshot(messages: &mut [ChatMessage], plan: &[PlanEntry]) {
    if plan.is_empty() {
        return;
    }
    let Some(target) = messages.iter_mut().rev().find(|message| {
        message.role == Role::Agent && !message.id.starts_with(SWITCH_SPLICE_ID_PREFIX)
    }) else {
        return;
    };
    let is_text = |block: &Value| block.get("type").and_then(Value::as_str) == Some("text");
    let text_of = |block: &Value| {
        block
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    target
        .blocks
        .retain(|block| !is_text(block) || !is_plan_fence(&text_of(block)));
    if let Some(last) = target
        .blocks
        .iter_mut()
        .rev()
        .find(|block| is_text(block) && !text_of(block).is_empty())
    {
        let text = text_of(last);
        if !text.ends_with('\n') {
            last["text"] = json!(format!("{text}\n"));
        }
    }
    let plan = serde_json::to_string(plan).expect("a plan serializes");
    target
        .blocks
        .push(json!({ "type": "text", "text": format!("```termul-plan\n{plan}\n```") }));
}

/// `normalizeUserMessageBlocks` without the composer's wire reverse (ported
/// with the composer): the read-only guard and a handoff preamble come off.
pub(crate) fn normalize_user_message(message: &mut ChatMessage) {
    if message.role != Role::User {
        return;
    }
    let (guard, blocks) = split_read_only_guard(std::mem::take(&mut message.blocks));
    message.blocks = blocks;
    if guard.is_some() {
        message.read_only_guard = guard;
    }
    let boundary = message.rest.get("handoffBoundary") == Some(&Value::Bool(true));
    let Some(first) = message
        .blocks
        .first()
        .filter(|first| first.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|first| first.get("text"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return;
    };
    match strip_handoff_preamble(&first) {
        None if message.blocks.len() == 1 && boundary => {}
        None => {
            message.blocks.remove(0);
            let boundary = message.blocks.is_empty();
            message
                .rest
                .insert("handoffBoundary".into(), json!(boundary));
        }
        Some(stripped) if stripped != first => {
            message.blocks[0]["text"] = json!(stripped);
            message
                .rest
                .insert("handoffBoundary".into(), json!(boundary));
        }
        Some(_) => {}
    }
}

/// `finalizeStreaming`: every streaming message of the session stops.
pub(crate) fn finalize_streaming(messages: Option<&mut Vec<ChatMessage>>) {
    for message in messages.into_iter().flatten().filter(|m| m.streaming) {
        message.streaming = false;
        normalize_user_message(message);
    }
}

impl State {
    /// `claimPromptTurnEnd`: true the first time a turn id ends.
    pub(crate) fn claim_turn_end(&mut self, turn_id: &str) -> bool {
        let ended = &mut self.book.ended_prompt_turns;
        if ended.iter().any(|id| id == turn_id) {
            return false;
        }
        ended.push_back(turn_id.to_owned());
        if ended.len() > ENDED_PROMPT_TURNS {
            ended.pop_front();
        }
        true
    }

    /// `lastErrorAfterStop`: a failed turn keeps the specific error an
    /// `agent_error` recorded; other stops show their note.
    fn last_error_after_stop(&mut self, session_id: &str, reason: &str) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        let note = note_for_stop_reason(reason);
        let failed = matches!(reason, "error" | "timeout" | "crashed");
        let keep = if failed {
            session.last_error != Some(None)
        } else {
            note.is_none()
        };
        if !keep {
            session.last_error = Some(note);
            session.last_error_code = Some(None);
            session.last_error_data = None;
        }
    }

    pub(crate) fn indexed(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
            && self
                .session_index
                .iter()
                .any(|entry| entry.get("id").and_then(Value::as_str) == Some(session_id))
    }

    pub(crate) fn on_prompt_complete(
        &mut self,
        event: PromptCompleteEvent,
        event_seq: Option<f64>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id.as_str();
        if self.book.history_covered(session_id, event_seq) {
            return;
        }
        if let Some(turn_id) = event.turn_id.as_deref().filter(|id| !id.is_empty()) {
            // A terminator for another turn than the one this client opened
            // is a stale replay of an earlier turn.
            if self
                .book
                .active_wire_turns
                .get(session_id)
                .is_some_and(|open| open != turn_id)
            {
                return;
            }
            if !self.claim_turn_end(turn_id) {
                self.flush_next_queued_prompt(session_id, env, fx);
                return;
            }
        }
        if self.settle_collector(session_id, Ok(event.stop_reason.clone())) {
            return;
        }
        let live_plan = self.plans.get(session_id);
        let settled = live_plan.and_then(|plan| settle_running_plan(plan));
        if let (Some(live), Some(settled)) = (live_plan, settled) {
            let stopped = live
                .iter()
                .filter(|entry| entry.text("status") == Some("in_progress"))
                .count();
            fx.push(Effect::info(
                "planSession",
                format!(
                    "Turn ended ({}) for session {session_id}; {stopped} in-progress plan item(s) returned to pending",
                    event.stop_reason
                ),
            ));
            self.plans.insert(session_id.to_owned(), settled);
        }
        let plan = self.plans.get(session_id).cloned().unwrap_or_default();
        if let Some(messages) = self.messages.get_mut(session_id) {
            append_plan_snapshot(messages, &plan);
        }
        finalize_streaming(self.messages.get_mut(session_id));
        let raised_here = |prompt_session: &str, prompt_agent: &str| {
            prompt_session == session_id && prompt_agent == event.agent_id
        };
        self.pending_permissions
            .retain(|_, p| !raised_here(&p.session_id, &p.agent_id));
        self.pending_questions
            .retain(|_, q| !raised_here(&q.session_id, &q.agent_id));
        self.pending_elicitations
            .retain(|_, e| !raised_here(&e.session_id, &e.agent_id));
        if self.sessions.contains_key(session_id) {
            if event.stop_reason == "end_turn"
                && let Some(usage) = self.session_usage.get_mut(session_id)
                && usage
                    .rate_limit
                    .as_ref()
                    .and_then(|limit| limit.get("status"))
                    == Some(&json!("rejected"))
            {
                usage.rate_limit = None;
            }
            if event.stop_reason == "timeout" {
                self.agent_status
                    .insert(event.agent_id.clone(), json!("error"));
            }
            self.last_error_after_stop(session_id, &event.stop_reason);
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.opencode_notice = None;
            }
        }
        if self.indexed(session_id) {
            fx.push(Effect::PersistSession {
                session_id: session_id.to_owned(),
            });
        }
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let turn_id = session.open_turn_id.clone().filter(|id| !id.is_empty());
        if turn_id.is_none() && !session.active_turn {
            return;
        }
        fx.push(Effect::EndTurn(TurnEnd {
            session_id: session_id.to_owned(),
            stop_reason: Some(event.stop_reason),
            turn_id,
        }));
    }

    /// The deferred close `scheduleTurnEnd` runs: only the turn it was
    /// scheduled for, or an `activeTurn`-only session when it named none.
    pub(crate) fn on_turn_end(&mut self, end: TurnEnd, env: &mut Env<'_>, fx: &mut Vec<Effect>) {
        let session_id = end.session_id.as_str();
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let open = session.open_turn_id.as_deref().filter(|id| !id.is_empty());
        let closes = match end.turn_id.as_deref() {
            Some(turn_id) => open == Some(turn_id),
            None => session.active_turn && open.is_none(),
        };
        if !closes {
            return;
        }
        finalize_streaming(self.messages.get_mut(session_id));
        let seq = self
            .turn_end_notices
            .get(session_id)
            .and_then(|notice| notice.get("seq"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        self.turn_end_notices.insert(
            session_id.to_owned(),
            json!({ "seq": seq + 1.0, "stopReason": end.stop_reason }),
        );
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.open_turn_id = None;
            session.active_turn = false;
        }
        if let Some(reason) = &end.stop_reason {
            self.last_error_after_stop(session_id, reason);
        }
        self.forget_prompt_turn(session_id);
        if self.indexed(session_id) {
            fx.push(Effect::PersistSession {
                session_id: session_id.to_owned(),
            });
        }
        self.flush_next_queued_prompt(session_id, env, fx);
        self.resume_send_now(session_id, env, fx);
    }

    pub(crate) fn on_agent_working(
        &mut self,
        event: AgentWorkingEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let report = self.book.next_report();
        self.book
            .host_reports
            .entry((event.session_id.clone(), event.agent_id.clone()))
            .or_default()
            .working = Some((event.working, report));
        let Some(session) = self
            .sessions
            .get_mut(&event.session_id)
            .filter(|session| session.agent_id == event.agent_id)
        else {
            return;
        };
        if session.agent_working.unwrap_or(false) == event.working {
            return;
        }
        session.agent_working = Some(event.working);
        fx.push(Effect::info(
            "acp-store.agentWorking",
            format!(
                "Agent {} working on session {} outside a prompt turn",
                if event.working { "started" } else { "stopped" },
                event.session_id
            ),
        ));
        if !event.working {
            self.flush_next_queued_prompt(&event.session_id, env, fx);
        }
    }

    pub(crate) fn on_background_commands(&mut self, event: BackgroundCommandsEvent) {
        let key = (event.session_id.clone(), event.agent_id.clone());
        let held_revision = self
            .book
            .host_reports
            .get(&key)
            .and_then(|reports| reports.background_commands.as_ref())
            .and_then(|(list, _)| js::finite(list.get("revision")));
        if held_revision.is_some_and(|held| event.revision <= held) {
            return;
        }
        let list = json!({ "revision": event.revision, "commands": event.commands });
        let report = self.book.next_report();
        self.book
            .host_reports
            .entry(key)
            .or_default()
            .background_commands = Some((list, report));
        let Some(session) = self
            .sessions
            .get_mut(&event.session_id)
            .filter(|session| session.agent_id == event.agent_id)
        else {
            return;
        };
        let next = json!({
            "agentId": event.agent_id,
            "revision": event.revision,
            "commands": event.commands,
        });
        // `newerBackgroundCommands`: another agent's list is stale; the same
        // agent's list wins only with a newer revision.
        let newer = match &session.background_commands {
            None => true,
            Some(held) => {
                held.get("agentId") != next.get("agentId")
                    || js::finite(held.get("revision")).is_some_and(|held| event.revision > held)
            }
        };
        if newer {
            session.background_commands = Some(next);
        }
    }
}
