//! One provider's agent stopping must not drop the launch recipe another
//! provider is holding until a limit resets. Both supervisors hear every
//! stop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::supervisor::Supervisor as AntigravitySupervisor;
use super::{AntigravityLimitHandle, ANTIGRAVITY_CONFIG_ID};
use crate::acp::antigravity_agent::{AntigravityAgentService, AntigravitySignInCheck};
use crate::acp::claude_agent::{ClaudeAgentService, SignInCheck};
use crate::acp::claude_limit::supervisor::Supervisor as ClaudeSupervisor;
use crate::acp::claude_limit::{ClaudeLimitHandle, CLAUDE_CONFIG_ID};
use crate::acp::config::{AgentConfig, AgentId, SessionId};
use crate::acp::manager::real_loop_tests::{find_python, MOCK_AGENT, SKIP_ENV, TIMEOUT_TEST_LOCK};
use crate::acp::manager::SessionCreationContext;
use crate::acp::session_persistence::SessionPersistence;
use crate::acp::{AcpManager, SpawnScope};
use crate::web::WsRelaySink;

const PROJECT: &str = "proj-1";
const WAIT: Duration = Duration::from_secs(45);

struct Host {
    dir: tempfile::TempDir,
    acp: Arc<AcpManager>,
    persistence: Arc<SessionPersistence>,
    python: &'static str,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Host {
    async fn start() -> Option<Self> {
        let Some(python) = find_python() else {
            if std::env::var(SKIP_ENV).as_deref() == Ok("1") {
                return None;
            }
            panic!("cross-keep tests need python3 on PATH (or {SKIP_ENV}=1)");
        };
        if which("node").is_none() {
            if std::env::var(SKIP_ENV).as_deref() == Ok("1") {
                return None;
            }
            panic!("cross-keep tests need node on PATH (or {SKIP_ENV}=1)");
        }
        let serial = TIMEOUT_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let claude = ClaudeAgentService::with_logins_for_test(&state, &[]);
        let work = claude.insert_profile("Work").unwrap().id;
        claude
            .set_project_account(PROJECT, Some(&work), None, SignInCheck::Skip)
            .await
            .unwrap();
        let antigravity = AntigravityAgentService::with_logins_for_test(&state, &[]);
        let antigravity_work = antigravity.insert_profile("Work").unwrap().id;
        antigravity
            .set_project_account(
                PROJECT,
                Some(&antigravity_work),
                None,
                AntigravitySignInCheck::Skip,
            )
            .await
            .unwrap();
        write_entrypoint(&state, python, dir.path());

        let persistence = SessionPersistence::open(dir.path().join("sessions"))
            .await
            .unwrap();
        let relay = Arc::new(WsRelaySink::with_persistence(64, persistence.clone()));
        let (claude_handle, claude_inbox) = ClaudeLimitHandle::channel();
        let (antigravity_handle, antigravity_inbox) = AntigravityLimitHandle::channel();
        let acp = Arc::new(
            AcpManager::with_persistence_and_host_agents(
                vec![
                    relay.clone(),
                    claude_handle.sink(),
                    antigravity_handle.sink(),
                ],
                persistence.clone(),
                Arc::new(claude),
                Arc::new(crate::acp::CodexAgentService::without_host_state()),
            )
            .with_antigravity_agent(Arc::new(antigravity)),
        );
        acp.set_claude_limit(claude_handle);
        acp.set_antigravity_limit(antigravity_handle);
        let claude_schedule = crate::acp::claude_limit::schedule::Schedule::load(Some(
            state.join("claude-limit-resumes.json"),
        ));
        tokio::spawn(
            ClaudeSupervisor::new(
                acp.clone(),
                relay.clone(),
                claude_schedule,
                Duration::ZERO,
                claude_inbox.clock,
                claude_inbox.limited,
            )
            .run(claude_inbox.rx),
        );
        tokio::spawn(
            AntigravitySupervisor::new(
                acp.clone(),
                relay,
                crate::acp::claude_limit::schedule::Schedule::open(
                    Some(state.join("antigravity-limit-resumes.json")),
                    "antigravity-limit",
                ),
                Duration::ZERO,
                antigravity_inbox.clock,
                antigravity_inbox.limited,
            )
            .run(antigravity_inbox.rx),
        );
        Some(Self {
            dir,
            acp,
            persistence,
            python,
            _serial: serial,
        })
    }

