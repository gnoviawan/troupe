//! A prompt whose reply was lost waits to be confirmed (AD-6). Port of
//! `prompt-reconcile.ts` and the detached half of `lib/command-engine.ts`.
//!
//! The app reports a dispatch whose outcome nobody knows as
//! [`DispatchOutcome::Uncertain`]: the socket dropped, timed out or halted
//! before the reply, or the host answered `outcome_unknown`. The turn stays
//! open and the command waits in `promptCommandStates`. It is resent with
//! the same turn id on a backoff ladder, on transport recovery, or when the
//! user asks to check, until the host answers, the turn's `user_prompt` echo
//! proves it was accepted, or its terminator ends it. An `outcome_unknown`
//! hold is resent only on the user's check. Stop abandons the command.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::prompt::{Dispatch, DispatchOutcome};
use super::types::SessionStatus;
use super::{Effect, State};

/// `COMMAND_RESEND_DELAYS_MS`; the last delay repeats.
const RESEND_DELAYS_MS: [f64; 4] = [2_000.0, 4_000.0, 8_000.0, 15_000.0];

const ENGINE: &str = "command-engine";
const KIND: &str = "send_prompt";

/// One session's prompt command waiting to be confirmed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pending {
    turn_id: String,
    dispatch: Dispatch,
    /// Timer-driven resends spent: picks the next delay, never a limit.
    ticks: usize,
    /// Sends made with this id, as the published state counts them.
    attempts: u64,
    /// `pending` while a resend is on the wire, else `pending-reconcile`.
    resending: bool,
    outcome_unknown: bool,
    /// Absent until a resend ran; true while the user's check is in flight.
    checking: Option<bool>,
    timer: Option<u64>,
}

/// A resend on the wire, by turn id.
#[derive(Clone, Debug, PartialEq)]
struct Resend {
    session_id: String,
    /// Stop abandoned the command meanwhile: once the resend settles, the
    /// turn it may have started is cancelled again on this agent.
    cancel_on: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ReconcileBook {
    pending: BTreeMap<String, Pending>,
    resends: BTreeMap<String, Resend>,
    timers: u64,
    /// Per session, this client's wire turn whose `user_prompt` echo arrived
    /// (`echoedWirePromptTurns`).
    pub(crate) echoed: BTreeMap<String, String>,
    /// A transport reconnect is in flight: a ladder tick waits for it.
    pub(crate) reconnecting: bool,
}

impl ReconcileBook {
    pub(crate) fn resending(&self, turn_id: &str) -> bool {
        self.resends.contains_key(turn_id)
    }
}

impl Pending {
    fn state(&self, session_id: &str) -> Value {
        let mut state = json!({
            "commandId": self.turn_id,
            "kind": KIND,
            "entityKey": session_id,
            "phase": if self.resending { "pending" } else { "pending-reconcile" },
            "outcomeUnknown": self.outcome_unknown,
            "attempts": self.attempts,
        });
        if let Some(checking) = self.checking {
            state["checking"] = json!(checking);
        }
        state
    }
}

impl State {
    fn publish_command_state(&mut self, session_id: &str) {
        match self.book.reconcile.pending.get(session_id) {
            Some(pending) => {
                let state = pending.state(session_id);
                self.prompt_command_states
                    .insert(session_id.to_owned(), state);
            }
            None => {
                self.prompt_command_states.remove(session_id);
            }
        }
    }

