use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{run_usage_command, Failure, UsageReader, MAX_REPLY_BYTES};
use crate::acp::config::AgentConfig;
use crate::acp::events;
use crate::acp::manager::real_loop_tests::{find_python, SKIP_ENV, TIMEOUT_TEST_LOCK};
use crate::acp::manager::AcpManager;
use crate::web::sink::{AcpEvent, EventSink};

fn agent_chunk(text: &str) -> AcpEvent {
    AcpEvent {
        sid: Some("s1".to_string()),
        type_: events::EVENT_MESSAGE_CHUNK,
        payload: serde_json::json!({
            "role": "agent",
            "content": { "type": "text", "text": text }
        }),
    }
}

fn commands_update(names: &[&str]) -> AcpEvent {
    let commands: Vec<_> = names
        .iter()
        .map(|name| serde_json::json!({ "name": name, "description": "" }))
        .collect();
    AcpEvent {
        sid: Some("s1".to_string()),
        type_: events::EVENT_COMMANDS_UPDATE,
        payload: serde_json::json!({ "availableCommands": commands }),
    }
}

#[tokio::test]
async fn usage_stays_offered_after_a_later_update_leaves_it_out() {
    let reader = UsageReader::default();

    reader.emit(&commands_update(&["usage"]));
    reader.emit(&commands_update(&["logout"]));

    assert!(reader.offers_usage(Duration::from_millis(50)).await);
}

#[tokio::test]
async fn usage_is_not_offered_when_no_update_lists_it() {
    let reader = UsageReader::default();

    reader.emit(&commands_update(&["logout"]));

    assert!(!reader.offers_usage(Duration::from_millis(50)).await);
}

#[test]
fn a_reply_over_the_cap_keeps_only_the_text_before_the_first_chunk_that_did_not_fit() {
    let reader = UsageReader::default();

    reader.emit(&agent_chunk("### Quota Limits\n"));
    reader.emit(&agent_chunk(&"x".repeat(MAX_REPLY_BYTES)));
    reader.emit(&agent_chunk("- tail"));

    assert_eq!(reader.take_text(), "### Quota Limits\n");
}

const MOCK_AGENT: &str = r#"
import json, os, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def update(update):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": "s1", "update": update}})

with open(os.environ["MOCK_PROMPTS_FILE"], "w") as f:
    pass

for line in sys.stdin:
    msg = json.loads(line)
    method, rid = msg.get("method"), msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": rid, "result": {"sessionId": "s1", "_meta": {
            "antigravity": {"tier": {"name": "Google AI Pro", "id": "g1-pro-tier",
                                     "upgradeUri": ""}}}}})
        commands = [{"name": "logout", "description": "Sign out."}]
        if os.environ["MOCK_USAGE"] == "late":
            update({"sessionUpdate": "available_commands_update",
                    "availableCommands": list(commands)})
        if os.environ["MOCK_USAGE"] != "none":
            commands.append({"name": "usage",
                             "description": "Show model token usage and quota limits."})
        update({"sessionUpdate": "available_commands_update",
                "availableCommands": commands})
    elif method == "session/prompt":
        text = msg["params"]["prompt"][0]["text"]
        with open(os.environ["MOCK_PROMPTS_FILE"], "a") as f:
            f.write(text + "\n")
        update({"sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": os.environ["MOCK_REPLY"]}})
        send({"jsonrpc": "2.0", "id": rid, "result": {"stopReason": "end_turn"}})
"#;

const REPLY: &str = "### Session Usage
- **Session total:** data not available yet

### Quota Limits
- **Gemini Models**
  - **Weekly Limit Remaining:** 91% remaining (resets Oct 02, 2026, 14:00 UTC)
- **Claude and GPT models**
  - **Weekly Limit Remaining:** 100% remaining";

/// When the mock agent lists `usage` among its commands.
#[derive(Clone, Copy)]
enum Offer {
    First,
    SecondUpdate,
    Never,
}

