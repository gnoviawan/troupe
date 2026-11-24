use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use serde_json::Value;

use super::super::{AntigravityLimitHandle, ANTIGRAVITY_CONFIG_ID};
use super::Supervisor;
use crate::acp::antigravity_agent::AntigravityLaunchAuth;
use crate::acp::claude_limit::schedule::Schedule;
use crate::acp::config::{AgentConfig, AgentId, SessionId};
use crate::acp::manager::AgentLaunch;
use crate::acp::AcpManager;
use crate::web::sink::{AcpEvent, EventSink};
use crate::web::WsRelaySink;

const SCHEDULE_FILE: &str = "antigravity-limit-resumes.json";

#[derive(Default)]
struct Recorded(parking_lot::Mutex<Vec<Value>>);

impl EventSink for Recorded {
    fn emit(&self, event: &AcpEvent) {
        if event.type_ == "acp:antigravity_limit_action" {
            self.0.lock().push(event.payload.clone());
        }
    }
}

struct Host {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    acp: Arc<AcpManager>,
    handle: AntigravityLimitHandle,
    actions: Arc<Recorded>,
}

fn start() -> Host {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SCHEDULE_FILE);
    let actions = Arc::new(Recorded::default());
    let relay = Arc::new(WsRelaySink::new());
    let (handle, inbox) = AntigravityLimitHandle::channel();
    let acp = Arc::new(AcpManager::new(vec![
        relay.clone(),
        handle.sink(),
        actions.clone(),
    ]));
    acp.set_antigravity_limit(handle.clone());
    tokio::spawn(
        Supervisor::new(
            acp.clone(),
            relay,
            Schedule::open(Some(path.clone()), "antigravity-limit"),
            Duration::ZERO,
            inbox.clock,
            inbox.limited,
        )
        .run(inbox.rx),
    );
    Host {
        _dir: dir,
        path,
        acp,
        handle,
        actions,
    }
}

fn launch(config_id: &str, antigravity: AntigravityLaunchAuth) -> AgentLaunch {
    AgentLaunch {
        config: AgentConfig {
            config_id: Some(config_id.to_string()),
            name: "agent".to_string(),
            command: "node".to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            allow_terminal: false,
        },
        project_id: Some("proj-1".to_string()),
        claude: crate::acp::claude_agent::ClaudeLaunchAuth::Unmanaged,
        codex: crate::acp::codex_agent::CodexLaunchAuth::Unmanaged,
        antigravity,
    }
}

fn work() -> AgentLaunch {
    launch(
        ANTIGRAVITY_CONFIG_ID,
        AntigravityLaunchAuth::Account("work".to_string()),
    )
}

fn in_an_hour() -> Option<SystemTime> {
    Some(
        SystemTime::UNIX_EPOCH
            + Duration::from_secs(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 3600,
            ),
    )
}

fn scheduled(path: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    file["resumes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["sessionId"].as_str().unwrap().to_string())
        .collect()
}

async fn until(what: &str, done: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(tokio::time::Instant::now() < deadline, "never saw: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn cancelled(session: &str) -> Value {
    serde_json::json!({
        "sessionId": session,
        "projectId": "proj-1",
        "action": "resume_cancelled",
    })
}

#[tokio::test]
async fn a_turn_of_the_chat_cancels_its_scheduled_resume() {
    let host = start();
    let agent = AgentId("agent-1".to_string());
    host.acp.install_test_agent_with_turns(
        agent.clone(),
        ["s1".to_string()].into(),
        Vec::new(),
        false,
    );
    host.acp.record_launch_for_test(&agent, "s1", work());
    host.handle
        .set_resume(SessionId("s1".to_string()), in_an_hour())
        .await
        .unwrap();
    assert_eq!(scheduled(&host.path), vec!["s1".to_string()]);

    let content = vec![ContentBlock::Text(TextContent::new("go on"))];
    let started = host
        .acp
        .start_prompt(
            &agent,
            SessionId("s1".to_string()),
            content,
            None,
            Some(Box::new(|| {})),
        )
        .await
        .unwrap();
    let _ = host.acp.wait_prompt(started).await;

    until("the resume dropped", || scheduled(&host.path).is_empty()).await;
    until("resume_cancelled", || {
        host.actions.0.lock().last() == Some(&cancelled("s1"))
    })
    .await;
}

#[tokio::test]
async fn set_resume_refuses_a_chat_without_a_managed_antigravity_account() {
    let host = start();
    let cases = [
        (
            "claude",
            launch("acp-registry:claude-acp", AntigravityLaunchAuth::Unmanaged),
        ),
        (
            "project-env",
            launch(ANTIGRAVITY_CONFIG_ID, AntigravityLaunchAuth::Fixed),
        ),
    ];
    for (session, launch) in cases {
        let agent = AgentId(format!("agent-{session}"));
        host.acp
            .install_test_agent_with_sessions(agent.clone(), [session.to_string()].into());
        host.acp.record_launch_for_test(&agent, session, launch);
        assert_eq!(
            host.handle
                .set_resume(SessionId(session.to_string()), in_an_hour())
                .await,
            Err("This chat does not run on an Antigravity account Troupe manages.".to_string()),
            "{session}"
        );
    }
    assert_eq!(scheduled(&host.path), Vec::<String>::new());
}

#[tokio::test]
async fn closing_a_chat_drops_its_scheduled_resume() {
    let host = start();
    let agent = AgentId("agent-1".to_string());
    host.acp
        .install_test_agent_with_closable_sessions(agent.clone(), vec!["s1".to_string()], true);
    host.acp.record_launch_for_test(&agent, "s1", work());
    host.handle
        .set_resume(SessionId("s1".to_string()), in_an_hour())
        .await
        .unwrap();

    host.acp
        .close_session(&agent, SessionId("s1".to_string()))
        .await
        .unwrap();

    until("the resume dropped", || scheduled(&host.path).is_empty()).await;
    until("resume_cancelled", || {
        host.actions.0.lock().last() == Some(&cancelled("s1"))
    })
    .await;
    assert!(host
        .acp
        .launch_recipe(&SessionId("s1".to_string()))
        .is_none());
}

#[tokio::test]
async fn a_resume_due_after_a_restart_marks_the_chat_for_its_client() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SCHEDULE_FILE);
    std::fs::write(
        &path,
        serde_json::json!({"version": 1, "resumes": [{
            "sessionId": "s-restarted", "projectId": "proj-1", "at": 1,
            "accountId": "a1", "window": "rate_limited"}]})
        .to_string(),
    )
    .unwrap();
    let live = Arc::new(Recorded::default());
    let relay = Arc::new(WsRelaySink::new());
    let (handle, inbox) = AntigravityLimitHandle::channel();
    let acp = Arc::new(AcpManager::new(vec![
        relay.clone(),
        handle.sink(),
        live.clone(),
    ]));
    tokio::spawn(
        Supervisor::new(
            acp,
            relay,
            Schedule::open(Some(path.clone()), "antigravity-limit"),
            Duration::ZERO,
            inbox.clock,
            inbox.limited,
        )
        .run(inbox.rx),
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let event = loop {
        let found = live.0.lock().last().cloned();
        if let Some(event) = found {
            break event;
        }
        assert!(tokio::time::Instant::now() < deadline, "no action event");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(
        event,
        serde_json::json!({
            "sessionId": "s-restarted",
            "projectId": "proj-1",
            "action": "failed",
            "reason": "The host restarted while this chat waited for its limit to reset. Send a message to continue.",
        })
    );
    assert_eq!(
        Schedule::open(Some(path), "antigravity-limit").next_at(),
        None
    );
    drop(handle);
}
