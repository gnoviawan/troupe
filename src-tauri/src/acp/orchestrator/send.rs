//! `subagent_send`: send a follow-up message to a live or settled subagent run
//! of the caller's chat. Split from `engine.rs` to keep both under the
//! file-size rule; the impl block belongs to `OrchestratorEngine`.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::config::{AgentConfig, AgentId, SessionId};
use crate::acp::manager::ChildOrigin;
use crate::acp::orchestrator::engine::{
    OrchestratorEngine, Reservation, MAX_TASK_CHARS, SPAWN_BOUND,
};
use crate::acp::orchestrator::lifecycle::ChildLease;
use crate::acp::orchestrator::profiles::{self, SubagentApprovals, SubagentProfile};
use crate::acp::orchestrator::run::{
    bounded, CallerRef, QueuedPrompt, RunBody, RunCapture, RunEnd, RunId, RunState, SendError,
    SendOutcome, SendRequest, Starting, MAX_QUEUED_PROMPTS,
};
use crate::acp::orchestrator::sinks::{RerouteFilter, RunWatcher};
use crate::acp::subagents::{SubagentAgent, SubagentWorkspaceMode};
use crate::web::EventSink;

impl OrchestratorEngine {
    pub async fn send(
        self: &Arc<Self>,
        caller: CallerRef,
        req: SendRequest,
    ) -> Result<SendOutcome, SendError> {
        self.send_until(caller, req, CancellationToken::new()).await
    }