    fn state(&self) -> PathBuf {
        self.dir.path().join("state")
    }

    fn cwd(&self) -> String {
        self.dir.path().to_string_lossy().to_string()
    }

    async fn open_claude(&self) -> (AgentId, SessionId) {
        let entrypoint = self.state().join(
            "acp-registry-binaries/claude-acp/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js",
        );
        self.open(AgentConfig {
            config_id: Some(CLAUDE_CONFIG_ID.to_string()),
            name: "Claude Agent".to_string(),
            command: "node".to_string(),
            args: vec![entrypoint.to_string_lossy().to_string()],
            env: mock_env(),
            allow_terminal: false,
        })
        .await
    }

    async fn open_antigravity(&self) -> (AgentId, SessionId) {
        let script = self.dir.path().join("mock_agent.py");
        self.open(AgentConfig {
            config_id: Some(ANTIGRAVITY_CONFIG_ID.to_string()),
            name: "Antigravity".to_string(),
            command: self.python.to_string(),
            args: vec!["-u".to_string(), script.to_string_lossy().to_string()],
            env: mock_env(),
            allow_terminal: false,
        })
        .await
    }

    async fn open(&self, config: AgentConfig) -> (AgentId, SessionId) {
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
                self.cwd(),
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
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

fn mock_env() -> HashMap<String, String> {
    HashMap::from([
        ("MOCK_RESUME_OK".to_string(), "1".to_string()),
        ("MOCK_SESSION_IDS".to_string(), "unique".to_string()),
    ])
}

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|dir| {
            let named = if cfg!(windows) {
                dir.join(format!("{cmd}.exe"))
            } else {
                dir.join(cmd)
            };
            if named.is_file() {
                return Some(named);
            }
            if cfg!(windows) {
                let plain = dir.join(cmd);
                if plain.is_file() {
                    return Some(plain);
                }
            }
            None
        })
    })
}

fn write_entrypoint(state: &Path, python: &str, dir: &Path) {
    let package = state.join(
        "acp-registry-binaries/claude-acp/node_modules/@agentclientprotocol/claude-agent-acp",
    );
    std::fs::create_dir_all(package.join("dist")).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"@agentclientprotocol/claude-agent-acp","version":"0.78.0","bin":{"claude-agent-acp":"dist/index.js"}}"#,
    )
    .unwrap();
    let script = dir.join("mock_agent.py");
    std::fs::write(&script, MOCK_AGENT).unwrap();
    std::fs::write(
        package.join("dist/index.js"),
        format!(
            "const child = require('child_process').spawn({python:?}, ['-u', {script:?}], {{ stdio: 'inherit' }});\nchild.on('exit', (code) => process.exit(code ?? 1));\n",
            script = script.to_string_lossy()
        ),
    )
    .unwrap();
}

fn later(secs: u64) -> SystemTime {
    SystemTime::now() + Duration::from_secs(secs)
}

fn actions(records: &[(String, Value)], kind: &str) -> Vec<Value> {
    records
        .iter()
        .filter(|(name, _)| name == kind)
        .map(|(_, payload)| payload.clone())
        .collect()
}

