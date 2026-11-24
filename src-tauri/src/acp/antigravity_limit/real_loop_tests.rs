use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use serde_json::{json, Value};

use super::supervisor::Supervisor;
use super::{switch_notice, AntigravityLimitHandle, ANTIGRAVITY_CONFIG_ID};
use crate::acp::antigravity_agent::{AntigravityAgentService, AntigravitySignInCheck};
use crate::acp::config::{AgentConfig, AgentId, SessionId};
use crate::acp::manager::real_loop_tests::{find_python, MOCK_AGENT, SKIP_ENV, TIMEOUT_TEST_LOCK};
use crate::acp::manager::SessionCreationContext;
use crate::acp::session_persistence::SessionPersistence;
use crate::acp::{AcpManager, SpawnScope};
use crate::web::prompt_turn::{run_prompt_turn, PromptTurnRequest};
use crate::web::WsRelaySink;

const PROJECT: &str = "proj-1";
const WAIT: Duration = Duration::from_secs(45);
const SCHEDULE_FILE: &str = "antigravity-limit-resumes.json";
const SIGNED_OUT: &[&str] = &["default"];

struct Host {
    dir: tempfile::TempDir,
    acp: Arc<AcpManager>,
    relay: Arc<WsRelaySink>,
    persistence: Arc<SessionPersistence>,
    accounts: HashMap<&'static str, String>,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Host {
    async fn start(names: &[&'static str], project_account: &str) -> Option<Self> {
        Self::start_ready(names, project_account, None).await
    }

    /// Load a resume that was already on disk, the way a host restart does.
    async fn start_with_saved_resume(
        names: &[&'static str],
        project_account: &str,
        session_id: &str,
        in_secs: i64,
    ) -> Option<Self> {
        Self::start_ready(names, project_account, Some((session_id, in_secs))).await
    }

    async fn start_ready(
        names: &[&'static str],
        project_account: &str,
        saved: Option<(&str, i64)>,
    ) -> Option<Self> {
        let Some(_python) = find_python() else {
            if std::env::var(SKIP_ENV).as_deref() == Ok("1") {
                return None;
            }
            panic!("antigravity_limit real-loop tests need python3 on PATH (or {SKIP_ENV}=1)");
        };
        let serial = TIMEOUT_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let antigravity = AntigravityAgentService::with_logins_for_test(&state, SIGNED_OUT);
        let mut accounts = HashMap::new();
        for name in names {
            let id = if *name == "default" {
                "default".to_string()
            } else {
                antigravity.insert_profile(name).unwrap().id
            };
            accounts.insert(*name, id);
        }
        antigravity
            .set_project_account(
                PROJECT,
                Some(&accounts[project_account]),
                None,
                AntigravitySignInCheck::Skip,
            )
            .await
            .unwrap();
        let script = dir.path().join("mock_agent.py");
        std::fs::write(&script, MOCK_AGENT).unwrap();

        let persistence = SessionPersistence::open(dir.path().join("sessions"))
            .await
            .unwrap();
        let relay = Arc::new(WsRelaySink::with_persistence(64, persistence.clone()));
        let (handle, inbox) = AntigravityLimitHandle::channel();
        let claude = Arc::new(crate::acp::claude_agent::ClaudeAgentService::without_host_state());
        let acp = Arc::new(
            AcpManager::with_persistence_and_host_agents(
                vec![relay.clone(), handle.sink()],
                persistence.clone(),
                claude,
                Arc::new(crate::acp::CodexAgentService::without_host_state()),
            )
            .with_antigravity_agent(Arc::new(antigravity)),
        );
        acp.set_antigravity_limit(handle);
        if let Some((session_id, in_secs)) = saved {
            let at = now_secs() + in_secs;
            let body = serde_json::json!({"version": 1, "resumes": [{
                "sessionId": session_id,
                "projectId": PROJECT,
                "at": at,
                "accountId": accounts[project_account],
                "window": "five_hour"
            }]});
            std::fs::write(state.join(SCHEDULE_FILE), body.to_string()).unwrap();
        }
        tokio::spawn(
            Supervisor::new(
                acp.clone(),
                relay.clone(),
                crate::acp::claude_limit::schedule::Schedule::open(
                    Some(state.join(SCHEDULE_FILE)),
                    "antigravity-limit",
                ),
                Duration::ZERO,
                inbox.clock,
                inbox.limited,
            )
            .run(inbox.rx),
        );
        Some(Self {
            dir,
            acp,
            relay,
            persistence,
            accounts,
            _serial: serial,
        })
    }

    fn state(&self) -> PathBuf {
        self.dir.path().join("state")
    }

    async fn open_chat(&self, python: &str) -> (AgentId, SessionId) {
        let script = self.dir.path().join("mock_agent.py");
        let config = AgentConfig {
            config_id: Some(ANTIGRAVITY_CONFIG_ID.to_string()),
            name: "Antigravity".to_string(),
            command: python.to_string(),
            args: vec!["-u".to_string(), script.to_string_lossy().to_string()],
            env: HashMap::from([
                ("MOCK_RESUME_OK".to_string(), "1".to_string()),
                ("MOCK_SESSION_IDS".to_string(), "1".to_string()),
            ]),
            allow_terminal: false,
        };
        let agent_id = self
            .acp
            .spawn_with_command_id(
                config,
                None,
                SpawnScope {
                    project_id: Some(PROJECT.to_string()),
                    ..SpawnScope::default()
                },
            )
            .await
            .expect("managed mock agent spawns")
            .agent_id;
        let session_id = self
            .acp
            .new_session_with_context(
                &agent_id,
                self.dir.path().to_string_lossy().to_string(),
                vec![],
                SessionCreationContext {
                    project_id: Some(PROJECT.to_string()),
                    ..SessionCreationContext::default()
                },
            )
            .await
            .expect("mock session")
            .session_id;
        (agent_id, session_id)
    }

    fn prompt_and_leave(&self, agent_id: &AgentId, session_id: &SessionId, text: &str) {
        let blocks = vec![ContentBlock::Text(TextContent::new(text))];
        let request = PromptTurnRequest {
            agent_id: agent_id.clone(),
            session_id: session_id.clone(),
            content: blocks.clone(),
            record_content: blocks,
            turn_id: Some(format!("client-turn-{}", uuid::Uuid::new_v4())),
        };
        let (acp, relay) = (self.acp.clone(), self.relay.clone());
        tokio::spawn(async move {
            let _ = run_prompt_turn(&acp, &relay, request).await;
        });
    }

    async fn log_until(
        &self,
        session_id: &SessionId,
        done: impl Fn(&[(String, Value)]) -> bool,
    ) -> Vec<(String, Value)> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let _ = self.persistence.flush_session(&session_id.0).await;
            let records: Vec<(String, Value)> = self
                .persistence
                .replay_after(&session_id.0, 0)
                .unwrap_or_default()
                .into_iter()
                .map(|record| (record.type_, record.payload))
                .collect();
            if done(&records) {
                return records;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "session log never matched; it holds {:?}",
                records.iter().map(|(kind, _)| kind).collect::<Vec<_>>()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn texts<'a>(records: &'a [(String, Value)], kind: &str) -> Vec<&'a str> {
    records
        .iter()
        .filter(|(k, _)| k == kind)
        .filter_map(|(k, payload)| {
            let path = if k == "user_prompt" {
                "/content/0/text"
            } else {
                "/content/text"
            };
            payload.pointer(path)?.as_str()
        })
        .collect()
}

fn actions(records: &[(String, Value)]) -> Vec<Value> {
    records
        .iter()
        .filter(|(kind, _)| kind == "antigravity_limit_action")
        .map(|(_, payload)| payload.clone())
        .collect()
}

fn has_home_chunk(records: &[(String, Value)]) -> bool {
    texts(records, "message_chunk")
        .iter()
        .any(|text| text.starts_with("antigravity_home="))
}

fn now_secs() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chat_at_its_limit_continues_on_the_next_signed_in_account_with_no_client() {
    let Some(python) = find_python() else {
        return;
    };
    let Some(host) = Host::start(&["default", "Work", "Spare", "Side"], "Work").await else {
        return;
    };
    let (old_agent, session_id) = host.open_chat(python).await;
    host.prompt_and_leave(&old_agent, &session_id, "antigravitylimit");

    let records = host.log_until(&session_id, has_home_chunk).await;
    let (work, spare) = (&host.accounts["Work"], &host.accounts["Spare"]);
    let snapshot = host
        .acp
        .antigravity_agent()
        .accounts_snapshot()
        .await
        .unwrap();
    assert_eq!(snapshot.project_profiles.get(PROJECT), Some(work));
    assert_eq!(
        snapshot.chat_profiles,
        std::collections::BTreeMap::from([(session_id.0.clone(), spare.clone())])
    );
    let live_agents = host.acp.list_agents();
    assert_eq!(live_agents.len(), 1);
    let new_agent = live_agents[0].clone();
    assert_ne!(new_agent, old_agent);
    assert_eq!(
        actions(&records),
        vec![json!({
            "sessionId": session_id.0,
            "projectId": PROJECT,
            "action": "switched",
            "from": {"id": work, "name": "Work"},
            "to": {"id": spare, "name": "Spare"},
            "fromAgentId": old_agent.0,
            "newAgentId": new_agent.0,
        })]
    );
    assert_eq!(
        texts(&records, "user_prompt"),
        vec![
            "antigravitylimit",
            switch_notice("Work", "Spare", "usage").as_str()
        ]
    );
    let spare_dir = host.state().join("antigravity-accounts").join(spare);
    assert_eq!(
        texts(&records, "message_chunk"),
        vec![
            // The limit message stays in the transcript, as Antigravity sent it.
            "Usage Limit Reached\n\nYou have reached your current quota for this period.",
            format!("antigravity_home={}", spare_dir.display()).as_str()
        ]
    );
    let _ = host.acp.kill(&new_agent).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_no_room_the_chat_continues_at_the_reset_after_its_agent_was_reaped() {
    let Some(python) = find_python() else {
        return;
    };
    let Some(host) = Host::start(&["default", "Work"], "Work").await else {
        return;
    };
    let (old_agent, session_id) = host.open_chat(python).await;
    let before = now_secs();
    host.prompt_and_leave(&old_agent, &session_id, "antigravitylimit 3 seconds");

    let scheduled = host
        .log_until(&session_id, |records| !actions(records).is_empty())
        .await;
    let after = now_secs();
    let scheduled = actions(&scheduled);
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0]["action"], "resume_scheduled");
    assert_eq!(scheduled[0]["sessionId"], session_id.0);
    assert_eq!(scheduled[0]["projectId"], PROJECT);
    let at = scheduled[0]["at"].as_i64().unwrap();
    assert!(
        ((before + 3) * 1000..=(after + 3) * 1000).contains(&at),
        "the resume is the reset the limit message named: {at}"
    );
    let file: Value =
        serde_json::from_slice(&std::fs::read(host.state().join(SCHEDULE_FILE)).unwrap()).unwrap();
    assert_eq!(file["resumes"][0]["sessionId"], session_id.0);

    host.acp.reap_agent(&old_agent).await.unwrap();
    assert_eq!(host.acp.list_agents(), Vec::<AgentId>::new());
    assert!(
        host.acp.launch_recipe(&session_id).is_some(),
        "the idle reaper keeps the recipe of a chat that waits for its reset"
    );

    let records = host.log_until(&session_id, has_home_chunk).await;
    let live_agents = host.acp.list_agents();
    assert_eq!(live_agents.len(), 1);
    assert_eq!(actions(&records)[1]["action"], "resumed");
    assert!(texts(&records, "user_prompt")
        .iter()
        .any(|text| text.starts_with("Troupe continued this chat")));
    assert!(
        !texts(&records, "user_prompt").contains(&"antigravitylimit 3 seconds")
            || texts(&records, "user_prompt")
                .iter()
                .filter(|text| **text == "antigravitylimit 3 seconds")
                .count()
                == 1
    );
    let _ = host.acp.kill(&live_agents[0]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_loaded_from_disk_survives_reopen_and_a_reap() {
    let Some(python) = find_python() else {
        return;
    };
    let Some(host) = Host::start_with_saved_resume(&["default", "Work"], "Work", "mock-1", 6).await
    else {
        return;
    };
    let (agent_id, session_id) = host.open_chat(python).await;
    assert_eq!(session_id.0, "mock-1");
    host.acp.reap_agent(&agent_id).await.unwrap();
    assert!(
        host.acp.launch_recipe(&session_id).is_some(),
        "a resume loaded at startup keeps its recipe after the reopened agent is reaped"
    );
    let records = host.log_until(&session_id, has_home_chunk).await;
    assert!(
        actions(&records)
            .iter()
            .any(|action| action["action"] == "resumed"),
        "the loaded resume did not continue the chat: {:?}",
        actions(&records)
    );
    let _ = host.acp.kill(&host.acp.list_agents()[0]).await;
}
