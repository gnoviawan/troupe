//! Host-initiated parent wake for settled sub-agents: when an orchestrated
//! sub-agent settles, Troupe sends its root chat a prompt carrying the result,
//! starting a new turn if the chat is idle or queuing it until the active turn
//! finishes.
//!
//! Uses only the universal ACP primitive — a new prompt turn via
//! [`crate::web::prompt_turn::accept_prompt_turn`] after the active turn ends
//! (no mid-turn steering). Every root chat has one typed mailbox under
//! [`WakeRegistry`] that keeps pending notices, in-flight delivery state,
//! dedupe/acknowledgement sets, and the consecutive autonomous-wake streak.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, OnceLock, Weak};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use parking_lot::Mutex;

use crate::acp::config::SessionId;
use crate::acp::events;
use crate::acp::orchestrator::engine::OrchestratorEngine;
use crate::acp::orchestrator::run::{RunId, RunState};
use crate::acp::subagents::SubagentWorkspace;
use crate::web::prompt_turn::{
    accept_prompt_turn, complete_prompt_turn, PromptAdmission, PromptTurnError, PromptTurnRequest,
};
use crate::web::sink::{AcpEvent, EventSink};

/// Prefix of every host-initiated sub-agent wake turn id.
pub(crate) const WAKE_TURN_PREFIX: &str = "subagent-wake-";

/// Maximum consecutive autonomous wake turns with no user prompt in between.
pub(crate) const MAX_WAKE_STREAK: u32 = 30;

/// Maximum UTF-8 bytes of one run's result text inlined in a wake prompt.
pub(crate) const MAX_INLINE_RESULT_BYTES: usize = 16_000;

/// First delay of the retry that re-attempts a wake the root chat could not
/// take (no live agent owns it, or the prompt was refused for a reason other
/// than a running turn); it doubles up to [`MAX_RETRY_DELAY`].
const FIRST_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(60);
/// Retries before a chat waits for its next prompt to flush (about 40 min).
const MAX_RETRIES: u32 = 40;

/// Opening instruction line of every wake prompt.
pub(crate) const WAKE_PROMPT_HEADER: &str = "[Troupe] Sub-agent update. You started these \
sub-agents; act on their results, then end your turn when nothing is left.";

/// One settled run waiting to be reported to its root chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WakeNotice {
    pub(crate) run_id: RunId,
    pub(crate) title: String,
    pub(crate) agent_name: String,
    pub(crate) state: RunState,
    pub(crate) result: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) workspace: Option<SubagentWorkspace>,
}

/// Exclusive delivery state of one root chat's mailbox. Making `Idle`,
/// `Starting`, and `InFlight` enum variants prevents concurrent delivery tasks
/// from overlapping and ties an in-flight turn id strictly to `InFlight`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DeliveryState {
    /// No wake delivery is running or in flight.
    Idle,
    /// A delivery task claimed the mailbox and is probing root chat readiness
    /// before taking the pending batch. `recheck` records a flush trigger that
    /// arrived during the async probe so it is never lost.
    Starting { recheck: bool },
    /// A wake prompt with `turn_id` is being submitted or running on the root
    /// chat. Cleared when its `acp:prompt_complete` arrives (or if submission
    /// fails). `recheck` records another turn completing while submission raced
    /// a `Busy` error.
    InFlight { turn_id: String, recheck: bool },
}

/// Per-root-chat wake mailbox.
struct RootMailbox {
    /// Settled notices in arrival order.
    pending: VecDeque<WakeNotice>,
    /// Runs that have already produced a wake notice for this chat (at most
    /// one notice per settle).
    enqueued: HashSet<RunId>,
    /// Runs whose terminal state the parent already read via `subagent_wait`
    /// or `subagent_status` (suppresses pending or late-arriving notices).
    acknowledged: HashSet<RunId>,
    delivery: DeliveryState,
    /// Consecutive autonomous wake turns with no user prompt in between.
    streak: u32,
    /// Incremented on every non-wake prompt reset so a failed delivery never
    /// decrements a freshly-reset streak.
    streak_epoch: u64,
    /// Ensures the loop-guard warning is logged once per streak saturation.
    limit_logged: bool,
    /// A retry timer is pending for this chat.
    retry_armed: bool,
    /// Retries since the last accepted prompt on this chat.
    retries: u32,
}