/// After `idle`'s agent stops, wait until that recipe is gone, then require
/// `held` to still be there. Both supervisors handle the stop; the one that
/// does not own `held` must not delete it.
async fn survives_the_other_stop(acp: &AcpManager, idle: &SessionId, held: &SessionId) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            acp.launch_recipe(held).is_some(),
            "the other provider's stop deleted the recipe of a chat waiting for its limit to reset"
        );
        if acp.launch_recipe(idle).is_none() {
            let stable = tokio::time::Instant::now() + Duration::from_millis(400);
            while tokio::time::Instant::now() < stable {
                assert!(
                    acp.launch_recipe(held).is_some(),
                    "the other provider's stop deleted the recipe of a chat waiting for its limit to reset"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the idle chat's recipe was never forgotten"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_antigravity_stop_keeps_a_claude_recipe_and_the_chat_resumes() {
    let Some(host) = Host::start().await else {
        return;
    };
    let (claude_agent, claude_chat) = host.open_claude().await;
    host.acp
        .claude_limit()
        .unwrap()
        .set_resume(claude_chat.clone(), Some(later(3600)))
        .await
        .unwrap();
    let (antigravity_agent, antigravity_chat) = host.open_antigravity().await;
    host.acp.reap_agent(&claude_agent).await.unwrap();
    host.acp.reap_agent(&antigravity_agent).await.unwrap();

    survives_the_other_stop(&host.acp, &antigravity_chat, &claude_chat).await;

    host.acp
        .claude_limit()
        .unwrap()
        .set_resume(claude_chat.clone(), Some(later(1)))
        .await
        .expect("the recipe is still on the host");
    let records = host
        .log_until(&claude_chat, |records| {
            let actions = actions(records, "claude_limit_action");
            if actions.iter().any(|action| action["action"] == "failed") {
                return true;
            }
            let resumed = actions.iter().any(|action| action["action"] == "resumed");
            let answered = records.iter().any(|(kind, payload)| {
                kind == "message_chunk"
                    && payload
                        .pointer("/content/text")
                        .and_then(|text| text.as_str())
                        .is_some_and(|text| text.starts_with("config_dir="))
            });
            resumed && answered
        })
        .await;
    let done = actions(&records, "claude_limit_action");
    assert_eq!(
        done.last().map(|action| action["action"].as_str()),
        Some(Some("resumed")),
        "resume after a antigravity stop: {done:?}"
    );
    assert!(
        records.iter().any(|(kind, payload)| {
            kind == "message_chunk"
                && payload
                    .pointer("/content/text")
                    .and_then(|text| text.as_str())
                    .is_some_and(|text| text.starts_with("config_dir="))
        }),
        "the resumed chat did not answer"
    );
    let _ = host.acp.kill(&host.acp.list_agents()[0]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claude_stop_keeps_a_antigravity_recipe_and_the_chat_resumes() {
    let Some(host) = Host::start().await else {
        return;
    };
    let (antigravity_agent, antigravity_chat) = host.open_antigravity().await;
    host.acp
        .antigravity_limit()
        .unwrap()
        .set_resume(antigravity_chat.clone(), Some(later(3600)))
        .await
        .unwrap();
    let (claude_agent, claude_chat) = host.open_claude().await;
    host.acp.reap_agent(&antigravity_agent).await.unwrap();
    host.acp.reap_agent(&claude_agent).await.unwrap();

    survives_the_other_stop(&host.acp, &claude_chat, &antigravity_chat).await;

    host.acp
        .antigravity_limit()
        .unwrap()
        .set_resume(antigravity_chat.clone(), Some(later(1)))
        .await
        .expect("the recipe is still on the host");
    let records = host
        .log_until(&antigravity_chat, |records| {
            let actions = actions(records, "antigravity_limit_action");
            if actions.iter().any(|action| action["action"] == "failed") {
                return true;
            }
            let resumed = actions.iter().any(|action| action["action"] == "resumed");
            let answered = records.iter().any(|(kind, payload)| {
                kind == "message_chunk"
                    && payload
                        .pointer("/content/text")
                        .and_then(|text| text.as_str())
                        .is_some_and(|text| text.starts_with("antigravity_home="))
            });
            resumed && answered
        })
        .await;
    let done = actions(&records, "antigravity_limit_action");
    assert_eq!(
        done.last().map(|action| action["action"].as_str()),
        Some(Some("resumed")),
        "resume after a claude stop: {done:?}"
    );
    assert!(
        records.iter().any(|(kind, payload)| {
            kind == "message_chunk"
                && payload
                    .pointer("/content/text")
                    .and_then(|text| text.as_str())
                    .is_some_and(|text| text.starts_with("antigravity_home="))
        }),
        "the resumed chat did not answer"
    );
    let _ = host.acp.kill(&host.acp.list_agents()[0]).await;
}
