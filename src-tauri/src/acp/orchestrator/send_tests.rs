//! End-to-end and unit tests for `subagent_send`: queueing while working,
//! interrupting an active turn, resuming completed/cancelled runs via
//! `session/load`, capacity accounting, ownership refusal, failed runs,
//! unloadable sessions, and input limits.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;
use tokio::runtime::Handle;

use crate::acp::config::{AgentConfig, AgentId, SessionId};
use crate::acp::orchestrator::engine::OrchestratorEngine;
use crate::acp::orchestrator::run::{
    CallerRef, ChildRun, RunBody, RunCapture, RunEnd, RunId, RunState, SendError, SendRequest,
    StartRequest,
};
use crate::acp::orchestrator::settings::OrchestratorSettings;
use crate::acp::subagents::SubagentWorkspaceMode;
use crate::web::EventSink;

const CHAT: &str = "chat-send-1";

/// Scripted ACP child supporting `session/new`, `session/load` (with optional
/// capability/failure env toggles), history replay during load, gate-file
/// synchronization for queue-while-working, and `session/cancel` for interrupt.
const MOCK_RESUMABLE_CHILD: &str = r#"
import json, os, sys, time

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def say(sid, text):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {
        "sessionId": sid,
        "update": {"sessionUpdate": "agent_message_chunk",
                   "content": {"type": "text", "text": text}}}})

state_file = os.environ.get("MOCK_STATE_FILE", "")
can_load = os.environ.get("MOCK_LOAD_CAP", "1") == "1"
fail_load = os.environ.get("MOCK_FAIL_LOAD", "0") == "1"
gate_file = os.environ.get("MOCK_GATE_FILE", "")

def read_turns():
    if not state_file or not os.path.exists(state_file):
        return 0
    with open(state_file, "r") as f:
        return json.load(f).get("turns", 0)

def write_turns(n):
    if not state_file:
        return
    with open(state_file, "w") as f:
        json.dump({"turns": n}, f)

loaded = False
hung = None
sid = "child-resumable-1"

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, rid = msg.get("method"), msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": 1,
            "agentCapabilities": {"loadSession": can_load},
            "authMethods": []}})
    elif method == "session/new":
        write_turns(0)
        send({"jsonrpc": "2.0", "id": rid, "result": {"sessionId": sid}})
    elif method == "session/load":
        if fail_load:
            send({"jsonrpc": "2.0", "id": rid, "error": {
                "code": -32002,
                "message": "session not found on disk"}})
        else:
            loaded = True
            # Replay an earlier message chunk to verify the host replay window
            # drops it and does not duplicate history in the chat.
            say(msg["params"]["sessionId"], "REPLAYED_OLD_CHUNK")
            send({"jsonrpc": "2.0", "id": rid, "result": {}})
    elif method == "session/prompt":
        task = msg["params"]["prompt"][0].get("text", "")
        turns = read_turns() + 1
        write_turns(turns)
        if task == "finish":
            say(sid, "turn1-done")
            send({"jsonrpc": "2.0", "id": rid, "result": {"stopReason": "end_turn"}})
        elif task == "gated":
            say(sid, "working-turn1")
            if gate_file:
                deadline = time.time() + 15.0
                while not os.path.exists(gate_file) and time.time() < deadline:
                    time.sleep(0.02)
            say(sid, " turn1-complete")
            send({"jsonrpc": "2.0", "id": rid, "result": {"stopReason": "end_turn"}})
        elif task == "hang":
            hung = rid
            say(sid, "hanging")
        else:
            say(sid, "reply:%s loaded=%s turns=%d" % (task, "true" if loaded else "false", turns))
            send({"jsonrpc": "2.0", "id": rid, "result": {"stopReason": "end_turn"}})
    elif method == "session/cancel":
        if hung is not None:
            send({"jsonrpc": "2.0", "id": hung, "result": {"stopReason": "cancelled"}})
            hung = None
"#;

#[derive(Default)]
struct EventLog(Mutex<Vec<crate::web::sink::AcpEvent>>);

