//! The agent roster: an agent process registering, failing a turn, crashing
//! or going away, and the sign-in prompts it raises. Port of the event
//! handlers of `slices/agent.ts` and the error and rate-limit helpers they
//! call (`parseAcpErrorDetail`, `describeProviderFailure`, `rate-limit.ts`).

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use super::turn::finalize_streaming;
use super::types::{Object, SessionStatus, Usage};
use super::{Effect, Env, State, js};

/// JSON-RPC `code` of an agent that wants the user to sign in.
const AUTH_REQUIRED_RPC_CODE: f64 = -32000.0;
/// The early browser-open buffer holds this many agents.
const MAX_EARLY_BROWSER_OPENS: usize = 32;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpawnedEvent {
    pub agent_id: String,
    #[serde(default)]
    pub capabilities: Option<Value>,
    #[serde(default)]
    pub auth_methods: Option<Value>,
    #[serde(default)]
    pub host_auth_ready: Option<Value>,
}

/// `agent_error` and `agent_crashed`: the session is absent when the
/// failure is the whole process's.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFailureEvent {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub code: Option<Value>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub provider_code: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDisconnectedEvent {
    pub agent_id: String,
    pub reason: String,
}

/// The payloads below arrive unchecked; the handlers guard each field.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOpenRequestEvent {
    #[serde(default)]
    pub agent_id: Value,
    #[serde(default)]
    pub url: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSignInRequestEvent {
    #[serde(default)]
    pub agent_id: Value,
    #[serde(default)]
    pub request_id: Value,
    #[serde(default)]
    pub url: Value,
    #[serde(default)]
    pub message: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSignInResolvedEvent {
    pub agent_id: String,
    pub request_id: String,
}

/// `parseAcpErrorDetail`: an integer `code`, a non-null `data`, a known
/// provider failure code.
struct ErrorDetail {
    code: Option<f64>,
    data: Option<Value>,
    provider: Option<&'static str>,
}

const PROVIDER_FAILURES: [(&str, &str); 3] = [
    (
        "provider_quota_exceeded",
        "The provider's usage limit was reached. Wait for it to reset or check your plan, then try again.",
    ),
    (
        "provider_session_limit",
        "The provider's session limit was reached. Wait for it to reset, then try again.",
    ),
    (
        "provider_request_rejected",
        "The provider rejected the request. Check the model and account settings, then try again.",
    ),
];

impl ErrorDetail {
    fn of(event: &AgentFailureEvent) -> Self {
        let code = js::finite(event.code.as_ref()).filter(|code| code.fract() == 0.0);
        let data = event.data.clone().filter(|data| !data.is_null());
        let provider = event.provider_code.as_ref().and_then(Value::as_str);
        let provider = PROVIDER_FAILURES
            .iter()
            .map(|(code, _)| *code)
            .find(|code| Some(*code) == provider);
        Self {
            code,
            data,
            provider,
        }
    }

    /// `describeProviderFailure` when the provider named one, else `own`.
    fn describe(&self, own: &str) -> String {
        let Some((code, plain)) = PROVIDER_FAILURES
            .iter()
            .find(|(code, _)| Some(*code) == self.provider)
        else {
            return own.to_owned();
        };
        let trimmed = own.trim();
        if trimmed.is_empty() || trimmed == *code {
            (*plain).to_owned()
        } else {
            format!("{plain} ({trimmed})")
        }
    }

    /// `isAdapterPlanLimit`: a Codex plan or rate limit.
    fn plan_limit(&self) -> bool {
        let Some(Value::Object(data)) = &self.data else {
            return false;
        };
        let kind = data.get("errorKind").and_then(Value::as_str);
        let info = data.get("codexErrorInfo").and_then(Value::as_str);
        matches!(kind, Some("rate_limited" | "quota_exhausted"))
            || matches!(info, Some("usageLimitExceeded" | "rateLimitExceeded"))
    }
}

/// `rejectedRateLimit`.
fn rejected_rate_limit(message: &str) -> Value {
    let lower = message.to_lowercase();
    let kind = if lower.contains("5-hour") {
        json!("five_hour")
    } else if lower.contains("weekly") {
        json!("seven_day")
    } else {
        Value::Null
    };
    json!({
        "status": "rejected",
        "resetsAt": null,
        "kind": kind,
        "percent": null,
        "overageInUse": false,
    })
}

/// `/^https?:\/\//i`, or only `https` when `secure`.
fn web_url(value: &Value, secure: bool) -> Option<&str> {
    let url = value.as_str()?;
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    (lower.starts_with("https://") || (!secure && lower.starts_with("http://"))).then_some(url)
}

impl State {
    pub(crate) fn on_agent_spawned(&mut self, event: AgentSpawnedEvent) {
        let id = event.agent_id;
        let existing = self.agents.get(&id);
        let kept = |key: &str, fallback: Option<Value>| {
            existing
                .and_then(|agent| agent.get(key))
                .filter(|value| !value.is_null())
                .cloned()
                .or(fallback.filter(|value| !value.is_null()))
        };
        let mut agent = Object::new();
        agent.insert("id".into(), json!(id));
        if let Some(capabilities) = kept("capabilities", event.capabilities) {
            agent.insert("capabilities".into(), capabilities);
        }
        let methods = kept("authMethods", event.auth_methods).unwrap_or_else(|| json!([]));
        agent.insert("authMethods".into(), methods);
        if let Some(ready) = kept("hostAuthReady", event.host_auth_ready) {
            agent.insert("hostAuthReady".into(), ready);
        }
        self.agents.insert(id.clone(), Value::Object(agent));
        self.agent_status.insert(id.clone(), json!("connected"));
        if let Some(url) = self.take_early_browser_open(&id) {
            self.pending_sign_in
                .insert(id, json!({ "kind": "browser", "url": url }));
        }
    }

    /// `endPromptTrackingForAgentFailure`: the named session, or every live
    /// one of the agent when it names none or a closed one, stops tracking
    /// its prompt turn.
    fn end_prompt_tracking(&mut self, agent_id: &str, session_id: Option<&str>) {
        let mut ended: BTreeSet<String> = session_id.map(str::to_owned).into_iter().collect();
        let named = session_id.and_then(|id| self.sessions.get(id));
        if named.is_none_or(|session| session.status == SessionStatus::Closed) {
            ended.extend(
                self.sessions
                    .values()
                    .filter(|s| s.agent_id == agent_id && s.status != SessionStatus::Closed)
                    .map(|s| s.id.clone()),
            );
        }
        for id in ended {
            self.book.active_wire_turns.remove(&id);
        }
    }

    fn persist_if_indexed(&self, session_id: Option<&str>, fx: &mut Vec<Effect>) {
        if let Some(id) = session_id
            && self.sessions.contains_key(id)
            && self.indexed(id)
        {
            fx.push(Effect::PersistSession {
                session_id: id.to_owned(),
            });
        }
    }

    /// Mark the agent's live sessions failed: `lastError`, the turn closed,
    /// and `error` status when `dead`.
    fn fail_live_sessions(
        &mut self,
        agent_id: &str,
        message: &str,
        detail: &ErrorDetail,
        dead: bool,
    ) {
        for session in self.sessions.values_mut() {
            if session.agent_id != agent_id || session.status == SessionStatus::Closed {
                continue;
            }
            if dead {
                session.status = SessionStatus::Error;
            }
            session.last_error = Some(message.to_owned());
            session.last_error_code = Some(detail.code);
            session.last_error_data = detail.data.clone().map(Some);
            session.active_turn = false;
            session.open_turn_id = None;
        }
    }

    /// `_onAgentError`: a turn failed. The chat stays live.
    pub(crate) fn on_agent_error(
        &mut self,
        event: AgentFailureEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id.as_deref().filter(|id| !id.is_empty());
        self.end_prompt_tracking(&event.agent_id, session_id);
        let detail = ErrorDetail::of(&event);
        let Some(id) = session_id else {
            self.agent_status
                .insert(event.agent_id.clone(), json!("error"));
            self.fail_live_sessions(&event.agent_id, &event.message, &detail, false);
            return;
        };
        let Some(session) = self
            .sessions
            .get_mut(id)
            .filter(|s| s.status != SessionStatus::Closed)
        else {
            self.persist_if_indexed(Some(id), fx);
            return;
        };
        if detail.code == Some(AUTH_REQUIRED_RPC_CODE) {
            self.agent_status
                .insert(event.agent_id.clone(), json!("error"));
        }
        if (detail.provider == Some("provider_quota_exceeded") || detail.plan_limit())
            && session.agent_id == event.agent_id
        {
            let usage = self.session_usage.entry(id.to_owned()).or_insert(Usage {
                used: 0.0,
                size: 0.0,
                baseline_used: 0.0,
                cost: None,
                rate_limit: None,
                updated_at: None,
                source: "reported".into(),
            });
            usage.rate_limit = Some(rejected_rate_limit(&event.message));
            usage.updated_at = Some(env.now);
        }
        session.last_error = Some(detail.describe(&event.message));
        session.last_error_code = Some(detail.code);
        session.last_error_data = detail.data.clone().map(Some);
        session.active_turn = false;
        session.open_turn_id = None;
        finalize_streaming(self.messages.get_mut(id));
        self.persist_if_indexed(Some(id), fx);
    }

    /// `_onAgentCrashed`: the process died; its chats reopen onto a new one.
    pub(crate) fn on_agent_crashed(
        &mut self,
        event: AgentFailureEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let agent_id = event.agent_id.as_str();
        let session_id = event.session_id.as_deref().filter(|id| !id.is_empty());
        self.end_prompt_tracking(agent_id, session_id);
        let detail = ErrorDetail::of(&event);
        self.agent_status
            .insert(agent_id.to_owned(), json!("error"));
        self.drop_agent_launch_state(agent_id);
        let named = session_id.filter(|id| {
            self.sessions
                .get(*id)
                .is_some_and(|s| s.status != SessionStatus::Closed)
        });
        if let Some(id) = named {
            let session = self.sessions.get_mut(id).expect("checked above");
            session.status = SessionStatus::Error;
            session.last_error = Some(detail.describe(&event.message));
            session.last_error_code = Some(detail.code);
            session.last_error_data = detail.data.clone().map(Some);
            session.active_turn = false;
            session.open_turn_id = None;
            finalize_streaming(self.messages.get_mut(id));
        } else {
            self.fail_live_sessions(agent_id, &event.message, &detail, true);
        }
        let mut crashed = self.agent_session_ids(agent_id);
        if let Some(id) = session_id
            && !crashed.iter().any(|known| known == id)
        {
            crashed.push(id.to_owned());
        }
        self.cancel_running_subagents(&crashed, env.now, fx);
        self.persist_if_indexed(session_id, fx);
    }

    /// `_onAgentDisconnected`: a kill, reap or close drops the agent like a
    /// local kill; a crash keeps its `error` status for a manual restart.
    pub(crate) fn on_agent_disconnected(
        &mut self,
        event: AgentDisconnectedEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let agent_id = event.agent_id.as_str();
        let gone = self.agent_session_ids(agent_id);
        if event.reason == "crashed" {
            self.agent_status
                .insert(agent_id.to_owned(), json!("error"));
        } else {
            self.agent_status.remove(agent_id);
            self.agents.remove(agent_id);
            self.config_to_live_agent
                .retain(|_, live| live.as_str() != Some(agent_id));
        }
        self.drop_agent_launch_state(agent_id);
        let mut retired = Vec::new();
        let mut emptied = Vec::new();
        for (id, session) in &mut self.sessions {
            if session.agent_id != agent_id || session.status == SessionStatus::Closed {
                continue;
            }
            if session.status != SessionStatus::Error {
                session.status = SessionStatus::Closed;
                session.active_turn = false;
                session.open_turn_id = None;
                session.replaying = Some(None);
                session.rest.insert("switching".into(), Value::Null);
            }
            retired.push(id.clone());
            if self.messages.get(id).is_none_or(Vec::is_empty) {
                emptied.push(id.clone());
            }
        }
        let prefix = format!("{agent_id}\0");
        self.discovered_sessions
            .retain(|key, _| !key.starts_with(&prefix));
        self.pending_permissions
            .retain(|_, p| p.agent_id != agent_id);
        self.pending_questions.retain(|_, p| p.agent_id != agent_id);
        self.pending_elicitations
            .retain(|_, p| p.agent_id != agent_id);
        self.clear_pending_sign_in(agent_id);
        self.cancel_running_subagents(&gone, env.now, fx);
        for id in retired {
            fx.push(Effect::PersistSession { session_id: id });
        }
        for id in emptied {
            fx.push(Effect::DropSessionTranscript { session_id: id });
        }
    }

    /// The agent's sessions, closed ones included.
    fn agent_session_ids(&self, agent_id: &str) -> Vec<String> {
        self.sessions
            .values()
            .filter(|s| s.agent_id == agent_id)
            .map(|s| s.id.clone())
            .collect()
    }

    /// A gone process can land no launch and fill no warm slot.
    fn drop_agent_launch_state(&mut self, agent_id: &str) {
        let owned = |id: &String| {
            self.sessions
                .get(id)
                .is_some_and(|s| s.agent_id == agent_id)
        };
        let launching: Vec<String> = self
            .launching_session_ids
            .keys()
            .filter(|id| owned(id))
            .cloned()
            .collect();
        let prepared: Vec<String> = self
            .prepared_sessions
            .iter()
            .filter(|(_, id)| owned(id))
            .map(|(key, _)| key.clone())
            .collect();
        for id in launching {
            self.launching_session_ids.remove(&id);
        }
        for key in prepared {
            self.prepared_sessions.remove(&key);
        }
    }

    /// `_onBrowserOpenRequest`: the host's browser-open shim caught the URL
    /// an agent tried to open. One for an agent not registered yet waits in
    /// a bounded buffer for its `agent_spawned`.
    pub(crate) fn on_browser_open_request(
        &mut self,
        event: BrowserOpenRequestEvent,
        fx: &mut Vec<Effect>,
    ) {
        let Some(agent_id) = event.agent_id.as_str().filter(|id| !id.is_empty()) else {
            return;
        };
        let Some(url) = web_url(&event.url, false) else {
            return;
        };
        if !self.agents.contains_key(agent_id) {
            let early = &mut self.book.early_browser_opens;
            while early.len() >= MAX_EARLY_BROWSER_OPENS {
                let Some((oldest, _)) = early.pop_front() else {
                    break;
                };
                fx.push(Effect::warn(
                    "acp-store:_onBrowserOpenRequest",
                    format!(
                        "[acp] early browser-open buffer evicted oldest entry (agentId={oldest}, size={})",
                        early.len()
                    ),
                ));
            }
            match early.iter_mut().find(|(id, _)| id == agent_id) {
                Some(entry) => entry.1 = url.to_owned(),
                None => early.push_back((agent_id.to_owned(), url.to_owned())),
            }
            fx.push(Effect::info(
                "acp-store:_onBrowserOpenRequest",
                format!(
                    "[acp] browser-open request buffered pending agent registration (agentId={agent_id})"
                ),
            ));
            return;
        }
        if self
            .auto_open_browser_url
            .get(agent_id)
            .and_then(Value::as_str)
            != Some(url)
        {
            self.auto_open_browser_url.remove(agent_id);
        }
        self.pending_sign_in.insert(
            agent_id.to_owned(),
            json!({ "kind": "browser", "url": url }),
        );
    }

    /// `_onAgentSignInRequest`: the agent asks the user to sign in at an
    /// `https` page.
    pub(crate) fn on_agent_sign_in_request(
        &mut self,
        event: AgentSignInRequestEvent,
        fx: &mut Vec<Effect>,
    ) {
        let Some(agent_id) = event
            .agent_id
            .as_str()
            .filter(|id| self.agents.contains_key(*id))
        else {
            return;
        };
        let (Some(request_id), Some(message)) = (event.request_id.as_str(), event.message.as_str())
        else {
            return;
        };
        let Some(url) = web_url(&event.url, true) else {
            return;
        };
        fx.push(Effect::info(
            "acp-store:_onAgentSignInRequest",
            format!("[acp] sign-in prompt received (agentId={agent_id}, requestId={request_id})"),
        ));
        self.pending_sign_in.insert(
            agent_id.to_owned(),
            json!({ "kind": "elicitation", "requestId": request_id, "url": url, "message": message }),
        );
    }

    pub(crate) fn on_agent_sign_in_resolved(&mut self, event: AgentSignInResolvedEvent) {
        let prompt = self.pending_sign_in.get(&event.agent_id);
        let elicitation = prompt.and_then(|p| p.get("kind")) == Some(&json!("elicitation"));
        let same = prompt.and_then(|p| p.get("requestId")) == Some(&json!(event.request_id));
        if elicitation && same {
            self.clear_pending_sign_in(&event.agent_id);
        }
    }

    fn take_early_browser_open(&mut self, agent_id: &str) -> Option<String> {
        let early = &mut self.book.early_browser_opens;
        let index = early.iter().position(|(id, _)| id == agent_id)?;
        early.remove(index).map(|(_, url)| url)
    }

    /// `clearPendingSignIn`.
    pub(crate) fn clear_pending_sign_in(&mut self, agent_id: &str) {
        self.take_early_browser_open(agent_id);
        self.pending_sign_in.remove(agent_id);
        self.auto_open_browser_url.remove(agent_id);
    }
}
