//! The run lifecycle: everything that happens to a run after `start`
//! returns. The supervision task, the settle path, stops (from the agent
//! tool, the renderer, a closing chat, or a dying parent agent), and the
//! cards those paths send. Split from `engine.rs` to keep both under the
//! file-size rule; the impl blocks are one engine.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::acp::config::{AgentId, SessionId};
use crate::acp::manager::{AcpManager, StartedPrompt};
use crate::acp::orchestrator::engine::{OrchestratorEngine, StartProgress};
use crate::acp::orchestrator::run::{
    bounded, ChildRun, QueuedPrompt, RunBody, RunEnd, RunId, StopError, StopOrigin, StopOutcome,
};
use crate::acp::orchestrator::wake::WakeNotice;
use crate::acp::subagents::{Subagent, SubagentWorkspaceMode};

/// After a stop, how long the cancel has to settle before the child is
/// killed outright.
const STOP_BACKSTOP: std::time::Duration = std::time::Duration::from_secs(10);

impl OrchestratorEngine {
    /// Clean up a start that failed, timed out or was cancelled. Nothing
    /// ran yet: no prompt was accepted. An announced run (its card went
    /// out, or may have) settles with `end`; an unannounced one is
    /// forgotten. The child is killed and the worktree made for it is
    /// removed with its branch before the settle card, so neither waits on
    /// the parent driver's reply.
    pub(super) async fn abort_start(
        self: &Arc<Self>,
        run: &RunId,
        progress: &Mutex<StartProgress>,
        end: RunEnd,
    ) {
        let (child, worktree, announced) = {
            let mut progress = progress.lock();
            progress.abandoned = true;
            (
                progress.child.take(),
                progress.worktree.take(),
                progress.announced,
            )
        };
        {
            let mut ledger = self.ledger.lock();
            if !announced {
                ledger.remove(run);
            } else if let Some(entry) = ledger.runs.get_mut(run) {
                if worktree.is_some() {
                    entry.workspace.path = None;
                    entry.workspace.branch = None;
                }
            }
        }
        if let Some(child) = child {
            self.manager.kill_detached(&child);
        }
        if let Some((root, path, branch)) = worktree {
            crate::acp::orchestrator::worktree::discard(root, path, branch).await;
        }
        if announced {
            self.finish(run, end).await;
        }
    }