impl RootMailbox {
    fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            enqueued: HashSet::new(),
            acknowledged: HashSet::new(),
            delivery: DeliveryState::Idle,
            streak: 0,
            streak_epoch: 0,
            limit_logged: false,
            retry_armed: false,
            retries: 0,
        }
    }
}

/// Notices claimed for one wake turn, with the streak metadata needed for
/// logging and rollback if submission fails.
pub(crate) struct TakenBatch {
    pub(crate) notices: Vec<WakeNotice>,
    pub(crate) streak: u32,
    pub(crate) streak_epoch: u64,
}

/// Thread-safe registry of per-root-chat wake mailboxes.
#[derive(Default)]
pub(crate) struct WakeRegistry {
    mailboxes: Mutex<HashMap<String, RootMailbox>>,
}

impl WakeRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Enqueue a notice for `root` unless this run already produced one or was
    /// already acknowledged via `subagent_wait`/`subagent_status`. Returns
    /// `true` when the notice was queued.
    pub(crate) fn enqueue(&self, root: &str, notice: WakeNotice) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let mailbox = mailboxes
            .entry(root.to_string())
            .or_insert_with(RootMailbox::new);
        if mailbox.acknowledged.contains(&notice.run_id)
            || !mailbox.enqueued.insert(notice.run_id.clone())
        {
            return false;
        }
        mailbox.pending.push_back(notice);
        if let DeliveryState::Starting { recheck } | DeliveryState::InFlight { recheck, .. } =
            &mut mailbox.delivery
        {
            *recheck = true;
        }
        true
    }

    /// A `subagent_send` resumed `run`: its next settle is a new result, so
    /// it may queue a notice again even though an earlier one went out or
    /// was read.
    pub(crate) fn reopen_run(&self, root: &str, run: &RunId) {
        if let Some(mailbox) = self.mailboxes.lock().get_mut(root) {
            mailbox.enqueued.remove(run);
            mailbox.acknowledged.remove(run);
        }
    }

    /// Record that the parent already observed `run`'s terminal state, dropping
    /// any pending notice and suppressing a late enqueue from an in-flight
    /// `finish`.
    pub(crate) fn acknowledge(&self, root: &str, run: &RunId) {
        let mut mailboxes = self.mailboxes.lock();
        let mailbox = mailboxes
            .entry(root.to_string())
            .or_insert_with(RootMailbox::new);
        mailbox.acknowledged.insert(run.clone());
        mailbox.pending.retain(|notice| &notice.run_id != run);
    }

    /// Claim the `Idle -> Starting` transition for `root` if notices are
    /// pending and the consecutive-wake guard allows another wake.
    pub(crate) fn begin_delivery(&self, root: &str) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return false;
        };
        if mailbox.pending.is_empty() || !matches!(mailbox.delivery, DeliveryState::Idle) {
            return false;
        }
        if mailbox.streak >= MAX_WAKE_STREAK {
            if !mailbox.limit_logged {
                mailbox.limit_logged = true;
                log::warn!(
                    "[orchestrator] chat {} reached the {MAX_WAKE_STREAK}-wake streak limit; \
                     holding pending sub-agent updates until the user sends a prompt",
                    crate::logging::redact_session_id(root)
                );
            }
            return false;
        }
        mailbox.delivery = DeliveryState::Starting { recheck: false };
        true
    }

    /// Release `Starting -> Idle` when the root chat is not ready yet (no live
    /// owner or active turn). Returns `true` when a
    /// flush trigger arrived during the readiness probe and notices remain.
    pub(crate) fn abort_starting(&self, root: &str) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return false;
        };
        match mailbox.delivery {
            DeliveryState::Starting { recheck } => {
                mailbox.delivery = DeliveryState::Idle;
                recheck && !mailbox.pending.is_empty()
            }
            DeliveryState::Idle | DeliveryState::InFlight { .. } => false,
        }
    }

    /// Transition `Starting -> InFlight { turn_id }` and drain ALL currently
    /// pending notices into one batch. Returns `None` (returning to `Idle`) if
    /// the mailbox was closed, emptied, or hit the wake cap during the probe.
    pub(crate) fn take_for_delivery(&self, root: &str, turn_id: String) -> Option<TakenBatch> {
        let mut mailboxes = self.mailboxes.lock();
        let mailbox = mailboxes.get_mut(root)?;
        if !matches!(mailbox.delivery, DeliveryState::Starting { .. }) {
            return None;
        }
        if mailbox.pending.is_empty() || mailbox.streak >= MAX_WAKE_STREAK {
            if mailbox.streak >= MAX_WAKE_STREAK
                && !mailbox.pending.is_empty()
                && !mailbox.limit_logged
            {
                mailbox.limit_logged = true;
                log::warn!(
                    "[orchestrator] chat {} reached the {MAX_WAKE_STREAK}-wake streak limit; \
                     holding pending sub-agent updates until the user sends a prompt",
                    crate::logging::redact_session_id(root)
                );
            }
            mailbox.delivery = DeliveryState::Idle;
            return None;
        }
        let notices: Vec<WakeNotice> = mailbox.pending.drain(..).collect();
        mailbox.streak += 1;
        let streak = mailbox.streak;
        let streak_epoch = mailbox.streak_epoch;
        mailbox.delivery = DeliveryState::InFlight {
            turn_id,
            recheck: false,
        };
        Some(TakenBatch {
            notices,
            streak,
            streak_epoch,
        })
    }

    /// Put `notices` back at the front of `root`'s pending queue (skipping any
    /// acknowledged in the meantime), release `InFlight { turn_id } -> Idle`,
    /// and undo the streak bump if no user prompt reset it meanwhile. Returns
    /// whether a flush trigger arrived during the failed attempt.
    pub(crate) fn restore_failed_delivery(
        &self,
        root: &str,
        turn_id: &str,
        notices: Vec<WakeNotice>,
        streak_epoch: u64,
    ) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return false;
        };
        let recheck = match &mailbox.delivery {
            DeliveryState::InFlight {
                turn_id: active,
                recheck,
            } if active == turn_id => *recheck,
            _ => return false,
        };
        mailbox.delivery = DeliveryState::Idle;
        for notice in notices.into_iter().rev() {
            if !mailbox.acknowledged.contains(&notice.run_id) {
                mailbox.pending.push_front(notice);
            }
        }
        if mailbox.streak_epoch == streak_epoch {
            mailbox.streak = mailbox.streak.saturating_sub(1);
        }
        recheck && !mailbox.pending.is_empty()
    }

    /// Handle `acp:prompt_complete` on `root`. Clears `InFlight` when its turn
    /// id matches, marks `recheck` if a delivery probe is running, and returns
    /// `true` when the caller should spawn a delivery attempt.
    pub(crate) fn on_prompt_complete(&self, root: &str, completed_turn_id: Option<&str>) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return false;
        };
        match &mut mailbox.delivery {
            DeliveryState::Idle => !mailbox.pending.is_empty(),
            DeliveryState::Starting { recheck } => {
                *recheck = true;
                false
            }
            DeliveryState::InFlight { turn_id, recheck } => {
                if completed_turn_id == Some(turn_id.as_str()) {
                    mailbox.delivery = DeliveryState::Idle;
                    !mailbox.pending.is_empty()
                } else {
                    *recheck = true;
                    false
                }
            }
        }
    }

    /// Handle a turnless claim
    /// releasing on `root`. Returns `true` when the caller should spawn a
    /// delivery attempt.
    pub(crate) fn on_flush_trigger(&self, root: &str) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return false;
        };
        match &mut mailbox.delivery {
            DeliveryState::Idle => !mailbox.pending.is_empty(),
            DeliveryState::Starting { recheck } | DeliveryState::InFlight { recheck, .. } => {
                *recheck = true;
                false
            }
        }
    }

    /// Defensive clear for `InFlight { turn_id }` when `complete_prompt_turn`
    /// returns without a matching `prompt_complete` having cleared it first.
    pub(crate) fn clear_if_in_flight(&self, root: &str, turn_id: &str) -> bool {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return false;
        };
        if matches!(&mailbox.delivery, DeliveryState::InFlight { turn_id: active, .. } if active == turn_id)
        {
            mailbox.delivery = DeliveryState::Idle;
            !mailbox.pending.is_empty()
        } else {
            false
        }
    }

    /// Arm a retry for `root` when notices are pending and no retry timer is
    /// pending. Returns the delay to wait, or `None` when nothing is pending,
    /// a timer is already pending, or the retries ran out.
    pub(crate) fn arm_retry(&self, root: &str) -> Option<std::time::Duration> {
        let mut mailboxes = self.mailboxes.lock();
        let mailbox = mailboxes.get_mut(root)?;
        if mailbox.pending.is_empty() || mailbox.retry_armed || mailbox.retries >= MAX_RETRIES {
            return None;
        }
        let delay = FIRST_RETRY_DELAY
            .saturating_mul(1 << mailbox.retries.min(4))
            .min(MAX_RETRY_DELAY);
        mailbox.retries += 1;
        mailbox.retry_armed = true;
        Some(delay)
    }

    /// The retry timer of `root` fired.
    pub(crate) fn disarm_retry(&self, root: &str) {
        if let Some(mailbox) = self.mailboxes.lock().get_mut(root) {
            mailbox.retry_armed = false;
        }
    }

    /// Reset the retry budget on any accepted prompt, and the consecutive-wake
    /// streak when the prompt is not a wake.
    pub(crate) fn on_prompt_accepted(&self, root: &str, turn_id: Option<&str>) {
        let mut mailboxes = self.mailboxes.lock();
        let Some(mailbox) = mailboxes.get_mut(root) else {
            return;
        };
        mailbox.retries = 0;
        if turn_id.is_some_and(|id| id.starts_with(WAKE_TURN_PREFIX)) {
            return;
        }
        mailbox.streak = 0;
        mailbox.limit_logged = false;
        mailbox.streak_epoch = mailbox.streak_epoch.wrapping_add(1);
    }

    /// Dispose the root chat's mailbox on chat close.
    pub(crate) fn dispose_root(&self, root: &str) -> bool {
        self.mailboxes.lock().remove(root).is_some()
    }

    #[cfg(test)]
    pub(crate) fn has_mailbox(&self, root: &str) -> bool {
        self.mailboxes.lock().contains_key(root)
    }

    #[cfg(test)]
    pub(crate) fn pending_count(&self, root: &str) -> usize {
        self.mailboxes
            .lock()
            .get(root)
            .map_or(0, |mailbox| mailbox.pending.len())
    }

    #[cfg(test)]
    pub(crate) fn streak(&self, root: &str) -> u32 {
        self.mailboxes
            .lock()
            .get(root)
            .map_or(0, |mailbox| mailbox.streak)
    }
}