    /// [`send`](Self::send), aborted if `abandoned` fires (the MCP caller hung
    /// up while a settled run was being resumed).
    pub async fn send_until(
        self: &Arc<Self>,
        caller: CallerRef,
        req: SendRequest,
        abandoned: CancellationToken,
    ) -> Result<SendOutcome, SendError> {
        let subagent_id = req.subagent_id.trim();
        if subagent_id.is_empty() || subagent_id.len() > crate::acp::subagents::MAX_ID_CHARS {
            return Err(SendError::InvalidInput("subagentId"));
        }
        let message_chars = req.message.chars().count();
        if req.message.trim().is_empty() || message_chars > MAX_TASK_CHARS {
            return Err(SendError::InvalidInput("message"));
        }

        let run = RunId(subagent_id.to_string());
        let sender = self.caller_sender_name(&caller);

        enum Decision {
            Queued {
                state: RunState,
                cancel: Option<(AgentId, SessionId)>,
                cleared_permission: bool,
            },
            WaitSettling(tokio::sync::oneshot::Receiver<()>),
            Resume(ResumeMeta),
        }

        let resume_meta = loop {
            let decision = {
                let mut ledger = self.ledger.lock();
                if ledger.starting.contains_key(&run) {
                    return Err(SendError::Busy(
                        "the subagent is already starting or resuming",
                    ));
                }
                let Some(entry) = ledger.runs.get_mut(&run) else {
                    return Err(SendError::NotFound);
                };
                if entry.parent_session_id != caller.session.0 {
                    return Err(SendError::NotFound);
                }
                if entry.session() == Some(&caller.session)
                    && entry.child_agent() == Some(&caller.agent)
                {
                    return Err(SendError::InvalidInput("subagentId"));
                }
                match &mut entry.body {
                    RunBody::Spawning { .. } => {
                        return Err(SendError::Busy("the subagent is still starting"));
                    }
                    RunBody::Stopping { .. } => {
                        return Err(SendError::Busy("the subagent is stopping"));
                    }
                    RunBody::Settling { .. } => {
                        entry.waiters.retain(|waiter| !waiter.is_closed());
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        entry.waiters.push(tx);
                        Decision::WaitSettling(rx)
                    }
                    RunBody::Live {
                        child_agent,
                        session,
                        awaiting_permission,
                    } => {
                        if entry.queued_prompts.len() >= MAX_QUEUED_PROMPTS {
                            return Err(SendError::Busy(
                                "too many follow-up messages are already queued for this subagent",
                            ));
                        }
                        entry.queued_prompts.push_back(QueuedPrompt {
                            message: req.message.clone(),
                            sender: sender.clone(),
                        });
                        let (cancel, cleared_permission) = if req.interrupt {
                            let cleared = std::mem::replace(awaiting_permission, false);
                            (Some((child_agent.clone(), session.clone())), cleared)
                        } else {
                            (None, false)
                        };
                        Decision::Queued {
                            state: entry.state(),
                            cancel,
                            cleared_permission,
                        }
                    }
                    RunBody::Settled {
                        end: RunEnd::Failed(_),
                        ..
                    } => {
                        return Err(SendError::NotResumable(
                            "cannot send a follow-up to a failed subagent run",
                        ));
                    }
                    RunBody::Settled {
                        end: RunEnd::Completed | RunEnd::Cancelled,
                        ..
                    } => {
                        let Some(session_id) = entry.session_id.clone() else {
                            return Err(SendError::NotResumable(
                                "this subagent run has no resumable session",
                            ));
                        };
                        Decision::Resume(ResumeMeta {
                            root: entry.root.clone(),
                            parent_agent: entry.parent_agent.clone(),
                            parent_session_id: entry.parent_session_id.clone(),
                            parent_run: entry.parent_run.clone(),
                            depth: entry.depth,
                            profile_id: entry.profile_id.clone(),
                            execution_policy: entry.execution_policy.clone(),
                            child_cwd: entry.child_cwd.clone(),
                            workspace_mode: entry.workspace.mode,
                            session_id,
                            capture: entry.capture.clone(),
                        })
                    }
                }
            };

            match decision {
                Decision::Queued {
                    state,
                    cancel,
                    cleared_permission,
                } => {
                    let run_log = crate::logging::redact_session_id(run.as_str());
                    if cleared_permission {
                        let engine = self.clone();
                        let run = run.clone();
                        self.handle.spawn(async move {
                            engine.send_change_card(&run).await;
                        });
                    }
                    if let Some((child_agent, session)) = cancel {
                        log::info!(
                            "[orchestrator] run {run_log} interrupting active turn for follow-up"
                        );
                        if let Err(error) = self.manager.cancel_prompt(&child_agent, session).await
                        {
                            log::warn!(
                                "[orchestrator] run {run_log} interrupt cancel failed: {error}"
                            );
                        }
                    } else {
                        log::info!("[orchestrator] run {run_log} queued follow-up message");
                    }
                    return Ok(SendOutcome {
                        run,
                        state,
                        queued: true,
                    });
                }
                Decision::WaitSettling(rx) => {
                    let waited = tokio::select! {
                        biased;
                        result = tokio::time::timeout(SPAWN_BOUND, rx) => result,
                        () = abandoned.cancelled() => {
                            return Err(SendError::SpawnFailed(
                                "the caller cancelled before the subagent settled".to_string(),
                            ));
                        }
                    };
                    if waited.is_err() {
                        return Err(SendError::Busy(
                            "timed out waiting for the subagent to settle",
                        ));
                    }
                }
                Decision::Resume(meta) => break meta,
            }
        };

        let checks = self.resume_checks(&resume_meta)?;
        let reservation =
            self.reserve_resume(&run, &resume_meta, checks.max_running, &abandoned)?;
        let cancel = reservation.cancel.clone();
        let progress: Arc<Mutex<ResumeProgress>> = Arc::default();
        let resume = {
            let engine = self.clone();
            let run = run.clone();
            let progress = Arc::clone(&progress);
            let message = req.message.clone();
            let sender = sender.clone();
            let cancel = cancel.clone();
            async move {
                engine
                    .resume_inner(&run, resume_meta, checks, message, sender, cancel, progress)
                    .await
            }
        };

        let outcome = tokio::select! {
            biased;
            outcome = tokio::time::timeout(SPAWN_BOUND, resume) => outcome.unwrap_or_else(|_| {
                Err(SendError::SessionUnloadable("reloading the subagent session timed out".to_string()))
            }),
            () = cancel.cancelled() => Err(SendError::SpawnFailed(
                "the subagent was stopped while resuming".to_string(),
            )),
        };

        if let Err(error) = &outcome {
            let end = if cancel.is_cancelled() {
                RunEnd::Cancelled
            } else {
                RunEnd::Failed(bounded(&error.to_string(), 512))
            };
            self.abort_resume(&run, &progress, end).await;
        }
        drop(reservation);
        outcome
    }

    fn caller_sender_name(&self, caller: &CallerRef) -> String {
        if let Some(title) = self
            .ledger
            .lock()
            .run_with_session(caller)
            .map(|run| run.title.clone())
        {
            if !title.trim().is_empty() {
                return title;
            }
        }
        self.manager
            .agent_name(&caller.agent)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Parent agent".to_string())
    }

