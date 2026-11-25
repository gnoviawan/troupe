use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use parking_lot::Mutex;
use tokio::sync::watch;

use super::report::parse_usage_reply;
use crate::acp::antigravity_accounts::usage::{AntigravityAccountUsage, AntigravityUsageStatus};
use crate::acp::antigravity_agent::ANTIGRAVITY_ACP_CONFIG_ID;
use crate::acp::config::{AgentConfig, AgentId};
use crate::acp::events;
use crate::acp::manager::{AcpManager, SessionCreationContext, ACP_AUTH_REQUIRED_PREFIX};
use crate::acp::session::ToolAllowlist;
use crate::web::sink::{AcpEvent, EventSink};

const USAGE_COMMAND: &str = "usage";
const SCRATCH_DIR: &str = "antigravity-usage-check";
const COMMANDS_WAIT: Duration = Duration::from_secs(10);
const CHECK_BUDGET: Duration = Duration::from_secs(60);
const MAX_REPLY_BYTES: usize = 64 * 1024;

const NO_USAGE_COMMAND: &str =
    "This version of Antigravity has no /usage command. Update the Antigravity agent, then check again.";

pub async fn check_antigravity_usage(
    acp: &Arc<AcpManager>,
    account_id: &str,
    config: AgentConfig,
) -> Result<AntigravityAccountUsage, String> {
    if config.config_id.as_deref() != Some(ANTIGRAVITY_ACP_CONFIG_ID) {
        return Err("Usage checks run only on the Antigravity agent".to_string());
    }
    let service = acp.antigravity_agent();
    if !service.has_account(account_id)? {
        return Err("That Antigravity account does not exist".to_string());
    }
    let scratch = service
        .state_dir()
        .map(|dir| dir.join(SCRATCH_DIR))
        .ok_or_else(|| "Antigravity accounts are not available on this host".to_string())?;
    let run = run_check(Arc::clone(acp), account_id.to_string(), config, scratch);
    Ok(service.usage_book().join_or_start(account_id, run).await)
}

async fn run_check(
    acp: Arc<AcpManager>,
    account_id: String,
    config: AgentConfig,
    scratch: PathBuf,
) -> AntigravityAccountUsage {
    let began = Instant::now();
    log::info!("[antigravity-usage] check started");
    let outcome = match signed_in(&acp, &account_id).await {
        Some(true) => {
            match tokio::time::timeout(
                CHECK_BUDGET,
                run_usage_command(&acp, &account_id, config, scratch),
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(_) => Err(Failure::TimedOut),
            }
        }
        Some(false) => Err(Failure::SignedOut(
            "This account is not signed in. Sign in to it, then check again.".to_string(),
        )),
        None => Err(Failure::Failed(
            "The sign-in of this account could not be read.".to_string(),
        )),
    };
    let now = chrono::Utc::now().timestamp_millis();
    let usage = match outcome {
        Ok(reply) => parse_usage_reply(&reply).into_usage(&account_id, now),
        Err(failure) => failure.into_usage(&account_id, now),
    };
    log::info!(
        "[antigravity-usage] check finished: status={:?}, {} window(s) in {} ms",
        usage.status,
        usage.windows.len(),
        began.elapsed().as_millis()
    );
    usage
}

async fn signed_in(acp: &AcpManager, account_id: &str) -> Option<bool> {
    let snapshot = acp.antigravity_agent().profile_list().await.ok()?;
    snapshot
        .profiles
        .iter()
        .find(|row| row.id == account_id)?
        .signed_in
}

#[derive(Debug)]
enum Failure {
    SignedOut(String),
    NoUsageCommand,
    TimedOut,
    Failed(String),
}

impl Failure {
    fn from_error(error: String) -> Self {
        if let Some(message) = crate::acp::antigravity_accounts::strip_signed_out_tag(&error) {
            return Self::SignedOut(message.to_string());
        }
        if error.starts_with(ACP_AUTH_REQUIRED_PREFIX) {
            return Self::SignedOut("Antigravity asks this account to sign in again.".to_string());
        }
        Self::Failed(error)
    }

    fn into_usage(self, account_id: &str, checked_at: i64) -> AntigravityAccountUsage {
        let (status, message) = match self {
            // Readiness comes from the login probe only, so a signed-out
            // answer is a message on this check, not a login state.
            Self::SignedOut(message) => (AntigravityUsageStatus::Error, message),
            Self::NoUsageCommand => (
                AntigravityUsageStatus::Unavailable,
                NO_USAGE_COMMAND.to_string(),
            ),
            Self::TimedOut => (
                AntigravityUsageStatus::Error,
                format!(
                    "Antigravity did not answer /usage within {} seconds.",
                    CHECK_BUDGET.as_secs()
                ),
            ),
            Self::Failed(error) => (
                AntigravityUsageStatus::Error,
                format!("Could not run Antigravity's /usage: {error}"),
            ),
        };
        log::warn!("[antigravity-usage] check did not read quota: {status:?}");
        AntigravityAccountUsage::status_only(account_id, status, message, checked_at)
    }
}