    /// The supervision task of one run: park on the child's turn, deliver any
    /// queued follow-up prompts on the same live session when a turn finishes
    /// normally or after a turn-only interrupt, then shut down the child and
    /// settle when no follow-up remains.
    pub(super) async fn supervise(
        self: Arc<Self>,
        run: RunId,
        lease: ChildLease,
        mut started: StartedPrompt,
    ) {
        enum Step {
            Deliver {
                child_agent: AgentId,
                session: SessionId,
                root: String,
                parent_agent: AgentId,
                queued: QueuedPrompt,
            },
            Settle {
                child_agent: AgentId,
            },
            AlreadySettled,
        }

        loop {
            let outcome = self.manager.wait_prompt(started).await;
            let turn_end = match &outcome {
                Ok(agent_client_protocol::schema::v1::StopReason::Cancelled) => RunEnd::Cancelled,
                Ok(_) => RunEnd::Completed,
                Err(error) => RunEnd::Failed(bounded(error, 512)),
            };
            let step = {
                let mut ledger = self.ledger.lock();
                let Some(entry) = ledger.runs.get_mut(&run) else {
                    drop(lease);
                    return;
                };
                match (&entry.body, &turn_end) {
                    (
                        RunBody::Live {
                            child_agent,
                            session,
                            ..
                        },
                        RunEnd::Completed | RunEnd::Cancelled,
                    ) => {
                        if let Some(queued) = entry.queued_prompts.pop_front() {
                            entry.capture.clear_text();
                            Step::Deliver {
                                child_agent: child_agent.clone(),
                                session: session.clone(),
                                root: entry.root.clone(),
                                parent_agent: entry.parent_agent.clone(),
                                queued,
                            }
                        } else {
                            let (child_agent, session) = (child_agent.clone(), session.clone());
                            entry.body = RunBody::Settling {
                                child_agent: child_agent.clone(),
                                session,
                            };
                            Step::Settle { child_agent }
                        }
                    }
                    (RunBody::Settled { .. }, _) => Step::AlreadySettled,
                    _ => {
                        entry.queued_prompts.clear();
                        let child_agent = entry
                            .child_agent()
                            .cloned()
                            .unwrap_or_else(|| lease.child.clone());
                        if let Some(session) = entry.session().cloned() {
                            entry.body = RunBody::Settling {
                                child_agent: child_agent.clone(),
                                session,
                            };
                        }
                        Step::Settle { child_agent }
                    }
                }
            };
            match step {
                Step::AlreadySettled => {
                    drop(lease);
                    return;
                }
                Step::Settle { child_agent } => {
                    // Join the child driver before waking waiters so an
                    // immediate `subagent_send` resume cannot race teardown.
                    let _ = self.manager.kill(&child_agent).await;
                    self.finish(&run, turn_end).await;
                    drop(lease);
                    return;
                }
                Step::Deliver {
                    child_agent,
                    session,
                    root,
                    parent_agent,
                    queued,
                } => {
                    let run_log = crate::logging::redact_session_id(run.as_str());
                    log::info!("[orchestrator] run {run_log} delivering queued follow-up");
                    self.emit_parent_message(
                        &run,
                        &root,
                        &parent_agent,
                        &queued.sender,
                        &queued.message,
                    );
                    let content = vec![agent_client_protocol::schema::v1::ContentBlock::Text(
                        agent_client_protocol::schema::v1::TextContent::new(queued.message),
                    )];
                    match self
                        .manager
                        .start_prompt(&child_agent, session.clone(), content, None, None)
                        .await
                    {
                        Ok(next_started) => {
                            let stopped = self.ledger.lock().runs.get(&run).is_some_and(|entry| {
                                matches!(entry.body, RunBody::Stopping { .. })
                            });
                            if stopped {
                                let _ = self.manager.cancel_prompt(&child_agent, session).await;
                            }
                            started = next_started;
                        }
                        Err(error) => {
                            log::warn!(
                                "[orchestrator] run {run_log} queued prompt failed: {error}"
                            );
                            if let Some(entry) = self.ledger.lock().runs.get_mut(&run) {
                                entry.queued_prompts.clear();
                            }
                            let _ = self.manager.kill(&child_agent).await;
                            self.finish(&run, RunEnd::Failed(bounded(&error, 512)))
                                .await;
                            drop(lease);
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Emit a parent-attributed follow-up message onto the root chat's sinks
    /// so it renders in the sub-agent panel transcript in chronological order
    /// and persists into `tool-calls.jsonl`.
    pub(super) fn emit_parent_message(
        &self,
        run: &RunId,
        root: &str,
        parent_agent: &AgentId,
        sender: &str,
        message: &str,
    ) {
        let sender = bounded(sender, crate::acp::subagents::MAX_TEXT_CHARS);
        let sender = if sender.is_empty() {
            "Parent agent".to_string()
        } else {
            sender
        };
        let event = crate::acp::events::MessageChunkEvent {
            agent_id: parent_agent.clone(),
            session_id: SessionId(root.to_string()),
            role: crate::acp::events::ChunkRole::User,
            content: agent_client_protocol::schema::v1::ContentBlock::Text(
                agent_client_protocol::schema::v1::TextContent::new(message.to_string()),
            ),
            message_id: Some(format!("parent-send-{}", uuid::Uuid::new_v4().simple())),
            parent_tool_call_id: Some(run.dispatch_id()),
            sender: Some(sender),
        };
        crate::acp::events::fan_out(
            self.manager.sinks(),
            Some(root),
            crate::acp::events::EVENT_MESSAGE_CHUNK,
            &event,
        );
    }

    /// Settle the run (first wins), resolve its waiters, compute the diff
    /// stat, send the settle card with the result the watcher captured, and
    /// enqueue a wake notice for the root chat when applicable.
    pub(super) async fn finish(self: &Arc<Self>, run: &RunId, end: RunEnd) {
        let (parent_agent, mut record, content, base_commit, wake_draft) = {
            let mut ledger = self.ledger.lock();
            let Some(entry) = ledger.runs.get_mut(run) else {
                return;
            };
            let should_wake = match (&end, entry.stop_origin) {
                (_, Some(StopOrigin::ChatClosed)) => false,
                (RunEnd::Completed | RunEnd::Failed(_), _) => true,
                (RunEnd::Cancelled, None) => true,
                (RunEnd::Cancelled, Some(StopOrigin::Parent | StopOrigin::User)) => false,
            };
            if !entry.settle(end, now_millis()) {
                return;
            }
            let waiters = std::mem::take(&mut entry.waiters);
            for waiter in waiters {
                let _ = waiter.send(());
            }
            if entry.result.is_none() {
                let text = entry.capture.result_text();
                entry.result = (!text.is_empty()).then_some(text);
            }
            let (parent_agent, record, content) = settle_card(entry);
            let base_commit = entry
                .base_commit
                .clone()
                .filter(|_| entry.workspace.mode == SubagentWorkspaceMode::Worktree);
            let root = entry.root.clone();
            let summary = entry.summary();
            let wake_draft = (should_wake && entry.parent_run.is_none()).then(|| {
                (
                    root.clone(),
                    WakeNotice {
                        run_id: entry.run.clone(),
                        title: entry.title.clone(),
                        agent_name: entry.identity.name.clone(),
                        state: summary.state,
                        result: summary.result,
                        error: summary.error,
                        workspace: summary.workspace,
                    },
                )
            });
            ledger.evict_settled(&root);
            (parent_agent, record, content, base_commit, wake_draft)
        };
        if let Some(workspace) = &mut record.workspace {
            if workspace.mode == SubagentWorkspaceMode::Worktree {
                if let Some(path) = workspace.path.clone() {
                    workspace.diff_stat = crate::acp::orchestrator::worktree::diff_stat(
                        &path,
                        base_commit.as_deref(),
                    )
                    .await;
                }
            }
        }
        let updated_workspace = record.workspace.clone();
        if let Some(diff_stat) = updated_workspace.as_ref().and_then(|w| w.diff_stat.clone()) {
            let mut ledger = self.ledger.lock();
            if let Some(entry) = ledger.runs.get_mut(run) {
                entry.workspace.diff_stat = Some(diff_stat);
            }
        }
        if self
            .ledger
            .lock()
            .runs
            .get(run)
            .is_some_and(|entry| entry.is_live())
        {
            // A `subagent_send` already resumed the run while `diff_stat` was
            // running; do not clobber the resumed Running card with an old
            // Idle settle card.
            return;
        }
        if let Err(error) = self
            .manager
            .upsert_subagent_record(&parent_agent, record, content)
            .await
        {
            log::warn!(
                "[orchestrator] run {} settle card failed: {error}",
                crate::logging::redact_session_id(run.as_str())
            );
        }
        log::info!(
            "[orchestrator] run {} settled",
            crate::logging::redact_session_id(run.as_str())
        );
        if let Some((root, mut notice)) = wake_draft {
            let closed_meanwhile = self
                .ledger
                .lock()
                .runs
                .get(run)
                .is_some_and(|entry| entry.stop_origin == Some(StopOrigin::ChatClosed));
            if !closed_meanwhile {
                notice.workspace = updated_workspace;
                self.enqueue_and_deliver(root, notice);
            }
        }
    }

    /// `None` when the ledger holds no run of `root` with this id: the
    /// caller's stop was for something else (the native path). `Sent`
    /// reports the stop accepted, not the child's reply: the cancel goes out
    /// on its own task, and the backstop settles a run whose child ignores
    /// it or is already gone.
    pub(super) async fn stop_run(
        self: &Arc<Self>,
        run: &RunId,
        root: &str,
        origin: StopOrigin,
    ) -> Option<Result<StopOutcome, StopError>> {
        let (cancel, starting) = {
            let mut ledger = self.ledger.lock();
            let starting = ledger.starting.get(run).map(|start| start.cancel.clone());
            let entry = ledger.runs.get_mut(run)?;
            if entry.root != root {
                return None;
            }
            if !entry.is_live() && starting.is_none() {
                return Some(Ok(StopOutcome::AlreadyEnded));
            }
            entry.stop_origin.get_or_insert(origin);
            (entry.begin_stop(now_millis()), starting)
        };
        // A run still starting has its start cancelled: the start cleans up
        // and settles it, and a prompt it was about to send never goes out.
        if let Some(starting) = &starting {
            starting.cancel();
        }
        // Live with no new cancel target: still starting (cancelled above)
        // or already stopping. Either way the run has not settled yet.
        if let Some((child_agent, session)) = cancel {
            self.arm_backstop(run.clone());
            let engine = self.clone();
            let run = run.clone();
            self.handle.spawn(async move {
                if let Err(error) = engine.manager.cancel_prompt(&child_agent, session).await {
                    log::warn!(
                        "[orchestrator] run {} cancel failed: {error}",
                        crate::logging::redact_session_id(run.as_str())
                    );
                }
            });
        }
        Some(Ok(StopOutcome::Sent))
    }

    /// After [`STOP_BACKSTOP`], a run that never settled is force-settled
    /// and its child killed.
    fn arm_backstop(self: &Arc<Self>, run: RunId) {
        let engine = self.clone();
        self.handle.spawn(async move {
            tokio::time::sleep(STOP_BACKSTOP).await;
            let stuck = engine
                .ledger
                .lock()
                .runs
                .get(&run)
                .and_then(|entry| match &entry.body {
                    RunBody::Stopping {
                        since_ms,
                        child_agent,
                        ..
                    } => Some((*since_ms, child_agent.clone())),
                    _ => None,
                });
            if let Some((stopped_at, child)) = stuck {
                log::warn!(
                    "[orchestrator] run {} did not settle {} ms after its stop; killing the child",
                    crate::logging::redact_session_id(run.as_str()),
                    now_millis().saturating_sub(stopped_at)
                );
                // Kill first: the settle frees the run's capacity slot, and a
                // child that ignored the cancel must not keep working past it.
                engine.manager.kill_detached(&child);
                engine.finish(&run, RunEnd::Cancelled).await;
            }
        });
    }

    /// The manager's stop interception: `Some(result)` for a run of this
    /// engine, `None` for anything else (the native path takes over).
    pub(crate) async fn stop_from_ui(
        self: &Arc<Self>,
        root: &str,
        record_id: &str,
    ) -> Option<Result<StopOutcome, StopError>> {
        self.stop_run(&RunId(record_id.to_string()), root, StopOrigin::User)
            .await
    }

    /// Stop every live run of a closed chat and dispose its wake mailbox.
    pub(crate) async fn chat_closed(self: &Arc<Self>, root: &str) {
        self.wake.dispose_root(root);
        let (runs, starts) = {
            let mut ledger = self.ledger.lock();
            let starts = ledger.cancel_starts(|start| start.root == root);
            if let Some(ids) = ledger.by_root.get(root).cloned() {
                for id in ids {
                    if let Some(entry) = ledger.runs.get_mut(&id) {
                        entry.stop_origin = Some(StopOrigin::ChatClosed);
                    }
                }
            }
            (ledger.live_runs_of_root(root), starts)
        };
        if starts > 0 {
            log::info!(
                "[orchestrator] chat {} closed: cancelled {starts} start(s)",
                crate::logging::redact_session_id(root)
            );
        }
        if runs.is_empty() {
            return;
        }
        log::info!(
            "[orchestrator] chat {} closed: stopping {} run(s)",
            crate::logging::redact_session_id(root),
            runs.len()
        );
        for run in runs {
            let cancel = {
                let mut ledger = self.ledger.lock();
                ledger
                    .runs
                    .get_mut(&run)
                    .and_then(|entry| entry.begin_stop(now_millis()))
            };
            if let Some((child_agent, session)) = cancel {
                // The same backstop as a stop: a child stuck in a tool call
                // is killed if its turn has not ended in time.
                self.arm_backstop(run.clone());
                if let Err(error) = self.manager.cancel_prompt(&child_agent, session).await {
                    log::warn!(
                        "[orchestrator] run {} cancel before chat close failed: {error}",
                        crate::logging::redact_session_id(run.as_str())
                    );
                }
            }
        }
    }

    /// Settle the live runs whose parent agent died, kill their children
    /// and cancel their starts. Returns each settled run's card for the
    /// dying driver to deliver itself, before its session writers close:
    /// its command channel is already gone, so the usual card path cannot
    /// reach it. Sync, for the driver thread's teardown.
    pub(crate) fn on_agent_gone(
        self: &Arc<Self>,
        agent: &AgentId,
    ) -> Vec<(Subagent, Option<String>)> {
        let (cards, children) = {
            let mut ledger = self.ledger.lock();
            ledger.cancel_starts(|start| &start.parent_agent == agent);
            let runs = ledger.live_runs_of_agent(agent);
            let mut cards = Vec::new();
            let mut children = Vec::new();
            for run in &runs {
                let Some(entry) = ledger.runs.get_mut(run) else {
                    continue;
                };
                children.extend(entry.child_agent().cloned());
                if !entry.settle(RunEnd::Cancelled, now_millis()) {
                    continue;
                }
                for waiter in std::mem::take(&mut entry.waiters) {
                    let _ = waiter.send(());
                }
                let (_, record, content) = settle_card(entry);
                cards.push((record, content));
            }
            let roots: std::collections::BTreeSet<String> = runs
                .iter()
                .filter_map(|run| ledger.runs.get(run).map(|entry| entry.root.clone()))
                .collect();
            for root in roots {
                ledger.evict_settled(&root);
            }
            (cards, children)
        };
        if !cards.is_empty() {
            log::info!(
                "[orchestrator] agent {} gone: stopped {} run(s)",
                agent.0,
                cards.len()
            );
        }
        for child in children {
            self.manager.kill_detached(&child);
        }
        cards
    }

    /// The watcher's permission mirror: flip the awaiting flag and send a
    /// change card when it flipped.
    pub(crate) fn note_permission(self: &Arc<Self>, run: &RunId, awaiting: bool) {
        let flipped = {
            let mut ledger = self.ledger.lock();
            match ledger.runs.get_mut(run) {
                Some(entry) if entry.session().is_some() => entry.set_awaiting(awaiting),
                _ => false,
            }
        };
        if flipped {
            let engine = self.clone();
            let run = run.clone();
            self.handle.spawn(async move {
                engine.send_change_card(&run).await;
            });
        }
    }

    pub(super) async fn send_change_card(self: Arc<Self>, run: &RunId) {
        let (parent_agent, record) = {
            let ledger = self.ledger.lock();
            let Some(entry) = ledger.runs.get(run) else {
                return;
            };
            (entry.parent_agent.clone(), entry.record())
        };
        if let Err(error) = self
            .manager
            .upsert_subagent_record(&parent_agent, record, None)
            .await
        {
            log::warn!(
                "[orchestrator] run {} change card failed: {error}",
                crate::logging::redact_session_id(run.as_str())
            );
        }
    }
}

/// The settle card of a settled run: the parent agent that receives it, the
/// record, and the result as the card's content when the run completed.
fn settle_card(entry: &ChildRun) -> (AgentId, Subagent, Option<String>) {
    let content = entry.result.clone().filter(|_| {
        matches!(
            entry.body,
            RunBody::Settled {
                end: RunEnd::Completed,
                ..
            }
        )
    });
    (entry.parent_agent.clone(), entry.record(), content)
}

/// Stops the child agent on drop. `kill_detached` is idempotent, so the
/// drop after a settled run is a no-op.
pub(super) struct ChildLease {
    pub(super) manager: Arc<AcpManager>,
    pub(super) child: AgentId,
}

impl Drop for ChildLease {
    fn drop(&mut self) {
        self.manager.kill_detached(&self.child);
    }
}

pub(super) fn now_millis() -> u64 {
    crate::acp::session_persistence::now_millis()
}