    fn resume_checks(&self, meta: &ResumeMeta) -> Result<ResumeChecks, SendError> {
        let settings = self.settings.get();
        if !settings.enabled {
            return Err(SendError::Disabled);
        }
        if meta.workspace_mode == SubagentWorkspaceMode::Worktree
            && !Path::new(&meta.child_cwd).is_dir()
        {
            return Err(SendError::SessionUnloadable(
                "the subagent's worktree directory no longer exists".to_string(),
            ));
        }
        let launchable = self.launchable();
        let Some(resolved) = launchable
            .find(&settings, &meta.profile_id)
            .or_else(|| launchable.find(&settings, &self.canonical_agent(&meta.profile_id)))
        else {
            return Err(SendError::UnknownAgent(meta.profile_id.clone()));
        };
        let profile = resolved.profile;
        if !profile.enabled {
            return Err(SendError::AgentNotAllowed(profile.id));
        }
        let Some((config, mut original, model)) = meta.execution_policy.clone() else {
            return Err(SendError::NotResumable(
                "original execution policy unavailable",
            ));
        };
        if profile.agent_id != original.agent_id
            || profile.mode != original.mode
            || profile.config_options != original.config_options
        {
            return Err(SendError::NotResumable(
                "execution policy changed; start a new run",
            ));
        }
        if profile.approvals == SubagentApprovals::Ask {
            original.approvals = SubagentApprovals::Ask;
        }
        let profile = original;
        Ok(ResumeChecks {
            config,
            model,
            profile,
            max_running: settings.max_running,
        })
    }