async fn run_usage_command(
    acp: &Arc<AcpManager>,
    account_id: &str,
    config: AgentConfig,
    scratch: PathBuf,
) -> Result<String, Failure> {
    std::fs::create_dir_all(&scratch).map_err(|error| {
        Failure::Failed(format!(
            "could not create the check folder: {:?}",
            error.kind()
        ))
    })?;
    let cwd = scratch
        .to_str()
        .ok_or_else(|| Failure::Failed("the check folder path is not Unicode".to_string()))?
        .to_string();
    let reader = Arc::new(UsageReader::default());
    let spawned = acp
        .spawn_private(
            config,
            None,
            &crate::acp::claude_agent::LaunchChat::New {
                account_id: account_id.to_string(),
            },
            vec![reader.clone()],
        )
        .await
        .map_err(Failure::from_error)?;
    let agent = AgentGuard {
        acp: Arc::clone(acp),
        agent_id: spawned.agent_id,
    };
    let created = acp
        .new_session_with_context(
            &agent.agent_id,
            cwd,
            Vec::new(),
            SessionCreationContext {
                ephemeral: true,
                tool_allowlist: Some(ToolAllowlist::default()),
                ..SessionCreationContext::default()
            },
        )
        .await
        .map_err(Failure::from_error)?;
    acp.antigravity_agent()
        .usage_book()
        .record_tier(account_id, created.antigravity_tier.clone());
    if !reader.offers(USAGE_COMMAND, COMMANDS_WAIT).await {
        log::info!("[antigravity-usage] the agent did not advertise /usage");
        return Err(Failure::NoUsageCommand);
    }
    let prompt = vec![ContentBlock::Text(TextContent::new(format!(
        "/{USAGE_COMMAND}"
    )))];
    let started = acp
        .start_prompt(&agent.agent_id, created.session_id, prompt, None, None)
        .await
        .map_err(Failure::from_error)?;
    acp.wait_prompt(started)
        .await
        .map_err(Failure::from_error)?;
    let reply = reader.take_text();
    agent.stop().await;
    Ok(reply)
}

struct AgentGuard {
    acp: Arc<AcpManager>,
    agent_id: AgentId,
}

impl AgentGuard {
    async fn stop(&self) {
        if let Err(error) = self.acp.kill(&self.agent_id).await {
            log::warn!("[antigravity-usage] could not stop the check agent: {error}");
        }
    }
}

impl Drop for AgentGuard {
    fn drop(&mut self) {
        self.acp.kill_detached(&self.agent_id);
    }
}

struct UsageReader {
    commands: watch::Sender<Vec<String>>,
    reply: Mutex<Reply>,
}

#[derive(Default)]
struct Reply {
    text: String,
    /// A chunk did not fit; every later chunk is dropped too, so the text
    /// stays a prefix of the reply.
    full: bool,
}

impl Default for UsageReader {
    fn default() -> Self {
        Self {
            commands: watch::Sender::new(Vec::new()),
            reply: Mutex::new(Reply::default()),
        }
    }
}

impl UsageReader {
    /// Whether an `available_commands_update` within `wait` lists `command`.
    /// The agent can send several updates; any of them counts.
    async fn offers(&self, command: &str, wait: Duration) -> bool {
        let mut commands = self.commands.subscribe();
        let listed = commands.wait_for(|names| names.iter().any(|name| name == command));
        let answer = tokio::time::timeout(wait, listed).await;
        matches!(answer, Ok(Ok(_)))
    }

    fn take_text(&self) -> String {
        std::mem::take(&mut self.reply.lock().text)
    }
}

impl EventSink for UsageReader {
    fn emit(&self, event: &AcpEvent) {
        let payload = &event.payload;
        match event.type_ {
            events::EVENT_COMMANDS_UPDATE => {
                let names = payload["availableCommands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|command| command["name"].as_str().map(str::to_string))
                    .collect();
                self.commands.send_replace(names);
            }
            events::EVENT_MESSAGE_CHUNK
                if payload["role"] == "agent"
                    && payload["content"]["type"] == "text"
                    && payload.get("parentToolCallId").is_none() =>
            {
                if let Some(chunk) = payload["content"]["text"].as_str() {
                    let mut reply = self.reply.lock();
                    if reply.full || reply.text.len() + chunk.len() > MAX_REPLY_BYTES {
                        reply.full = true;
                    } else {
                        reply.text.push_str(chunk);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
