#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tests for host-initiated parent wake on settled sub-agents: driven through
//! the real `OrchestratorEngine`, `WakeSink`, `WsRelaySink`, `prompt_turn`,
//! and `FakeDriver` so every delivery, batching, suppression, loop-guard, and
//! disposal guarantee is asserted against literal recorded prompts.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::acp::config::{AgentId, SessionId};
use crate::acp::manager::{AcpManager, FakeTurn};
use crate::acp::orchestrator::run::{
    CallerRef, ChildRun, MessageParentError, RunBody, RunCapture, RunEnd, RunId, RunState,
};
use crate::acp::subagents::{SubagentAgent, SubagentWorkspace, SubagentWorkspaceMode};
use crate::web::permissions::{PermissionRendezvous, QuestionRendezvous};
use crate::web::prompt_turn::{
    accept_prompt_turn, complete_prompt_turn, PromptAdmission, PromptTurnRequest,
};
use crate::web::sink::ReplayResult;
use crate::web::{EventSink, StreamItem, WsRelaySink};

const PARENT_AGENT: &str = "agent-parent";
const CHAT: &str = "chat-wake-1";

struct WakeHarness {
    engine: Arc<OrchestratorEngine>,
    acp: Arc<AcpManager>,
    relay: Arc<WsRelaySink>,
    caller: CallerRef,
    _data: tempfile::TempDir,
}

impl WakeHarness {
    fn with_turns(turns: Vec<FakeTurn>) -> Self {
        let data = tempfile::tempdir().expect("tempdir");
        let relay = Arc::new(WsRelaySink::new());
        let wake_sink = WakeSink::new();
        let acp = Arc::new(AcpManager::new(vec![
            relay.clone() as Arc<dyn EventSink>,
            wake_sink.clone() as Arc<dyn EventSink>,
        ]));
        let permissions = Arc::new(PermissionRendezvous::new(Arc::clone(&acp)));
        relay.set_rendezvous(permissions);
        let questions = Arc::new(QuestionRendezvous::new(Arc::clone(&acp)));
        relay.set_question_rendezvous(questions);
        let mut sessions = HashSet::new();
        sessions.insert(CHAT.to_string());
        acp.install_test_agent_with_turns(
            AgentId(PARENT_AGENT.to_string()),
            sessions,
            turns,
            false,
        );
        let engine = Arc::new(
            OrchestratorEngine::new(
                Arc::clone(&acp),
                data.path().to_path_buf(),
                None,
                Handle::current(),
            )
            .with_relay(Arc::clone(&relay)),
        );
        wake_sink.attach(&engine);
        Self {
            engine,
            acp,
            relay,
            caller: CallerRef {
                agent: AgentId(PARENT_AGENT.to_string()),
                session: SessionId(CHAT.to_string()),
            },
            _data: data,
        }
    }

    fn seed_run(
        &self,
        title: &str,
        agent_name: &str,
        captured_result: Option<&str>,
        workspace: SubagentWorkspace,
    ) -> RunId {
        let run = RunId::mint();
        let capture = Arc::new(RunCapture::default());
        if let Some(text) = captured_result {
            capture.note_chunk(text);
        }
        let entry = ChildRun {
            run: run.clone(),
            root: CHAT.to_string(),
            parent_agent: AgentId(PARENT_AGENT.to_string()),
            parent_session_id: CHAT.to_string(),
            parent_run: None,
            depth: 1,
            identity: SubagentAgent {
                name: agent_name.to_string(),
                config_id: None,
                agent_id: format!("agent-child-{}", run.as_str()),
            },
            profile_id: "mock-sub".to_string(),
            execution_policy: None,
            child_cwd: "/tmp".to_string(),
            session_id: Some(SessionId(format!("child-sess-{}", run.as_str()))),
            title: title.to_string(),
            task: "do the work".to_string(),
            model: None,
            created_ms: 1,
            workspace,
            base_commit: None,
            body: RunBody::Live {
                child_agent: AgentId(format!("agent-child-{}", run.as_str())),
                session: SessionId(format!("child-sess-{}", run.as_str())),
                awaiting_permission: false,
            },
            stop_origin: None,
            result: None,
            waiters: Vec::new(),
            capture,
            queued_prompts: std::collections::VecDeque::new(),
        };
        self.engine.ledger.lock().insert(entry);
        run
    }

    fn shared_workspace() -> SubagentWorkspace {
        SubagentWorkspace {
            mode: SubagentWorkspaceMode::Shared,
            path: None,
            branch: None,
            base: None,
            diff_stat: None,
        }
    }