impl EventSink for EventLog {
    fn emit(&self, event: &crate::web::sink::AcpEvent) {
        self.0.lock().push(event.clone());
    }
}

fn find_python() -> Option<&'static str> {
    crate::acp::manager::real_loop_tests::find_python()
}

struct SendHarness {
    engine: Arc<OrchestratorEngine>,
    caller: CallerRef,
    events: Arc<EventLog>,
    gate_path: std::path::PathBuf,
    _data: tempfile::TempDir,
    _cwd: tempfile::TempDir,
}

fn harness(load_cap: bool, fail_load: bool) -> Option<SendHarness> {
    let python = find_python()?;
    Some(harness_with_command(python, load_cap, fail_load))
}

fn harness_with_command(command: &str, load_cap: bool, fail_load: bool) -> SendHarness {
    let data = tempfile::tempdir().expect("data dir");
    let cwd = tempfile::tempdir().expect("cwd dir");
    let script = cwd.path().join("child.py");
    let state_file = cwd.path().join("state.json");
    let gate_path = cwd.path().join("release.gate");
    std::fs::write(&script, MOCK_RESUMABLE_CHILD).expect("script");
    let mut env = std::collections::HashMap::new();
    env.insert(
        "MOCK_STATE_FILE".to_string(),
        state_file.to_string_lossy().into_owned(),
    );
    env.insert(
        "MOCK_LOAD_CAP".to_string(),
        if load_cap { "1" } else { "0" }.to_string(),
    );
    env.insert(
        "MOCK_FAIL_LOAD".to_string(),
        if fail_load { "1" } else { "0" }.to_string(),
    );
    env.insert(
        "MOCK_GATE_FILE".to_string(),
        gate_path.to_string_lossy().into_owned(),
    );
    let agent = AgentConfig {
        config_id: Some("mock-sub".to_string()),
        name: "Mock Sub".to_string(),
        command: command.to_string(),
        args: vec!["-u".to_string(), script.to_string_lossy().into_owned()],
        env,
        allow_terminal: false,
    };
    std::fs::write(
        data.path().join(crate::automation::agents::FILE_NAME),
        json!({"schema_version": 1, "agents": {"mock-sub": agent}}).to_string(),
    )
    .expect("registry");
    let events = Arc::new(EventLog::default());
    let acp = Arc::new(crate::acp::manager::AcpManager::new(vec![events.clone()]));
    let parent = AgentId("agent-parent".to_string());
    acp.install_test_agent_with_session_roots(
        parent.clone(),
        vec![(CHAT.to_string(), cwd.path().to_path_buf(), false)],
    );
    let engine = Arc::new(OrchestratorEngine::new(
        acp,
        data.path().to_path_buf(),
        None,
        Handle::current(),
    ));
    SendHarness {
        engine,
        caller: CallerRef {
            agent: parent,
            session: SessionId(CHAT.to_string()),
        },
        events,
        gate_path,
        _data: data,
        _cwd: cwd,
    }
}

fn start_req(task: &str) -> StartRequest {
    StartRequest {
        agent: "mock-sub".to_string(),
        task: task.to_string(),
        workspace: SubagentWorkspaceMode::Shared,
        title: None,
        model: None,
    }
}

