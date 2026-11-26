use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use serde_json::{json, Value};

use super::supervisor::Supervisor;
use super::{switch_notice, CodexLimitHandle, CODEX_CONFIG_ID};
use crate::acp::codex_accounts::usage::ProbeVerdict;
use crate::acp::codex_agent::{CodexAgentService, CodexSignInCheck, CodexUsageProbe};
use crate::acp::config::{AgentConfig, AgentId, SessionId};
use crate::acp::manager::real_loop_tests::{find_python, MOCK_AGENT, SKIP_ENV, TIMEOUT_TEST_LOCK};
use crate::acp::manager::SessionCreationContext;
use crate::acp::session_persistence::SessionPersistence;
use crate::acp::{AcpManager, SpawnScope};
use crate::web::prompt_turn::{run_prompt_turn, PromptTurnRequest};
use crate::web::WsRelaySink;

const PROJECT: &str = "proj-1";
const WAIT: Duration = Duration::from_secs(45);
const SCHEDULE_FILE: &str = "codex-limit-resumes.json";

struct FixedUsage {
    by_id: HashMap<String, f64>,
    resets_at: Arc<AtomicI64>,
}

#[async_trait::async_trait]
impl CodexUsageProbe for FixedUsage {
    async fn probe(&self, codex_home: &Path) -> ProbeVerdict {
        let key = codex_home
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("default");
        let percent = self
            .by_id
            .get(key)
            .or_else(|| self.by_id.get("default"))
            .copied()
            .unwrap_or(0.0);
        ProbeVerdict::Limits(json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": percent,
                    "resetsAt": self.resets_at.load(Ordering::Relaxed),
                    "windowDurationMins": 300
                }
            }
        }))
    }
}