    /// The dispatch lost its reply. Nothing waits when the turn moved on
    /// meanwhile, or when its echo already proved the host accepted it.
    pub(crate) fn on_prompt_uncertain(
        &mut self,
        turn_id: String,
        dispatch: Dispatch,
        outcome_unknown: bool,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = dispatch.session_id.clone();
        let open = self
            .sessions
            .get(&session_id)
            .and_then(|session| session.open_turn_id.as_deref());
        if open != Some(dispatch.open_turn_id.as_str())
            || self.book.active_wire_turns.get(&session_id) != Some(&turn_id)
        {
            return;
        }
        if self.book.reconcile.echoed.get(&session_id) == Some(&turn_id) {
            return fx.push(Effect::info(
                "acp-store.promptReceipt",
                format!(
                    "Prompt turn {turn_id} on session {session_id} lost its reply after its user_prompt echo; accepted, awaiting its prompt_complete"
                ),
            ));
        }
        self.book.reconcile.pending.insert(
            session_id.clone(),
            Pending {
                turn_id: turn_id.clone(),
                dispatch,
                ticks: 0,
                attempts: 1,
                resending: false,
                outcome_unknown,
                checking: None,
                timer: None,
            },
        );
        self.publish_command_state(&session_id);
        fx.push(Effect::warn(
            ENGINE,
            format!(
                "{KIND} {turn_id} on {session_id} lost its reply; waiting to confirm and resending the same id"
            ),
        ));
        if !outcome_unknown {
            self.arm_resend_timer(&session_id, fx);
        }
    }

    fn arm_resend_timer(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        let book = &mut self.book.reconcile;
        let Some(pending) = book.pending.get_mut(session_id) else {
            return;
        };
        book.timers += 1;
        pending.timer = Some(book.timers);
        fx.push(Effect::PromptResendAfter {
            session_id: session_id.to_owned(),
            delay_ms: RESEND_DELAYS_MS[pending.ticks.min(RESEND_DELAYS_MS.len() - 1)],
            generation: book.timers,
        });
    }

    /// A ladder tick: resend, unless a reconnect is in flight, which resends
    /// on its own completion.
    pub(crate) fn on_prompt_resend_due(
        &mut self,
        session_id: &str,
        generation: u64,
        fx: &mut Vec<Effect>,
    ) {
        let book = &mut self.book.reconcile;
        let Some(pending) = book.pending.get_mut(session_id) else {
            return;
        };
        if pending.timer != Some(generation) {
            return;
        }
        pending.timer = None;
        if book.reconnecting {
            return self.arm_resend_timer(session_id, fx);
        }
        pending.ticks += 1;
        self.resend_prompt(session_id, false, fx);
    }

    /// `runResend`: send the same turn id again. An `outcome_unknown` hold
    /// goes out only on the user's check.
    fn resend_prompt(&mut self, session_id: &str, manual: bool, fx: &mut Vec<Effect>) {
        let Some(pending) = self.book.reconcile.pending.get(session_id) else {
            return;
        };
        if pending.resending || (pending.outcome_unknown && !manual) {
            return;
        }
        let gone = self.sessions.get(session_id).is_none_or(|session| {
            session.status == SessionStatus::Closed && !self.book.history.opening(session_id)
        });
        if gone {
            return self.clear_prompt_reconcile(session_id);
        }
        let agent_id = self.sessions[session_id].agent_id.clone();
        let pending = self
            .book
            .reconcile
            .pending
            .get_mut(session_id)
            .expect("found above");
        pending.resending = true;
        pending.timer = None;
        pending.attempts += 1;
        pending.checking = Some(manual);
        let turn_id = pending.turn_id.clone();
        let (blocks, display_blocks) = pending.dispatch.wire();
        self.book.reconcile.resends.insert(
            turn_id.clone(),
            Resend {
                session_id: session_id.to_owned(),
                cancel_on: None,
            },
        );
        if manual {
            self.publish_command_state(session_id);
        }
        fx.push(Effect::DispatchPrompt {
            agent_id,
            session_id: session_id.to_owned(),
            turn_id,
            blocks,
            display_blocks,
        });
    }