async fn wait_settled(
    engine: &Arc<OrchestratorEngine>,
    caller: &CallerRef,
    run: &RunId,
) -> crate::acp::orchestrator::RunSummary {
    let outcome = engine
        .wait(caller.clone(), run.clone(), Duration::from_secs(60))
        .await
        .expect("wait resolves");
    let crate::acp::orchestrator::WaitOutcome::Settled(summary) = outcome else {
        panic!("expected settled run");
    };
    *summary
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queue_while_working_delivers_followup_after_current_turn_ends() {
    let Some(test) = harness(true, false) else {
        return;
    };
    let _serial = crate::acp::manager::real_loop_tests::TIMEOUT_TEST_LOCK
        .lock()
        .await;
    let engine = test.engine.clone();
    let caller = test.caller.clone();

    let started = engine
        .start(caller.clone(), start_req("gated"))
        .await
        .expect("run starts");
    let run = started.run.clone();

    let sent = engine
        .send(
            caller.clone(),
            SendRequest {
                subagent_id: run.0.clone(),
                message: "add error handling".to_string(),
                interrupt: false,
            },
        )
        .await
        .expect("queued follow-up");
    assert_eq!(sent.run, run);
    assert_eq!(sent.state, RunState::Running);
    assert!(sent.queued, "send to a live run queues the message");

    // Release turn 1 so supervise delivers the queued follow-up on the same session.
    std::fs::write(&test.gate_path, "go").expect("release gate");
    let settled = wait_settled(&engine, &caller, &run).await;
    assert_eq!(settled.state, RunState::Completed);
    assert_eq!(
        settled.result.as_deref(),
        Some("reply:add error handling loaded=false turns=2")
    );

    let parent_msgs: Vec<_> = test
        .events
        .0
        .lock()
        .iter()
        .filter(|e| {
            e.type_ == crate::acp::events::EVENT_MESSAGE_CHUNK
                && e.payload["role"] == "user"
                && e.payload["parentToolCallId"] == run.dispatch_id()
        })
        .cloned()
        .collect();
    assert_eq!(parent_msgs.len(), 1);
    assert_eq!(
        parent_msgs[0].payload["content"]["text"],
        "add error handling"
    );
    assert!(
        parent_msgs[0].payload["sender"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "parent message carries a sender attribution"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupt_cancels_active_turn_and_delivers_followup() {
    let Some(test) = harness(true, false) else {
        return;
    };
    let _serial = crate::acp::manager::real_loop_tests::TIMEOUT_TEST_LOCK
        .lock()
        .await;
    let engine = test.engine.clone();
    let caller = test.caller.clone();

    let started = engine
        .start(caller.clone(), start_req("hang"))
        .await
        .expect("run starts");
    let run = started.run.clone();

    // Wait briefly until the child emits its first chunk so the prompt is active.
    for _ in 0..200 {
        let seen = test.events.0.lock().iter().any(|e| {
            e.type_ == crate::acp::events::EVENT_MESSAGE_CHUNK
                && e.payload["content"]["text"] == "hanging"
        });
        if seen {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let sent = engine
        .send(
            caller.clone(),
            SendRequest {
                subagent_id: run.0.clone(),
                message: "switch to plan B".to_string(),
                interrupt: true,
            },
        )
        .await
        .expect("interrupt send succeeds");
    assert_eq!(sent.state, RunState::Running);
    assert!(sent.queued);

    let settled = wait_settled(&engine, &caller, &run).await;
    assert_eq!(settled.state, RunState::Completed);
    assert_eq!(
        settled.result.as_deref(),
        Some("reply:switch to plan B loaded=false turns=2")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_after_complete_loads_kept_session_and_returns_to_running() {
    let Some(test) = harness(true, false) else {
        return;
    };
    let _serial = crate::acp::manager::real_loop_tests::TIMEOUT_TEST_LOCK
        .lock()
        .await;
    let engine = test.engine.clone();
    let caller = test.caller.clone();

    let started = engine
        .start(caller.clone(), start_req("finish"))
        .await
        .expect("run starts");
    let run = started.run.clone();
    let first = wait_settled(&engine, &caller, &run).await;
    assert_eq!(first.state, RunState::Completed);
    assert_eq!(first.result.as_deref(), Some("turn1-done"));

    let resumed = engine
        .send(
            caller.clone(),
            SendRequest {
                subagent_id: run.0.clone(),
                message: "fix review findings".to_string(),
                interrupt: false,
            },
        )
        .await
        .expect("resume succeeds");
    assert_eq!(resumed.run, run);
    assert_eq!(resumed.state, RunState::Running);
    assert!(!resumed.queued, "resuming an idle run starts immediately");

    let second = wait_settled(&engine, &caller, &run).await;
    assert_eq!(second.state, RunState::Completed);
    assert_eq!(
        second.result.as_deref(),
        Some("reply:fix review findings loaded=true turns=2")
    );

    let replayed = test.events.0.lock().iter().any(|e| {
        e.type_ == crate::acp::events::EVENT_MESSAGE_CHUNK
            && e.payload["content"]["text"] == "REPLAYED_OLD_CHUNK"
    });
    assert!(
        !replayed,
        "replayed chunks during session/load must be suppressed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_after_cancelled_reloads_session_and_counts_against_capacity() {
    let Some(test) = harness(true, false) else {
        return;
    };
    let _serial = crate::acp::manager::real_loop_tests::TIMEOUT_TEST_LOCK
        .lock()
        .await;
    let engine = test.engine.clone();
    let caller = test.caller.clone();
    engine
        .set_settings(OrchestratorSettings {
            max_running: 1,
            ..OrchestratorSettings::default()
        })
        .expect("max_running=1");

    let run_a = engine
        .start(caller.clone(), start_req("hang"))
        .await
        .expect("run A starts")
        .run;
    engine
        .stop(caller.clone(), run_a.clone())
        .await
        .expect("stop A");
    let cancelled = wait_settled(&engine, &caller, &run_a).await;
    assert_eq!(cancelled.state, RunState::Cancelled);

    // Occupy the single running slot with run B: resuming A must hit capacity.
    let run_b = engine
        .start(caller.clone(), start_req("hang"))
        .await
        .expect("run B starts")
        .run;
    let err = engine
        .send(
            caller.clone(),
            SendRequest {
                subagent_id: run_a.0.clone(),
                message: "continue".to_string(),
                interrupt: false,
            },
        )
        .await
        .expect_err("capacity must block resume");
    assert_eq!(err, SendError::Capacity { max: 1 });

    // Free the slot by stopping B; resuming A now succeeds.
    engine
        .stop(caller.clone(), run_b.clone())
        .await
        .expect("stop B");
    wait_settled(&engine, &caller, &run_b).await;

    let resumed = engine
        .send(
            caller.clone(),
            SendRequest {
                subagent_id: run_a.0.clone(),
                message: "continue now".to_string(),
                interrupt: false,
            },
        )
        .await
        .expect("resume after capacity freed");
    assert_eq!(resumed.state, RunState::Running);
    assert!(!resumed.queued);

    let done = wait_settled(&engine, &caller, &run_a).await;
    assert_eq!(done.state, RunState::Completed);
    assert_eq!(
        done.result.as_deref(),
        Some("reply:continue now loaded=true turns=2")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unloadable_session_refuses_and_preserves_completed_run() {
    for (load_cap, fail_load) in [(false, false), (true, true)] {
        let Some(test) = harness(load_cap, fail_load) else {
            return;
        };
        let _serial = crate::acp::manager::real_loop_tests::TIMEOUT_TEST_LOCK
            .lock()
            .await;
        let engine = test.engine.clone();
        let caller = test.caller.clone();

        let run = engine
            .start(caller.clone(), start_req("finish"))
            .await
            .expect("run starts")
            .run;
        let first = wait_settled(&engine, &caller, &run).await;
        assert_eq!(first.state, RunState::Completed);
        assert_eq!(first.result.as_deref(), Some("turn1-done"));

        let err = engine
            .send(
                caller.clone(),
                SendRequest {
                    subagent_id: run.0.clone(),
                    message: "follow up".to_string(),
                    interrupt: false,
                },
            )
            .await
            .expect_err("unloadable session must be refused");
        assert!(
            matches!(err, SendError::SessionUnloadable(_)),
            "expected SessionUnloadable for load_cap={load_cap} fail_load={fail_load}, got {err:?}"
        );

        let status = engine
            .status(caller.clone(), Some(vec![run.clone()]))
            .expect("status");
        assert_eq!(status[0].state, RunState::Completed);
        assert_eq!(status[0].result.as_deref(), Some("turn1-done"));
        assert_eq!(
            engine.ledger.lock().occupied_of_root(CHAT),
            0,
            "capacity slot must be released after failed resume"
        );
    }
}

#[tokio::test]
async fn ownership_failed_run_and_input_limits_are_enforced() {
    let test = harness_with_command("troupe-test-no-such-agent", true, false);
    let engine = test.engine.clone();
    let caller = test.caller.clone();

    let live_run = RunId("sub-owner111111".to_string());
    let failed_run = RunId("sub-failed11111".to_string());
    let make_entry = |run: RunId, body: RunBody| ChildRun {
        run,
        root: CHAT.to_string(),
        parent_agent: caller.agent.clone(),
        parent_session_id: CHAT.to_string(),
        parent_run: None,
        depth: 1,
        identity: crate::acp::subagents::SubagentAgent {
            name: "Mock Sub".to_string(),
            config_id: None,
            agent_id: "agent-child".to_string(),
        },
        profile_id: "mock-sub".to_string(),
        requested_model: None,
        child_cwd: "/tmp".to_string(),
        session_id: Some(SessionId("child-1".to_string())),
        title: "Mock Sub".to_string(),
        task: "task".to_string(),
        model: None,
        created_ms: 1,
        workspace: crate::acp::subagents::SubagentWorkspace {
            mode: SubagentWorkspaceMode::Shared,
            path: None,
            branch: None,
            base: None,
            diff_stat: None,
        },
        base_commit: None,
        body,
        result: None,
        waiters: Vec::new(),
        capture: Arc::new(RunCapture::default()),
        queued_prompts: std::collections::VecDeque::new(),
    };
    {
        let mut ledger = engine.ledger.lock();
        ledger.insert(make_entry(
            live_run.clone(),
            RunBody::Live {
                child_agent: AgentId("agent-child".to_string()),
                session: SessionId("child-1".to_string()),
                awaiting_permission: false,
            },
        ));
        ledger.insert(make_entry(
            failed_run.clone(),
            RunBody::Settled {
                end: RunEnd::Failed("boom".to_string()),
                at_ms: 5,
            },
        ));
    }

    // Another chat cannot send to this chat's run.
    let other_chat = CallerRef {
        agent: caller.agent.clone(),
        session: SessionId("chat-other".to_string()),
    };
    assert_eq!(
        engine
            .send(
                other_chat,
                SendRequest {
                    subagent_id: live_run.0.clone(),
                    message: "hijack".to_string(),
                    interrupt: false,
                },
            )
            .await,
        Err(SendError::NotFound)
    );

    // A failed run is refused with NotResumable.
    assert!(matches!(
        engine
            .send(
                caller.clone(),
                SendRequest {
                    subagent_id: failed_run.0.clone(),
                    message: "retry".to_string(),
                    interrupt: false,
                },
            )
            .await,
        Err(SendError::NotResumable(_))
    ));

    // Message bounds: blank and >8000 chars rejected; 8000 chars accepted past validation.
    assert_eq!(
        engine
            .send(
                caller.clone(),
                SendRequest {
                    subagent_id: live_run.0.clone(),
                    message: "   ".to_string(),
                    interrupt: false,
                },
            )
            .await,
        Err(SendError::InvalidInput("message"))
    );
    assert_eq!(
        engine
            .send(
                caller.clone(),
                SendRequest {
                    subagent_id: live_run.0.clone(),
                    message: "a".repeat(8_001),
                    interrupt: false,
                },
            )
            .await,
        Err(SendError::InvalidInput("message"))
    );
    assert_eq!(
        engine
            .send(
                caller.clone(),
                SendRequest {
                    subagent_id: "sub-missing".to_string(),
                    message: "a".repeat(8_000),
                    interrupt: false,
                },
            )
            .await,
        Err(SendError::NotFound)
    );
    assert_eq!(
        engine
            .send(
                caller,
                SendRequest {
                    subagent_id: "".to_string(),
                    message: "hi".to_string(),
                    interrupt: false,
                },
            )
            .await,
        Err(SendError::InvalidInput("subagentId"))
    );
}