    /// Re-verify under the ledger lock that `run` is still resumable and claim
    /// a capacity slot in `ledger.starting`. Dropping the returned reservation
    /// releases the slot.
    fn reserve_resume(
        self: &Arc<Self>,
        run: &RunId,
        meta: &ResumeMeta,
        max: u32,
        abandoned: &CancellationToken,
    ) -> Result<Reservation, SendError> {
        let mut ledger = self.ledger.lock();
        if ledger.starting.contains_key(run) {
            return Err(SendError::Busy(
                "the subagent is already starting or resuming",
            ));
        }
        let Some(entry) = ledger.runs.get(run) else {
            return Err(SendError::NotFound);
        };
        if entry.root != meta.root {
            return Err(SendError::NotFound);
        }
        match &entry.body {
            RunBody::Settled {
                end: RunEnd::Completed | RunEnd::Cancelled,
                ..
            } => {}
            RunBody::Settled {
                end: RunEnd::Failed(_),
                ..
            } => {
                return Err(SendError::NotResumable(
                    "cannot send a follow-up to a failed subagent run",
                ));
            }
            _ => {
                return Err(SendError::Busy("the subagent state changed while resuming"));
            }
        }
        if ledger.occupied_of_root(&meta.root) >= max as usize {
            return Err(SendError::Capacity { max });
        }
        let cancel = abandoned.child_token();
        ledger.starting.insert(
            run.clone(),
            Starting {
                root: meta.root.clone(),
                parent_agent: meta.parent_agent.clone(),
                cancel: cancel.clone(),
            },
        );
        Ok(Reservation {
            engine: self.clone(),
            run: run.clone(),
            cancel,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn resume_inner(
        self: &Arc<Self>,
        run: &RunId,
        meta: ResumeMeta,
        checks: ResumeChecks,
        message: String,
        sender: String,
        cancel: CancellationToken,
        progress: Arc<Mutex<ResumeProgress>>,
    ) -> Result<SendOutcome, SendError> {
        let run_log = crate::logging::redact_session_id(run.as_str());
        let chat_log = crate::logging::redact_session_id(&meta.root);
        let agent_name = checks.config.name.clone();
        log::info!("[orchestrator] run {run_log} resuming in chat {chat_log}");

        let sinks: Vec<Arc<dyn EventSink>> = vec![
            Arc::new(RerouteFilter::new(
                meta.root.clone(),
                self.manager.sinks().to_vec(),
            )),
            Arc::new(RunWatcher::new(
                self.clone(),
                run.clone(),
                meta.capture.clone(),
            )),
        ];
        let project_id = self.project_id(&meta.root);
        let spawned = self
            .manager
            .spawn_private(
                checks.config,
                project_id.as_deref(),
                &crate::acp::claude_agent::LaunchChat::Existing(SessionId(meta.root.clone())),
                sinks,
            )
            .await
            .map_err(|error| {
                log::warn!(
                    "[orchestrator] run {run_log} resume spawn ({agent_name}) failed: {error}"
                );
                SendError::SpawnFailed(error)
            })?;
        let child_agent = spawned.agent_id;
        progress.lock().child = Some(child_agent.clone());

        let origin = ChildOrigin {
            root_session_id: meta.root.clone(),
            parent_session_id: meta.parent_session_id.clone(),
            parent_dispatch_id: meta.parent_run.as_ref().map(|parent| parent.dispatch_id()),
            dispatch_id: run.dispatch_id(),
            depth: meta.depth,
            auto_approve: checks.profile.approvals == SubagentApprovals::AllowAll,
        };
        let loaded = self
            .manager
            .load_child_session(
                &child_agent,
                meta.session_id.clone(),
                meta.child_cwd.clone(),
                origin,
            )
            .await
            .map_err(|error| {
                log::warn!("[orchestrator] run {run_log} session/load failed: {error}");
                SendError::SessionUnloadable(bounded(&error, 512))
            })?;

        let model = profiles::apply_to_session(
            &self.manager,
            &child_agent,
            &meta.session_id,
            checks.model,
            &checks.profile,
            loaded.config_options.as_deref(),
            &run_log,
        )
        .await;

        if cancel.is_cancelled() {
            return Err(SendError::SpawnFailed(
                "the subagent was stopped while resuming".to_string(),
            ));
        }

        let (snapshot, record) = {
            let mut ledger = self.ledger.lock();
            let Some(entry) = ledger.runs.get_mut(run) else {
                return Err(SendError::NotFound);
            };
            let snapshot = PreviousRunState {
                body: entry.body.clone(),
                result: entry.result.clone(),
                identity: entry.identity.clone(),
                model: entry.model.clone(),
            };
            entry.body = RunBody::Live {
                child_agent: child_agent.clone(),
                session: meta.session_id.clone(),
                awaiting_permission: false,
            };
            entry.identity.agent_id = child_agent.0.clone();
            entry.model = model;
            entry.result = None;
            entry.capture.clear_text();
            (snapshot, entry.record())
        };
        progress.lock().previous = Some(snapshot);

        // From here on, failures settle through the card path, even while the
        // resume card itself is still queued on the parent driver.
        progress.lock().resumed_card = true;
        if let Err(error) = self
            .manager
            .resume_subagent_record(&meta.parent_agent, record)
            .await
        {
            log::warn!("[orchestrator] run {run_log} resume card failed: {error}");
            return Err(SendError::SpawnFailed(error));
        }

        self.emit_parent_message(run, &meta.root, &meta.parent_agent, &sender, &message);

        let content = vec![agent_client_protocol::schema::v1::ContentBlock::Text(
            agent_client_protocol::schema::v1::TextContent::new(message),
        )];
        let started = self
            .manager
            .start_prompt(&child_agent, meta.session_id, content, None, None)
            .await
            .map_err(|error| {
                log::warn!("[orchestrator] run {run_log} resumed prompt failed: {error}");
                SendError::SpawnFailed(bounded(&error, 512))
            })?;

        let engine = self.clone();
        let supervise_run = run.clone();
        let lease = ChildLease {
            manager: self.manager.clone(),
            child: child_agent,
        };
        self.handle.spawn(async move {
            engine.supervise(supervise_run, lease, started).await;
        });

        Ok(SendOutcome {
            run: run.clone(),
            state: RunState::Running,
            queued: false,
        })
    }

    async fn abort_resume(
        self: &Arc<Self>,
        run: &RunId,
        progress: &Mutex<ResumeProgress>,
        end: RunEnd,
    ) {
        let (child, previous, resumed_card) = {
            let mut progress = progress.lock();
            (
                progress.child.take(),
                progress.previous.take(),
                progress.resumed_card,
            )
        };
        if let Some(child) = child {
            let _ = self.manager.kill(&child).await;
        }
        if resumed_card {
            // The UI already saw the card return to Running; settle it so it
            // does not stay stuck running.
            self.finish(run, end).await;
        } else if let Some(previous) = previous {
            let mut ledger = self.ledger.lock();
            if let Some(entry) = ledger.runs.get_mut(run) {
                entry.body = previous.body;
                entry.result = previous.result;
                entry.identity = previous.identity;
                entry.model = previous.model;
            }
        }
    }
}

struct ResumeMeta {
    root: String,
    parent_agent: AgentId,
    parent_session_id: String,
    parent_run: Option<RunId>,
    depth: u8,
    profile_id: String,
    execution_policy: Option<(AgentConfig, SubagentProfile, Option<String>)>,
    child_cwd: String,
    workspace_mode: SubagentWorkspaceMode,
    session_id: SessionId,
    capture: Arc<RunCapture>,
}

struct ResumeChecks {
    config: AgentConfig,
    model: Option<String>,
    profile: SubagentProfile,
    max_running: u32,
}

struct PreviousRunState {
    body: RunBody,
    result: Option<String>,
    identity: SubagentAgent,
    model: Option<String>,
}

#[derive(Default)]
struct ResumeProgress {
    child: Option<AgentId>,
    previous: Option<PreviousRunState>,
    resumed_card: bool,
}

#[cfg(test)]
#[path = "send_tests.rs"]
mod send_tests;