/// Host event sink that connects `AcpManager`'s root-chat lifecycle events
/// (`acp:prompt_complete`, accepted prompts) to the
/// orchestrator's wake mailbox. Constructed before `AcpManager` and attached
/// once [`OrchestratorEngine`] is built.
#[derive(Default)]
pub struct WakeSink {
    engine: OnceLock<Weak<OrchestratorEngine>>,
}

impl WakeSink {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn attach(&self, engine: &Arc<OrchestratorEngine>) {
        let _ = self.engine.set(Arc::downgrade(engine));
    }
}

impl EventSink for WakeSink {
    fn emit(&self, event: &AcpEvent) {
        let Some(sid) = event.sid.as_deref() else {
            return;
        };
        let Some(engine) = self.engine.get().and_then(Weak::upgrade) else {
            return;
        };
        if event.type_ == events::EVENT_PROMPT_COMPLETE {
            let turn_id = event
                .payload
                .get("turnId")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty());
            engine.on_root_prompt_complete(sid, turn_id);
        }
    }

    fn note_prompt_accepted(&self, session_id: &str, turn_id: Option<&str>) {
        if let Some(engine) = self.engine.get().and_then(Weak::upgrade) {
            engine.on_root_prompt_accepted(session_id, turn_id);
        }
    }

    fn note_turn_claim_released(&self, session_id: &str) {
        if let Some(engine) = self.engine.get().and_then(Weak::upgrade) {
            engine.on_prompt_resolved(session_id);
        }
    }
}