    /// Return all recorded wake `user_prompt` events (`(turnId, text)`) on `CHAT`.
    async fn recorded_wakes(&self) -> Vec<(String, String)> {
        let cancel = CancellationToken::new();
        let (client_id, mut stream, replay, _) =
            self.relay.subscribe(CHAT, Some(0), None, &cancel).await;
        if replay == ReplayResult::NotFound {
            return Vec::new();
        }
        let mut wakes = Vec::new();
        while let Ok(item) = stream.try_recv() {
            if let StreamItem::Event(event) = item {
                if event.type_ == "user_prompt" {
                    let turn_id = event
                        .payload
                        .get("turnId")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if turn_id.starts_with(WAKE_TURN_PREFIX) {
                        let text = event.payload["content"][0]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        wakes.push((turn_id, text));
                    }
                }
            }
        }
        self.relay.unsubscribe(CHAT, client_id);
        wakes
    }

    /// Poll until `recorded_wakes().len() >= expected` or fail after 3s.
    async fn wait_for_wakes(&self, expected: usize) -> Vec<(String, String)> {
        for _ in 0..300 {
            let wakes = self.recorded_wakes().await;
            if wakes.len() >= expected {
                return wakes;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "timed out waiting for {expected} wake(s); got {}",
            self.recorded_wakes().await.len()
        );
    }

    /// Poll until `prompt_complete` count for wake turns reaches `expected`.
    async fn wait_for_wake_completions(&self, expected: usize) {
        for _ in 0..300 {
            let cancel = CancellationToken::new();
            let (client_id, mut stream, replay, _) =
                self.relay.subscribe(CHAT, Some(0), None, &cancel).await;
            if replay != ReplayResult::NotFound {
                let mut count = 0;
                while let Ok(item) = stream.try_recv() {
                    if let StreamItem::Event(event) = item {
                        if event.type_ == "prompt_complete"
                            && event
                                .payload
                                .get("turnId")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|id| id.starts_with(WAKE_TURN_PREFIX))
                        {
                            count += 1;
                        }
                    }
                }
                self.relay.unsubscribe(CHAT, client_id);
                if count >= expected {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {expected} wake completion(s)");
    }

    /// Start a user prompt turn on `CHAT`, returning a handle to its completion task.
    async fn start_user_prompt(
        &self,
        turn_id: &str,
        text: &str,
    ) -> tokio::task::JoinHandle<Result<agent_client_protocol::schema::v1::StopReason, String>>
    {
        let blocks = vec![ContentBlock::Text(TextContent::new(text))];
        let request = PromptTurnRequest {
            agent_id: AgentId(PARENT_AGENT.to_string()),
            session_id: SessionId(CHAT.to_string()),
            content: blocks.clone(),
            record_content: blocks,
            turn_id: Some(turn_id.to_string()),
        };
        let PromptAdmission::Running(running) = accept_prompt_turn(&self.acp, &self.relay, request)
            .await
            .expect("user prompt accepted")
        else {
            panic!("expected running prompt admission");
        };
        let acp = Arc::clone(&self.acp);
        tokio::spawn(async move { complete_prompt_turn(&acp, running).await })
    }
}

fn hold_turn() -> (FakeTurn, oneshot::Sender<()>, oneshot::Receiver<()>) {
    let (release_tx, release_rx) = oneshot::channel();
    let (entered_tx, entered_rx) = oneshot::channel();
    (
        FakeTurn::Hold {
            release: release_rx,
            entered: entered_tx,
        },
        release_tx,
        entered_rx,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_parent_gets_exactly_one_wake_with_result_text() {
    let h = WakeHarness::with_turns(vec![]);
    let run = h.seed_run(
        "Test Runner",
        "Codex",
        Some("all 42 tests passed"),
        WakeHarness::shared_workspace(),
    );

    h.engine.finish(&run, RunEnd::Completed).await;
    // Duplicate settle is a no-op and produces no second wake.
    h.engine.finish(&run, RunEnd::Completed).await;

    let wakes = h.wait_for_wakes(1).await;
    h.wait_for_wake_completions(1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let wakes_after = h.recorded_wakes().await;
    assert_eq!(wakes_after.len(), 1, "idle parent gets exactly one wake");
    let (turn_id, body) = &wakes[0];
    assert!(turn_id.starts_with(WAKE_TURN_PREFIX));
    assert!(body.starts_with(WAKE_PROMPT_HEADER), "body: {body}");
    assert!(body.contains("Test Runner"), "missing title: {body}");
    assert!(body.contains(run.as_str()), "missing run id: {body}");
    assert!(body.contains("Codex"), "missing agent name: {body}");
    assert!(body.contains("completed"), "missing state: {body}");
    assert!(
        body.contains("all 42 tests passed"),
        "missing result text: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn busy_parent_gets_wake_only_after_prompt_complete() {
    let (held, release, entered) = hold_turn();
    let h = WakeHarness::with_turns(vec![held]);

    let user_turn = h.start_user_prompt("user-turn-1", "start a worker").await;
    entered.await.expect("parent turn active");

    let run = h.seed_run(
        "Linter",
        "Gemini",
        Some("zero lint errors"),
        WakeHarness::shared_workspace(),
    );
    h.engine.finish(&run, RunEnd::Completed).await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert!(
        h.recorded_wakes().await.is_empty(),
        "must not wake while parent turn is active"
    );
    assert_eq!(h.engine.wake.pending_count(CHAT), 1);

    // End the active parent turn: prompt_complete flushes the queued wake.
    let _ = release.send(());
    user_turn.await.expect("join").expect("turn ok");

    let wakes = h.wait_for_wakes(1).await;
    assert_eq!(wakes.len(), 1);
    assert!(wakes[0].1.contains("zero lint errors"));
    assert!(wakes[0].1.contains(run.as_str()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_settles_while_busy_go_out_as_one_batched_wake() {
    let (held, release, entered) = hold_turn();
    let h = WakeHarness::with_turns(vec![held]);

    let user_turn = h.start_user_prompt("user-turn-batch", "fan out").await;
    entered.await.expect("parent turn active");

    let run_a = h.seed_run(
        "Worker A",
        "Codex",
        Some("alpha result"),
        WakeHarness::shared_workspace(),
    );
    let run_b = h.seed_run(
        "Worker B",
        "Claude",
        Some("beta result"),
        WakeHarness::shared_workspace(),
    );

    h.engine.finish(&run_a, RunEnd::Completed).await;
    h.engine
        .finish(&run_b, RunEnd::Failed("compile error".to_string()))
        .await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert!(h.recorded_wakes().await.is_empty());
    assert_eq!(h.engine.wake.pending_count(CHAT), 2);

    let _ = release.send(());
    user_turn.await.expect("join").expect("turn ok");

    let wakes = h.wait_for_wakes(1).await;
    h.wait_for_wake_completions(1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let wakes_after = h.recorded_wakes().await;
    assert_eq!(
        wakes_after.len(),
        1,
        "both settled notices must batch into ONE wake turn"
    );
    let body = &wakes[0].1;
    let pos_a = body.find("alpha result").expect("first notice present");
    let pos_b = body.find("compile error").expect("second notice present");
    assert!(pos_a < pos_b, "arrival order must be preserved");
    assert!(body.contains(run_a.as_str()));
    assert!(body.contains(run_b.as_str()));
    assert!(body.contains("Worker A"));
    assert!(body.contains("Worker B"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parent_stop_and_user_stop_produce_no_wake_but_external_cancel_wakes() {
    let h = WakeHarness::with_turns(vec![]);

    // 1. Parent calls subagent_stop (including backstop kill settling as Cancelled).
    let parent_stopped = h.seed_run(
        "Stopped By Parent",
        "Codex",
        Some("partial"),
        WakeHarness::shared_workspace(),
    );
    assert_eq!(
        h.engine
            .stop(h.caller.clone(), parent_stopped.clone())
            .await
            .expect("stop"),
        crate::acp::orchestrator::StopOutcome::Sent
    );
    h.engine.finish(&parent_stopped, RunEnd::Cancelled).await;

    // 2. User clicks Stop in the UI.
    let user_stopped = h.seed_run(
        "Stopped By User",
        "Codex",
        Some("partial"),
        WakeHarness::shared_workspace(),
    );
    assert!(h
        .engine
        .stop_from_ui(CHAT, user_stopped.as_str())
        .await
        .is_some_and(|res| res.is_ok()));
    h.engine.finish(&user_stopped, RunEnd::Cancelled).await;

    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        h.recorded_wakes().await.is_empty(),
        "parent stop and user stop must produce no wake"
    );

    // 3. External cancel (no stop_origin recorded) DOES produce a wake.
    let external_cancelled = h.seed_run(
        "External Cancel",
        "Gemini",
        None,
        WakeHarness::shared_workspace(),
    );
    h.engine
        .finish(&external_cancelled, RunEnd::Cancelled)
        .await;

    let wakes = h.wait_for_wakes(1).await;
    assert_eq!(wakes.len(), 1);
    assert!(wakes[0].1.contains("External Cancel"));
    assert!(wakes[0].1.contains("cancelled"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subagent_wait_and_status_suppress_pending_wake() {
    let (held, release, entered) = hold_turn();
    let h = WakeHarness::with_turns(vec![held]);

    let user_turn = h.start_user_prompt("user-turn-ack", "check workers").await;
    entered.await.expect("parent turn active");

    let waited_run = h.seed_run(
        "Waited Run",
        "Codex",
        Some("already read by wait"),
        WakeHarness::shared_workspace(),
    );
    let status_run = h.seed_run(
        "Status Run",
        "Claude",
        Some("already read by status"),
        WakeHarness::shared_workspace(),
    );
    let unread_run = h.seed_run(
        "Unread Run",
        "Gemini",
        Some("needs wake"),
        WakeHarness::shared_workspace(),
    );

    h.engine.finish(&waited_run, RunEnd::Completed).await;
    h.engine.finish(&status_run, RunEnd::Completed).await;
    h.engine.finish(&unread_run, RunEnd::Completed).await;
    assert_eq!(h.engine.wake.pending_count(CHAT), 3);

    // Parent reads waited_run through subagent_wait -> drops its pending notice.
    let wait_outcome = h
        .engine
        .wait(h.caller.clone(), waited_run.clone(), Duration::from_secs(1))
        .await
        .expect("wait");
    assert!(matches!(
        wait_outcome,
        crate::acp::orchestrator::WaitOutcome::Settled(_)
    ));

    // Parent reads status_run through subagent_status -> drops its pending notice.
    let statuses = h
        .engine
        .status(h.caller.clone(), Some(vec![status_run.clone()]))
        .expect("status");
    assert_eq!(statuses[0].state, RunState::Completed);

    assert_eq!(
        h.engine.wake.pending_count(CHAT),
        1,
        "only the unread run should remain pending"
    );

    let _ = release.send(());
    user_turn.await.expect("join").expect("turn ok");

    let wakes = h.wait_for_wakes(1).await;
    h.wait_for_wake_completions(1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let wakes_after = h.recorded_wakes().await;
    assert_eq!(wakes_after.len(), 1);
    assert!(wakes[0].1.contains("needs wake"));
    assert!(!wakes[0].1.contains("already read by wait"));
    assert!(!wakes[0].1.contains("already read by status"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_run_does_not_wake_root() {
    let h = WakeHarness::with_turns(vec![]);
    let run = h.seed_run(
        "Nested",
        "Codex",
        Some("nested result"),
        WakeHarness::shared_workspace(),
    );
    h.engine
        .ledger
        .lock()
        .runs
        .get_mut(&run)
        .unwrap()
        .parent_run = Some(RunId("parent-run".into()));
    h.engine.finish(&run, RunEnd::Completed).await;
    assert_eq!(h.engine.wake.pending_count(CHAT), 0);
    assert!(h.recorded_wakes().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loop_guard_stops_at_30_consecutive_wakes_and_resets_on_user_prompt() {
    let h = WakeHarness::with_turns(vec![]);

    // Deliver MAX_WAKE_STREAK (30) consecutive autonomous wakes.
    for index in 1..=MAX_WAKE_STREAK {
        let run = h.seed_run(
            &format!("Run {index}"),
            "Codex",
            Some(&format!("result-{index}")),
            WakeHarness::shared_workspace(),
        );
        h.engine.finish(&run, RunEnd::Completed).await;
        h.wait_for_wake_completions(index as usize).await;
    }
    assert_eq!(h.engine.wake.streak(CHAT), MAX_WAKE_STREAK);

    // The 31st settle is held pending; no 31st wake fires.
    let blocked_run = h.seed_run(
        "Run 31",
        "Codex",
        Some("held-by-guard"),
        WakeHarness::shared_workspace(),
    );
    h.engine.finish(&blocked_run, RunEnd::Completed).await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert_eq!(
        h.recorded_wakes().await.len(),
        MAX_WAKE_STREAK as usize,
        "streak guard must stop waking at 30"
    );
    assert_eq!(h.engine.wake.pending_count(CHAT), 1);

    // User sends a prompt -> streak resets and held notice delivers after its prompt_complete.
    let user_turn = h.start_user_prompt("user-reset-1", "continue").await;
    user_turn.await.expect("join").expect("turn ok");

    let wakes = h.wait_for_wakes((MAX_WAKE_STREAK + 1) as usize).await;
    assert_eq!(wakes.len(), (MAX_WAKE_STREAK + 1) as usize);
    assert!(wakes.last().expect("31st wake").1.contains("held-by-guard"));
    assert_eq!(h.engine.wake.streak(CHAT), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_close_disposes_mailbox_and_suppresses_wakes() {
    let (held, _release, entered) = hold_turn();
    let h = WakeHarness::with_turns(vec![held]);

    let _user_turn = h.start_user_prompt("user-turn-close", "work").await;
    entered.await.expect("parent turn active");

    let settled_while_busy = h.seed_run(
        "Queued Run",
        "Codex",
        Some("pending notice"),
        WakeHarness::shared_workspace(),
    );
    let live_run = h.seed_run(
        "Live Run",
        "Codex",
        Some("live notice"),
        WakeHarness::shared_workspace(),
    );

    h.engine
        .finish(&settled_while_busy, RunEnd::Completed)
        .await;
    assert!(h.engine.wake.has_mailbox(CHAT));
    assert_eq!(h.engine.wake.pending_count(CHAT), 1);

    h.engine.chat_closed(CHAT).await;
    assert!(
        !h.engine.wake.has_mailbox(CHAT),
        "chat close must dispose the root mailbox"
    );

    // Finishing the run that was stopped by chat_closed must not recreate or wake.
    h.engine.finish(&live_run, RunEnd::Cancelled).await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(!h.engine.wake.has_mailbox(CHAT));
    assert!(h.recorded_wakes().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_agent_gone_produces_no_wake() {
    let h = WakeHarness::with_turns(vec![]);
    h.seed_run(
        "Orphaned",
        "Codex",
        Some("never delivered"),
        WakeHarness::shared_workspace(),
    );
    let cards = h.engine.on_agent_gone(&AgentId(PARENT_AGENT.to_string()));
    assert_eq!(cards.len(), 1);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(h.recorded_wakes().await.is_empty());
    assert!(!h.engine.wake.has_mailbox(CHAT));
}

#[test]
fn format_wake_prompt_truncates_utf8_safely_and_includes_workspace_metadata() {
    // 3-byte UTF-8 character repeated so byte 16_000 lands inside a character.
    let long_result = "€".repeat(6_000);
    assert!(long_result.len() > MAX_INLINE_RESULT_BYTES);
    let (cut, truncated) = truncate_utf8_bytes(&long_result, MAX_INLINE_RESULT_BYTES);
    assert!(truncated);
    assert!(cut.len() <= MAX_INLINE_RESULT_BYTES);
    assert_eq!(cut.len(), 15_999);
    assert!(cut.chars().all(|ch| ch == '€'));

    let notice = WakeNotice::Settled {
        run: WakeRun {
            run_id: RunId("sub-abcdef012345".to_string()),
            title: "Patch Builder".to_string(),
            agent_name: "Codex".to_string(),
        },
        state: RunState::Completed,
        result: Some(long_result),
        error: None,
        workspace: Some(SubagentWorkspace {
            mode: SubagentWorkspaceMode::Worktree,
            path: Some("/repo/.termul/worktrees/sub-abcdef012345".to_string()),
            branch: Some("troupe/sub/sub-abcdef012345".to_string()),
            base: Some("dev @ 1234567".to_string()),
            diff_stat: Some("2 files changed, +15 -3".to_string()),
        }),
    };
    let prompt = format_wake_prompt(&[notice]);
    assert_eq!(prompt.lines().next(), Some(WAKE_PROMPT_HEADER));
    assert!(prompt
        .contains("Patch Builder (subagentId: sub-abcdef012345, agent: Codex, state: completed)"));
    assert!(prompt.contains("Workspace path: /repo/.termul/worktrees/sub-abcdef012345"));
    assert!(prompt.contains("Workspace branch: troupe/sub/sub-abcdef012345"));
    assert!(prompt.contains("Workspace diff: 2 files changed, +15 -3"));
    assert!(prompt
        .contains("call subagent_wait with subagentId \"sub-abcdef012345\" for the full result"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resumed_run_wakes_the_parent_again_when_it_settles() {
    let h = WakeHarness::with_turns(vec![]);
    let run = h.seed_run(
        "Resumed Run",
        "Codex",
        Some("first result"),
        WakeHarness::shared_workspace(),
    );

    h.engine.finish(&run, RunEnd::Completed).await;
    h.wait_for_wakes(1).await;
    h.wait_for_wake_completions(1).await;
    h.engine
        .status(h.caller.clone(), Some(vec![run.clone()]))
        .expect("status reads the first result");

    {
        let mut ledger = h.engine.ledger.lock();
        let entry = ledger.runs.get_mut(&run).expect("run kept");
        entry.body = RunBody::Live {
            child_agent: AgentId(format!("agent-child-{}", run.as_str())),
            session: SessionId(format!("child-sess-{}", run.as_str())),
            awaiting_permission: false,
        };
        entry.result = None;
    }
    h.engine.wake.reopen_run(CHAT, &run);

    h.engine.finish(&run, RunEnd::Completed).await;
    let wakes = h.wait_for_wakes(2).await;
    assert_eq!(wakes.len(), 2, "the second settle after a resume wakes again");
    assert!(wakes[1].1.contains("Resumed Run"), "body: {}", wakes[1].1);
}

#[test]
fn retry_backs_off_to_a_cap_and_resets_on_an_accepted_prompt() {
    let wake = WakeRegistry::new();
    assert_eq!(wake.arm_retry(CHAT), None, "no mailbox, no retry");
    let notice = WakeNotice::Settled {
        run: WakeRun {
            run_id: RunId("run-retry".to_string()),
            title: "Retry".to_string(),
            agent_name: "Codex".to_string(),
        },
        state: RunState::Completed,
        result: Some("done".to_string()),
        error: None,
        workspace: None,
    };
    assert!(wake.enqueue(CHAT, notice));

    let mut delays = Vec::new();
    for _ in 0..6 {
        delays.push(wake.arm_retry(CHAT).expect("retry armed").as_secs());
        assert_eq!(wake.arm_retry(CHAT), None, "one timer at a time");
        wake.disarm_retry(CHAT);
    }
    assert_eq!(delays, vec![5, 10, 20, 40, 60, 60]);

    for _ in 6..MAX_RETRIES {
        assert!(wake.arm_retry(CHAT).is_some());
        wake.disarm_retry(CHAT);
    }
    assert_eq!(wake.arm_retry(CHAT), None, "retries exhausted");

    wake.on_prompt_accepted(CHAT, Some("user-turn"));
    assert_eq!(wake.arm_retry(CHAT).map(|d| d.as_secs()), Some(5));
}

fn child_caller(run: &RunId) -> CallerRef {
    CallerRef {
        agent: AgentId(format!("agent-child-{}", run.as_str())),
        session: SessionId(format!("child-sess-{}", run.as_str())),
    }
}

/// One frame down the host MCP TCP listener, its raw reply line back
/// (mirrors `host_mcp::parent::tests::connect_and_send`). The reply stays a
/// string: serde serializes compactly with the struct's field order, so the
/// literal wire text is the assertion.
async fn send_host_frame(port: u16, frame: serde_json::Value) -> String {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("host listener");
    let mut buf = serde_json::to_vec(&frame).expect("frame json");
    buf.push(b'\n');
    stream.write_all(&buf).await.expect("write frame");
    let mut reader = tokio::io::BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.expect("reply line");
    line
}

fn ask_frame(token: &str, provisional: &str, question: &str) -> serde_json::Value {
    serde_json::json!({
        "token": token,
        "session_id": provisional,
        "kind": "ask_user",
        "ask": {
            "question": question,
            "options": [{"label": "Yes"}, {"label": "No"}],
            "multi_select": false,
        },
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn message_parent_wakes_the_idle_parent_with_the_text() {
    let h = WakeHarness::with_turns(vec![]);
    let run = h.seed_run(
        "Researcher",
        "Codex",
        None,
        WakeHarness::shared_workspace(),
    );

    h.engine
        .message_parent(
            child_caller(&run),
            "halfway done, 3 files patched".to_string(),
        )
        .expect("message accepted");

    let wakes = h.wait_for_wakes(1).await;
    let body = &wakes[0].1;
    assert!(
        body.contains(&format!(
            "Researcher (subagentId: {}, agent: Codex) says:\nhalfway done, 3 files patched",
            run.as_str()
        )),
        "message wake shape: {body}"
    );
    assert!(
        body.contains(
            "Reply with subagent_send."
        ),
        "reply hint: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn message_parent_is_refused_for_a_plain_session_and_a_nested_run() {
    let h = WakeHarness::with_turns(vec![]);
    let outsider = CallerRef {
        agent: AgentId("agent-random".to_string()),
        session: SessionId("chat-elsewhere".to_string()),
    };
    assert_eq!(
        h.engine.message_parent(outsider, "hi".to_string()),
        Err(MessageParentError::NotASubagent)
    );

    let run = h.seed_run("Nested", "Codex", None, WakeHarness::shared_workspace());
    h.engine
        .ledger
        .lock()
        .runs
        .get_mut(&run)
        .unwrap()
        .depth = 2;
    assert_eq!(
        h.engine.message_parent(child_caller(&run), "hi".to_string()),
        Err(MessageParentError::Nested)
    );
    assert!(
        h.recorded_wakes().await.is_empty(),
        "a refused message produces no wake"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_and_a_settle_while_busy_go_out_as_one_wake() {
    let (held, release, entered) = hold_turn();
    let h = WakeHarness::with_turns(vec![held]);

    let user_turn = h.start_user_prompt("user-turn-msg", "work").await;
    entered.await.expect("parent turn active");

    let run = h.seed_run(
        "Worker",
        "Codex",
        Some("settled result"),
        WakeHarness::shared_workspace(),
    );
    h.engine
        .message_parent(child_caller(&run), "almost there".to_string())
        .expect("message accepted");
    h.engine.finish(&run, RunEnd::Completed).await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert!(h.recorded_wakes().await.is_empty());
    assert_eq!(h.engine.wake.pending_count(CHAT), 2);

    let _ = release.send(());
    user_turn.await.expect("join").expect("turn ok");

    let wakes = h.wait_for_wakes(1).await;
    h.wait_for_wake_completions(1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        h.recorded_wakes().await.len(),
        1,
        "message and settle batch into ONE wake turn"
    );
    let body = &wakes[0].1;
    let pos_message = body.find("almost there").expect("message present");
    let pos_settle = body.find("settled result").expect("settle present");
    assert!(pos_message < pos_settle, "arrival order is preserved");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_child_question_wakes_the_parent_with_the_question_id() {
    let h = WakeHarness::with_turns(vec![]);
    let run = h.seed_run("Asker", "Gemini", None, WakeHarness::shared_workspace());

    let root = h.engine.enqueue_question_wake(
        child_caller(&run),
        "q-test-1".to_string(),
        "Ship it?".to_string(),
        vec!["Yes".to_string(), "No".to_string()],
    );
    assert_eq!(root.as_deref(), Some(CHAT));

    let wakes = h.wait_for_wakes(1).await;
    let body = &wakes[0].1;
    assert!(
        body.contains(&format!(
            "Asker (subagentId: {}, agent: Gemini) asks (questionId: q-test-1):\nShip it?",
            run.as_str()
        )),
        "question wake shape: {body}"
    );
    assert!(body.contains("\n- Yes"), "option label: {body}");
    assert!(body.contains("\n- No"), "option label: {body}");
    assert!(
        body.contains("You may answer it with subagent_answer, or leave it for the user."),
        "answer hint: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_question_answered_before_the_wake_drops_its_notice() {
    let (held, release, entered) = hold_turn();
    let h = WakeHarness::with_turns(vec![held]);
    let server = crate::acp::host_mcp::parent::HostPlanServer::start(vec![], None);
    server.set_orchestrator(Arc::clone(&h.engine));

    let run = h.seed_run("Asker", "Gemini", None, WakeHarness::shared_workspace());
    let child_agent = format!("agent-child-{}", run.as_str());
    let child_session = format!("child-sess-{}", run.as_str());
    let (port, token, provisional) = server.register_session(&child_agent);
    server.bind_session(&token, &child_session);
    server.begin_turn(&child_agent, &child_session);

    let user_turn = h.start_user_prompt("user-turn-q", "work").await;
    entered.await.expect("parent turn active");

    let frame = ask_frame(&token, &provisional, "Ship it?");
    let call = tokio::spawn(async move { send_host_frame(port, frame).await });

    let mut question_id = None;
    for _ in 0..300 {
        if let Some(id) = server.open_ask_ids().into_iter().next() {
            question_id = Some(id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let question_id = question_id.expect("ask parked");
    for _ in 0..300 {
        if h.engine.wake.pending_count(CHAT) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        h.engine.wake.pending_count(CHAT),
        1,
        "the question notice waits while the parent is busy"
    );

    // The user answers before the wake went out: its notice is dropped.
    assert!(server.answer_ask(&question_id, Some(vec!["Yes".to_string()])));
    assert_eq!(
        h.engine.wake.pending_count(CHAT),
        0,
        "the answered question's pending notice is dropped"
    );

    let reply = call.await.expect("ask call joined");
    assert!(
        reply.contains("\"answers\":[\"Yes\"]"),
        "the user's answer resolves the parked call: {reply}"
    );

    let _ = release.send(());
    user_turn.await.expect("join").expect("turn ok");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        h.recorded_wakes().await.is_empty(),
        "no wake reports a question the user already answered"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn child_ask_user_wakes_parent_and_subagent_answer_resolves_the_call() {
    let h = WakeHarness::with_turns(vec![]);
    let server = crate::acp::host_mcp::parent::HostPlanServer::start(vec![], None);
    server.set_orchestrator(Arc::clone(&h.engine));

    let run = h.seed_run("Asker", "Gemini", None, WakeHarness::shared_workspace());
    let child_agent = format!("agent-child-{}", run.as_str());
    let child_session = format!("child-sess-{}", run.as_str());
    let (port, token, provisional) = server.register_session(&child_agent);
    server.bind_session(&token, &child_session);
    server.begin_turn(&child_agent, &child_session);

    let frame = ask_frame(&token, &provisional, "Ship it?");
    let call = tokio::spawn(async move { send_host_frame(port, frame).await });

    let wakes = h.wait_for_wakes(1).await;
    let body = wakes[0].1.clone();
    assert!(body.contains("Ship it?"), "question text: {body}");
    let marker = "asks (questionId: ";
    let start = body.find(marker).expect("question id in the wake") + marker.len();
    let end = body[start..].find(')').expect("question id end") + start;
    let question_id = body[start..end].to_string();
    assert!(question_id.starts_with("q-"), "{question_id}");

    let stranger = server.answer_subagent_question(
        &h.engine,
        CallerRef {
            agent: AgentId(PARENT_AGENT.to_string()),
            session: SessionId("another-chat".to_string()),
        },
        crate::acp::host_mcp::subagent_tools::SubagentAnswerInput {
            subagent_id: run.as_str().to_string(),
            question_id: question_id.clone(),
            answers: vec!["No".to_string()],
        },
    );
    assert_eq!(
        stranger.code.as_deref(),
        Some("not_found"),
        "another chat cannot answer an open question it knows the id of"
    );

    let reply = server.answer_subagent_question(
        &h.engine,
        h.caller.clone(),
        crate::acp::host_mcp::subagent_tools::SubagentAnswerInput {
            subagent_id: run.as_str().to_string(),
            question_id: question_id.clone(),
            answers: vec!["Yes".to_string()],
        },
    );
    assert!(reply.ok, "subagent_answer accepted: {reply:?}");
    assert_eq!(reply.result.as_ref().unwrap()["outcome"], "answered");

    let ask_reply = call.await.expect("ask call joined");
    assert!(ask_reply.contains("\"ok\":true"), "wire ok: {ask_reply}");
    assert!(
        ask_reply.contains("\"answers\":[\"Yes\"]"),
        "the orchestrator's answer resolves the parked call: {ask_reply}"
    );
    assert!(
        ask_reply.contains("\"cancelled\":false"),
        "answered, not cancelled: {ask_reply}"
    );

    // First answer wins: the orchestrator's second answer is not_found.
    let second = server.answer_subagent_question(
        &h.engine,
        h.caller.clone(),
        crate::acp::host_mcp::subagent_tools::SubagentAnswerInput {
            subagent_id: run.as_str().to_string(),
            question_id,
            answers: vec!["No".to_string()],
        },
    );
    assert!(!second.ok);
    assert_eq!(second.code.as_deref(), Some("not_found"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subagent_answer_for_another_chats_run_is_refused() {
    let h = WakeHarness::with_turns(vec![]);
    let server = crate::acp::host_mcp::parent::HostPlanServer::start(vec![], None);
    server.set_orchestrator(Arc::clone(&h.engine));
    let run = h.seed_run("Asker", "Gemini", None, WakeHarness::shared_workspace());

    let stranger = CallerRef {
        agent: AgentId(PARENT_AGENT.to_string()),
        session: SessionId("another-chat".to_string()),
    };
    let reply = server.answer_subagent_question(
        &h.engine,
        stranger,
        crate::acp::host_mcp::subagent_tools::SubagentAnswerInput {
            subagent_id: run.as_str().to_string(),
            question_id: "q-whatever".to_string(),
            answers: vec!["Yes".to_string()],
        },
    );
    assert!(!reply.ok);
    assert_eq!(reply.code.as_deref(), Some("not_found"));

    // The right chat but an unknown question is not_found too.
    let reply = server.answer_subagent_question(
        &h.engine,
        h.caller.clone(),
        crate::acp::host_mcp::subagent_tools::SubagentAnswerInput {
            subagent_id: run.as_str().to_string(),
            question_id: "q-unknown".to_string(),
            answers: vec!["Yes".to_string()],
        },
    );
    assert!(!reply.ok);
    assert_eq!(reply.code.as_deref(), Some("not_found"));
}

#[test]
fn acknowledgement_drops_only_settled_notices_and_messages_never_dedupe() {
    let wake = WakeRegistry::new();
    let run_id = RunId("run-x".to_string());
    let wake_run = | wake_run_id: &RunId | WakeRun {
        run_id: wake_run_id.clone(),
        title: "T".to_string(),
        agent_name: "A".to_string(),
    };
    assert!(wake.enqueue(
        CHAT,
        WakeNotice::Message {
            run: wake_run(&run_id),
            message: "one".to_string(),
        }
    ));
    assert!(wake.enqueue(
        CHAT,
        WakeNotice::Message {
            run: wake_run(&run_id),
            message: "two".to_string(),
        }
    ));
    assert_eq!(wake.pending_count(CHAT), 2, "messages never dedupe per run");

    wake.acknowledge(CHAT, &run_id);
    assert_eq!(
        wake.pending_count(CHAT),
        2,
        "reading the run's result does not acknowledge its messages"
    );
}
