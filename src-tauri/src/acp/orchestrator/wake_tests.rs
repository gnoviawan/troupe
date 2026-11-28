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
    CallerRef, ChildRun, RunBody, RunCapture, RunEnd, RunId, RunState,
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
            parent_run: None,
            depth: 1,
            identity: SubagentAgent {
                name: agent_name.to_string(),
                config_id: None,
                agent_id: format!("agent-child-{}", run.as_str()),
            },
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

    let notice = WakeNotice {
        run_id: RunId("sub-abcdef012345".to_string()),
        title: "Patch Builder".to_string(),
        agent_name: "Codex".to_string(),
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

#[test]
fn retry_backs_off_to_a_cap_and_resets_on_an_accepted_prompt() {
    let wake = WakeRegistry::new();
    assert_eq!(wake.arm_retry(CHAT), None, "no mailbox, no retry");
    let notice = WakeNotice {
        run_id: RunId("run-retry".to_string()),
        title: "Retry".to_string(),
        agent_name: "Codex".to_string(),
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