impl OrchestratorEngine {
    /// Record a settled run's notice and trigger a wake delivery attempt.
    pub(super) fn enqueue_and_deliver(self: &Arc<Self>, root: String, notice: WakeNotice) {
        if self.wake.enqueue(&root, notice) {
            self.spawn_try_deliver(root);
        }
    }

    /// Flush queued wakes after a root chat turn ends.
    pub(crate) fn on_root_prompt_complete(self: &Arc<Self>, root: &str, turn_id: Option<&str>) {
        if self.wake.on_prompt_complete(root, turn_id) {
            self.spawn_try_deliver(root.to_string());
        }
    }

    /// Reset the consecutive-wake streak when a non-wake prompt is accepted on
    /// `root`.
    pub(crate) fn on_root_prompt_accepted(&self, root: &str, turn_id: Option<&str>) {
        self.wake.on_prompt_accepted(root, turn_id);
    }

    /// Flush queued wakes when a turnless claim
    /// on `root` clears.
    pub(crate) fn on_prompt_resolved(self: &Arc<Self>, root: &str) {
        if self.wake.on_flush_trigger(root) {
            self.spawn_try_deliver(root.to_string());
        }
    }

    /// Re-attempt delivery later: the root chat could not take the wake for a
    /// reason no flush trigger reports (no live owner yet, a refused prompt).
    fn schedule_retry(self: &Arc<Self>, root: String) {
        let Some(delay) = self.wake.arm_retry(&root) else {
            return;
        };
        let engine = Arc::clone(self);
        self.handle.spawn(async move {
            tokio::time::sleep(delay).await;
            engine.wake.disarm_retry(&root);
            engine.spawn_try_deliver(root);
        });
    }