    /// A resend settled. One that outlived Stop cancels again; one whose
    /// command was dropped or replaced meanwhile does nothing.
    pub(crate) fn on_resend_settled(
        &mut self,
        turn_id: &str,
        outcome: DispatchOutcome,
        fx: &mut Vec<Effect>,
    ) {
        let Some(resend) = self.book.reconcile.resends.remove(turn_id) else {
            return;
        };
        let session_id = resend.session_id;
        if let Some(agent_id) = resend.cancel_on {
            return fx.push(Effect::CancelPrompt {
                agent_id,
                session_id,
            });
        }
        let Some(pending) = self
            .book
            .reconcile
            .pending
            .get_mut(&session_id)
            .filter(|pending| pending.turn_id == turn_id)
        else {
            return;
        };
        pending.resending = false;
        let (message, agent_dead) = match outcome {
            DispatchOutcome::Answered(result) => {
                self.clear_prompt_reconcile(&session_id);
                return super::prompt::log_receipt(turn_id, &session_id, &result, fx);
            }
            DispatchOutcome::Uncertain {
                outcome_unknown, ..
            } => {
                pending.outcome_unknown = outcome_unknown;
                pending.checking = Some(false);
                self.publish_command_state(&session_id);
                if !outcome_unknown {
                    self.arm_resend_timer(&session_id, fx);
                }
                return;
            }
            DispatchOutcome::Busy => (super::prompt::BUSY_ERROR.to_owned(), false),
            DispatchOutcome::Failed {
                message,
                agent_dead,
            } => (format!("Error: {message}"), agent_dead),
        };
        self.clear_prompt_reconcile(&session_id);
        fx.push(Effect::warn(
            ENGINE,
            format!("Resend of {KIND} {turn_id} on {session_id} was rejected: {message}"),
        ));
        self.fail_prompt_turn(&session_id, message, agent_dead);
    }

    /// `recheckPromptCommand`: the user asked whether the prompt went
    /// through; the same turn id goes out now, whatever its state.
    pub(crate) fn recheck_prompt_command(&mut self, session_id: &str, fx: &mut Vec<Effect>) {
        self.resend_prompt(session_id, true, fx);
    }

    /// `resendPendingPrompts`: the transport recovered.
    pub(crate) fn resend_pending_prompts(
        &mut self,
        session_id: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let sessions: Vec<String> = match session_id {
            Some(session_id) => vec![session_id],
            None => self.book.reconcile.pending.keys().cloned().collect(),
        };
        for session_id in sessions {
            self.resend_prompt(&session_id, false, fx);
        }
    }

    /// `clearPromptReconcile`: the command is over (answered, echoed,
    /// completed, failed or gone with its session).
    pub(crate) fn clear_prompt_reconcile(&mut self, session_id: &str) {
        self.book.reconcile.pending.remove(session_id);
        self.prompt_command_states.remove(session_id);
    }

    /// The turn's `user_prompt` echo: the host accepted the command.
    pub(crate) fn on_prompt_echo(&mut self, session_id: &str, turn_id: &str) {
        if self
            .book
            .active_wire_turns
            .get(session_id)
            .map(String::as_str)
            == Some(turn_id)
        {
            self.book
                .reconcile
                .echoed
                .insert(session_id.to_owned(), turn_id.to_owned());
        }
        if self
            .book
            .reconcile
            .pending
            .get(session_id)
            .is_some_and(|pending| pending.turn_id == turn_id)
        {
            self.clear_prompt_reconcile(session_id);
        }
    }

    /// `abandonPendingPrompt`: Stop on a prompt still waiting to be
    /// confirmed. No resend may send it again, and its turn id counts as
    /// ended, so a late terminator cannot end a newer turn. A resend already
    /// on the wire cancels again once it settles. False when nothing waited.
    pub(crate) fn abandon_pending_prompt(&mut self, session_id: &str, agent_id: &str) -> bool {
        let Some(pending) = self.book.reconcile.pending.remove(session_id) else {
            return false;
        };
        self.prompt_command_states.remove(session_id);
        let turn_id = pending.turn_id;
        if let Some(resend) = self.book.reconcile.resends.get_mut(&turn_id) {
            resend.cancel_on = Some(agent_id.to_owned());
        }
        self.claim_turn_end(&turn_id);
        if self.book.active_wire_turns.get(session_id) == Some(&turn_id) {
            self.book.active_wire_turns.remove(session_id);
        }
        if self.book.reconcile.echoed.get(session_id) == Some(&turn_id) {
            self.book.reconcile.echoed.remove(session_id);
        }
        true
    }

    /// The session's wire turn, its echo and any command waiting on it end.
    pub(crate) fn forget_prompt_turn(&mut self, session_id: &str) {
        self.clear_prompt_reconcile(session_id);
        self.book.active_wire_turns.remove(session_id);
        self.book.reconcile.echoed.remove(session_id);
    }
}