impl Offer {
    fn env(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::SecondUpdate => "late",
            Self::Never => "none",
        }
    }
}

struct Mock {
    acp: Arc<AcpManager>,
    dir: tempfile::TempDir,
    python: &'static str,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Mock {
    async fn new() -> Option<Self> {
        let Some(python) = find_python() else {
            if std::env::var(SKIP_ENV).as_deref() == Ok("1") {
                eprintln!("SKIP usage check tests: no python interpreter ({SKIP_ENV}=1)");
                return None;
            }
            panic!("usage check tests need `python3` on PATH, or {SKIP_ENV}=1 to skip");
        };
        let serial = TIMEOUT_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mock_agent.py"), MOCK_AGENT).unwrap();
        Some(Self {
            acp: Arc::new(AcpManager::new(vec![])),
            dir,
            python,
            _serial: serial,
        })
    }

    fn config(&self, usage: Offer) -> AgentConfig {
        let env = HashMap::from([
            ("MOCK_USAGE".to_string(), usage.env().to_string()),
            ("MOCK_REPLY".to_string(), REPLY.to_string()),
            (
                "MOCK_PROMPTS_FILE".to_string(),
                self.prompts_file().to_string_lossy().into_owned(),
            ),
        ]);
        AgentConfig {
            config_id: None,
            name: "mock-antigravity".to_string(),
            command: self.python.to_string(),
            args: vec![
                "-u".to_string(),
                self.dir
                    .path()
                    .join("mock_agent.py")
                    .to_string_lossy()
                    .into_owned(),
            ],
            env,
            allow_terminal: false,
        }
    }

    fn prompts_file(&self) -> std::path::PathBuf {
        self.dir.path().join("prompts")
    }

    fn prompts(&self) -> String {
        // Windows text-mode writes may turn LF into CRLF, so compare
        // the text, not the line endings.
        std::fs::read_to_string(self.prompts_file())
            .map(|text| text.replace("\r\n", "\n"))
            .unwrap_or_default()
    }

    /// The reply, and the tier the session named, if a session started.
    async fn run(&self, usage: Offer) -> (Result<String, Failure>, Option<Option<String>>) {
        let mut session_tier = None;
        let reply = run_usage_command(
            &self.acp,
            "work",
            self.config(usage),
            self.dir.path().join("scratch"),
            &mut session_tier,
        )
        .await;
        (reply, session_tier)
    }

    fn no_agent_left(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.acp.list_agents().is_empty() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_check_sends_usage_returns_the_agent_text_and_stops_the_agent() {
    let Some(mock) = Mock::new().await else {
        return;
    };

    let (reply, session_tier) = mock.run(Offer::First).await;

    assert_eq!(reply.expect("the check reads the reply"), REPLY);
    assert_eq!(session_tier, Some(Some("Google AI Pro".to_string())));
    assert_eq!(mock.prompts(), "/usage\n");
    assert!(mock.dir.path().join("scratch").is_dir());
    assert!(mock.no_agent_left());
    assert!(!mock
        .acp
        .antigravity_agent()
        .usage_book()
        .entries
        .lock()
        .contains_key("work"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_without_usage_is_never_prompted_and_is_stopped() {
    let Some(mock) = Mock::new().await else {
        return;
    };

    let failure = mock
        .run(Offer::Never)
        .await
        .0
        .expect_err("no /usage command");

    assert!(matches!(failure, Failure::NoUsageCommand), "{failure:?}");
    assert_eq!(mock.prompts(), "");
    assert!(mock.no_agent_left());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usage_listed_in_a_later_commands_update_still_counts() {
    let Some(mock) = Mock::new().await else {
        return;
    };

    let reply = mock.run(Offer::SecondUpdate).await.0;

    assert_eq!(reply.expect("the check reads the reply"), REPLY);
    assert_eq!(mock.prompts(), "/usage\n");
    assert!(mock.no_agent_left());
}