    fn spawn_try_deliver(self: &Arc<Self>, root: String) {
        if self.relay.is_none() {
            return;
        }
        let engine = Arc::clone(self);
        self.handle.spawn(async move {
            engine.try_deliver(root).await;
        });
    }

    async fn try_deliver(self: Arc<Self>, root: String) {
        let Some(relay) = self.relay.clone() else {
            return;
        };
        loop {
            if !self.wake.begin_delivery(&root) {
                return;
            }
            let session_id = SessionId(root.clone());
            let owner =
                crate::acp::limit_runtime::live_owner(&self.manager, &session_id, None).await;
            // A running turn ends with `acp:prompt_complete`, which flushes;
            // a chat with no live owner has no such trigger, so it retries.
            let (ready_agent, retry) = match owner {
                Some(agent_id) => {
                    let active = self
                        .manager
                        .is_turn_active(&agent_id, session_id.clone())
                        .await
                        .unwrap_or(true);
                    ((!active).then_some(agent_id), false)
                }
                None => (None, true),
            };
            let Some(agent_id) = ready_agent else {
                if self.wake.abort_starting(&root) {
                    continue;
                }
                if retry {
                    self.schedule_retry(root);
                }
                return;
            };
            let turn_id = format!("{WAKE_TURN_PREFIX}{}", uuid::Uuid::new_v4());
            let Some(taken) = self.wake.take_for_delivery(&root, turn_id.clone()) else {
                return;
            };
            let prompt = format_wake_prompt(&taken.notices);
            let blocks = vec![ContentBlock::Text(TextContent::new(prompt))];
            let request = PromptTurnRequest {
                agent_id,
                session_id,
                content: blocks.clone(),
                record_content: blocks,
                turn_id: Some(turn_id.clone()),
            };
            let chat = crate::logging::redact_session_id(&root);
            match accept_prompt_turn(&self.manager, &relay, request).await {
                Ok(PromptAdmission::Running(running)) => {
                    log::info!(
                        "[orchestrator] woke chat {chat} with {} sub-agent update(s) \
                         (streak {}/{MAX_WAKE_STREAK})",
                        taken.notices.len(),
                        taken.streak
                    );
                    let engine = Arc::clone(&self);
                    let acp = Arc::clone(&self.manager);
                    let root_done = root.clone();
                    let turn_done = turn_id.clone();
                    self.handle.spawn(async move {
                        match complete_prompt_turn(&acp, running).await {
                            Ok(reason) => log::info!(
                                "[orchestrator] wake turn of chat {chat} ended ({reason:?})"
                            ),
                            Err(_) => {
                                log::warn!("[orchestrator] wake turn of chat {chat} failed")
                            }
                        }
                        if engine.wake.clear_if_in_flight(&root_done, &turn_done) {
                            engine.spawn_try_deliver(root_done);
                        }
                    });
                    return;
                }
                Ok(PromptAdmission::Receipt(_)) => {
                    if self.wake.clear_if_in_flight(&root, &turn_id) {
                        continue;
                    }
                    return;
                }
                Err(error) => {
                    let is_busy = matches!(error, PromptTurnError::Busy { .. })
                        || matches!(
                            &error,
                            PromptTurnError::Failed(msg) if msg.starts_with("ACP_TURN_IN_PROGRESS")
                        );
                    log::warn!(
                        "[orchestrator] wake prompt for chat {chat} was not accepted ({})",
                        error.kind()
                    );
                    let recheck = self.wake.restore_failed_delivery(
                        &root,
                        &turn_id,
                        taken.notices,
                        taken.streak_epoch,
                    );
                    if is_busy && recheck {
                        continue;
                    }
                    if !is_busy {
                        self.schedule_retry(root);
                    }
                    return;
                }
            }
        }
    }
}