pub(crate) struct Host {
    dir: tempfile::TempDir,
    pub(crate) acp: Arc<AcpManager>,
    pub(crate) relay: Arc<WsRelaySink>,
    persistence: Arc<SessionPersistence>,
    pub(crate) accounts: HashMap<&'static str, String>,
    resets_at: Arc<AtomicI64>,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Host {
    pub(crate) async fn start(
        usage: &[(&'static str, f64)],
        project_account: &str,
        resets_at: i64,
    ) -> Option<Self> {
        Self::start_ready(usage, project_account, resets_at, None).await
    }

    /// Load a resume that was already on disk, the way a host restart does.
    async fn start_with_saved_resume(
        usage: &[(&'static str, f64)],
        project_account: &str,
        session_id: &str,
        in_secs: i64,
    ) -> Option<Self> {
        Self::start_ready(
            usage,
            project_account,
            now_secs() + 3600,
            Some((session_id, in_secs)),
        )
        .await
    }

    async fn start_ready(
        usage: &[(&'static str, f64)],
        project_account: &str,
        resets_at: i64,
        saved: Option<(&str, i64)>,
    ) -> Option<Self> {
        let Some(_python) = find_python() else {
            if std::env::var(SKIP_ENV).as_deref() == Ok("1") {
                return None;
            }
            panic!("codex_limit real-loop tests need python3 on PATH (or {SKIP_ENV}=1)");
        };
        let serial = TIMEOUT_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let mut codex = CodexAgentService::with_logins_for_test(&state, &[]);
        let mut accounts = HashMap::new();
        let mut percents = HashMap::new();
        for (name, percent) in usage {
            let id = if *name == "default" {
                "default".to_string()
            } else {
                codex.insert_profile(name).unwrap().id
            };
            percents.insert(id.clone(), *percent);
            accounts.insert(*name, id);
        }
        let resets_at = Arc::new(AtomicI64::new(resets_at));
        codex.set_auto_switch(true).unwrap();
        codex.set_usage_probe_for_test(Arc::new(FixedUsage {
            by_id: percents,
            resets_at: Arc::clone(&resets_at),
        }));
        codex
            .set_project_account(
                PROJECT,
                Some(&accounts[project_account]),
                None,
                CodexSignInCheck::Skip,
            )
            .await
            .unwrap();
        let script = dir.path().join("mock_agent.py");
        std::fs::write(&script, MOCK_AGENT).unwrap();

        let persistence = SessionPersistence::open(dir.path().join("sessions"))
            .await
            .unwrap();
        let relay = Arc::new(WsRelaySink::with_persistence(64, persistence.clone()));
        let (handle, inbox) = CodexLimitHandle::channel();
        let claude = Arc::new(crate::acp::claude_agent::ClaudeAgentService::without_host_state());
        let acp = Arc::new(AcpManager::with_persistence_and_host_agents(
            vec![relay.clone(), handle.sink()],
            persistence.clone(),
            claude,
            Arc::new(codex),
        ));
        acp.set_codex_limit(handle);
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
                    "codex-limit",
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
            resets_at,
            _serial: serial,
        })
    }

    pub(crate) fn state(&self) -> PathBuf {
        self.dir.path().join("state")
    }

    async fn open_chat(&self, python: &str) -> (AgentId, SessionId) {
        self.open_chat_with(python, &[]).await
    }

    /// [`Self::open_chat`] with `env` set on the agent over the defaults.
    pub(crate) async fn open_chat_with(
        &self,
        python: &str,
        env: &[(&str, &str)],
    ) -> (AgentId, SessionId) {
        let script = self.dir.path().join("mock_agent.py");
        let mut agent_env = HashMap::from([
            ("MOCK_RESUME_OK".to_string(), "1".to_string()),
            ("MOCK_SESSION_IDS".to_string(), "1".to_string()),
        ]);
        agent_env.extend(
            env.iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string())),
        );
        let config = AgentConfig {
            config_id: Some(CODEX_CONFIG_ID.to_string()),
            name: "Codex".to_string(),
            command: python.to_string(),
            args: vec!["-u".to_string(), script.to_string_lossy().to_string()],
            env: agent_env,
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

    pub(crate) async fn log_until(
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

pub(crate) fn texts<'a>(records: &'a [(String, Value)], kind: &str) -> Vec<&'a str> {
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
        .filter(|(kind, _)| kind == "codex_limit_action")
        .map(|(_, payload)| payload.clone())
        .collect()
}

pub(crate) fn has_home_chunk(records: &[(String, Value)]) -> bool {
    texts(records, "message_chunk")
        .iter()
        .any(|text| text.starts_with("codex_home="))
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
async fn a_chat_at_its_limit_continues_on_the_account_with_most_room_with_no_client() {
    let Some(python) = find_python() else {
        return;
    };
    let Some(host) = Host::start(
        &[("default", 90.0), ("Work", 100.0), ("Spare", 10.0)],
        "Work",
        now_secs() + 3600,
    )
    .await
    else {
        return;
    };
    let (old_agent, session_id) = host.open_chat(python).await;
    host.prompt_and_leave(&old_agent, &session_id, "codexlimit");

    let records = host.log_until(&session_id, has_home_chunk).await;
    let (work, spare) = (&host.accounts["Work"], &host.accounts["Spare"]);
    let snapshot = host.acp.codex_agent().accounts_snapshot().await.unwrap();
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
            "codexlimit",
            switch_notice("Work", "Spare", "5-hour").as_str()
        ]
    );
    let spare_dir = host.state().join("codex-accounts").join(spare);
    assert_eq!(
        texts(&records, "message_chunk"),
        vec![format!("codex_home={}", spare_dir.display()).as_str()]
    );
    let _ = host.acp.kill(&new_agent).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_no_room_the_chat_continues_at_the_reset_after_its_agent_was_reaped() {
    let Some(python) = find_python() else {
        return;
    };
    let Some(host) = Host::start(&[("default", 100.0), ("Work", 100.0)], "Work", now_secs() + 3600)
        .await
    else {
        return;
    };
    let (old_agent, session_id) = host.open_chat(python).await;
    // The reset is measured from the open chat, so host setup cannot make it due
    // before the test has seen the scheduled resume.
    let resets_at = now_secs() + 3;
    host.resets_at.store(resets_at, Ordering::Relaxed);
    host.prompt_and_leave(&old_agent, &session_id, "codexlimit");

    let scheduled = host
        .log_until(&session_id, |records| !actions(records).is_empty())
        .await;
    assert_eq!(
        actions(&scheduled),
        vec![json!({
            "sessionId": session_id.0,
            "projectId": PROJECT,
            "action": "resume_scheduled",
            "at": resets_at * 1000,
        })]
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
        !texts(&records, "user_prompt").contains(&"codexlimit")
            || texts(&records, "user_prompt")
                .iter()
                .filter(|text| **text == "codexlimit")
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
    let Some(host) = Host::start_with_saved_resume(
        &[("default", 100.0), ("Work", 100.0)],
        "Work",
        "mock-1",
        6,
    )
    .await
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