fn state_label(state: RunState) -> &'static str {
    match state {
        RunState::Running => "running",
        RunState::RequiresAction => "requires_action",
        RunState::Completed => "completed",
        RunState::Cancelled => "cancelled",
        RunState::Failed => "failed",
    }
}

/// UTF-8 safe prefix cut at `max_bytes`. Returns `(slice, was_cut)`.
pub(crate) fn truncate_utf8_bytes(text: &str, max_bytes: usize) -> (&str, bool) {
    if text.len() <= max_bytes {
        return (text, false);
    }
    let mut cut = max_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (&text[..cut], true)
}

/// Format one or more settled run notices into a single plain-text wake prompt.
pub(crate) fn format_wake_prompt(notices: &[WakeNotice]) -> String {
    let mut out = String::from(WAKE_PROMPT_HEADER);
    for notice in notices {
        let _ = write!(
            out,
            "\n\n{} (subagentId: {}, agent: {}, state: {})",
            notice.title,
            notice.run_id.as_str(),
            notice.agent_name,
            state_label(notice.state),
        );
        if let Some(ws) = &notice.workspace {
            if let Some(path) = &ws.path {
                let _ = write!(out, "\nWorkspace path: {path}");
            }
            if let Some(branch) = &ws.branch {
                let _ = write!(out, "\nWorkspace branch: {branch}");
            }
            if let Some(diff_stat) = &ws.diff_stat {
                let _ = write!(out, "\nWorkspace diff: {diff_stat}");
            }
        }
        if let Some(error) = notice.error.as_deref().filter(|err| !err.is_empty()) {
            let _ = write!(out, "\nError: {error}");
        }
        if let Some(result) = notice.result.as_deref().filter(|text| !text.is_empty()) {
            let (inlined, truncated) = truncate_utf8_bytes(result, MAX_INLINE_RESULT_BYTES);
            let _ = write!(out, "\nResult:\n{inlined}");
            if truncated {
                let _ = write!(
                    out,
                    "\n[Result truncated at {MAX_INLINE_RESULT_BYTES} bytes; call subagent_wait \
                     with subagentId \"{}\" for the full result.]",
                    notice.run_id.as_str()
                );
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "wake_tests.rs"]
mod wake_tests;
