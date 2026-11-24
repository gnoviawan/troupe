//! The agent roster on web (`agent-roster-watch.ts`) and the agent browser
//! tabs (`browser-agent-tabs.ts`).
//!
//! The host's `agents` watch carries the roster events the desktop gets one
//! by one. Its full state, sent on every subscribe, is reconciled against the
//! agents the model holds; a delta goes to the handler the desktop event
//! reaches. The watch also carries the browser policy and automation hints,
//! which the model hands on as effects.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::agents::{AgentDisconnectedEvent, AgentSpawnedEvent};
use super::js::{string_of, truthy};
use super::{Effect, Env, State};

const WATCH_SOURCE: &str = "acp-store.agentRosterWatch";
const TAB_SOURCE: &str = "acp-store:_onBrowserAgentTab";

/// `acp:browser_agent_tab`. The payload arrives unchecked.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentTabEvent {
    #[serde(default)]
    pub action: Value,
    #[serde(default)]
    pub tab_id: Value,
    #[serde(default)]
    pub url: Value,
    #[serde(default)]
    pub session_id: Value,
}

/// `acp:browser_tab_closed`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBrowserTabClosedEvent {
    #[serde(default)]
    pub tab_id: Value,
}

fn non_empty(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

impl State {
    /// `onState` of the agents watch: an agent the host lists that the model
    /// lacks counts as spawned, a connected agent it no longer lists as
    /// disconnected. A resubscribe may have lost hints, so both hint views
    /// re-read.
    pub(crate) fn on_agents_watch_state(
        &mut self,
        summaries: Value,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        if let Value::Array(summaries) = summaries {
            // The TypeScript reads the roster once, before the reconcile.
            let held: Vec<(String, bool)> = self
                .agents
                .iter()
                .map(|(id, agent)| (id.clone(), truthy(agent)))
                .collect();
            let connected: Vec<String> = held
                .iter()
                .filter(|(id, _)| self.agent_status.get(id) == Some(&json!("connected")))
                .map(|(id, _)| id.clone())
                .collect();
            let mut listed = Vec::new();
            for summary in &summaries {
                let Some(id) = summary.get("id").and_then(Value::as_str) else {
                    continue;
                };
                listed.push(id.to_owned());
                if held.iter().any(|(held, known)| held == id && *known) {
                    continue;
                }
                // The roster carries no auth methods: the spawn reply fills
                // them in.
                self.on_agent_spawned(AgentSpawnedEvent {
                    agent_id: id.to_owned(),
                    capabilities: summary.get("capabilities").cloned(),
                    auth_methods: None,
                    host_auth_ready: None,
                });
            }
            for agent_id in connected {
                if listed.contains(&agent_id) {
                    continue;
                }
                let event = AgentDisconnectedEvent {
                    agent_id,
                    reason: "killed".into(),
                };
                self.on_agent_disconnected(event, env, fx);
            }
        }
        fx.push(Effect::RequestBrowserPolicyResync);
        fx.push(Effect::RequestAutomationResync);
    }

    /// `onDelta` of the agents watch.
    pub(crate) fn on_agents_watch_delta(
        &mut self,
        delta: Value,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let kind = delta.get("event");
        let payload = delta.get("payload").cloned().unwrap_or(Value::Null);
        match kind.and_then(Value::as_str) {
            Some("browser_policy_changed") => {
                return fx.push(Effect::PublishBrowserPolicyChange { payload });
            }
            Some("automation_changed") => {
                return fx.push(Effect::PublishAutomationChange { payload });
            }
            Some("automation_resync") => {
                let missed = string_of(payload.get("missed"));
                fx.push(Effect::warn(
                    WATCH_SOURCE,
                    format!("host dropped {missed} automation change(s); re-reading automations"),
                ));
                return fx.push(Effect::RequestAutomationResync);
            }
            _ => {}
        }
        if !payload.get("agentId").is_some_and(Value::is_string) {
            let kind = string_of(kind);
            return fx.push(Effect::warn(
                WATCH_SOURCE,
                format!("ignored an agents watch delta without an agent id ({kind})"),
            ));
        }
        match kind.and_then(Value::as_str) {
            Some("agent_spawned") => {
                if let Some(event) = parse(&payload, fx) {
                    self.on_agent_spawned(event);
                }
            }
            Some(kind @ ("agent_crashed" | "agent_error")) => {
                let Some(event) = parse::<super::agents::AgentFailureEvent>(&payload, fx) else {
                    return;
                };
                let message = match event.message.as_str() {
                    "" if kind == "agent_crashed" => "Agent crashed".to_owned(),
                    "" => "Agent error".to_owned(),
                    message => message.to_owned(),
                };
                if kind == "agent_crashed" {
                    self.on_agent_crashed(event, env, fx);
                } else {
                    self.on_agent_error(event, env, fx);
                }
                fx.push(Effect::Toast {
                    level: "error".into(),
                    message,
                });
            }
            Some("agent_disconnected") => {
                if let Some(event) = parse(&payload, fx) {
                    self.on_agent_disconnected(event, env, fx);
                }
            }
            Some("browser_open_request") => {
                if let Some(event) = parse(&payload, fx) {
                    self.on_browser_open_request(event, fx);
                }
            }
            Some("agent_sign_in_request") => {
                if let Some(event) = parse(&payload, fx) {
                    self.on_agent_sign_in_request(event, fx);
                }
            }
            Some("agent_sign_in_resolved") => {
                if let Some(event) = parse(&payload, fx) {
                    self.on_agent_sign_in_resolved(event);
                }
            }
            _ => {
                let kind = string_of(kind);
                fx.push(Effect::warn(
                    WATCH_SOURCE,
                    format!("ignored an agents watch delta of an unknown kind: {kind}"),
                ));
            }
        }
    }

    /// `_onBrowserAgentTab`: an open tracks the tab's session for the Stop
    /// indicator, a close forgets it. Only the desktop shows the tab. An
    /// open the installed history covers is a replay of a tab whose
    /// automation session is gone, so it is skipped.
    pub(crate) fn on_browser_agent_tab(
        &mut self,
        event: BrowserAgentTabEvent,
        seq: Option<f64>,
        env: &Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(tab_id) = non_empty(&event.tab_id).map(str::to_owned) else {
            return;
        };
        let action = event.action.as_str();
        let session_id = event.session_id.as_str().unwrap_or_default();
        if action == Some("open") && self.book.history_covered_tool(session_id, seq) {
            let seq = string_of(seq.map(|seq| json!(seq)).as_ref());
            return fx.push(Effect::info(
                TAB_SOURCE,
                format!(
                    "[acp] replayed agent browser tab open skipped (tabId={tab_id}, seq={seq})"
                ),
            ));
        }
        match action {
            Some("open") if !session_id.is_empty() => fx.push(Effect::TrackAgentTab {
                tab_id: tab_id.clone(),
                session_id: session_id.to_owned(),
            }),
            Some("close") => fx.push(Effect::UntrackAgentTab {
                tab_id: tab_id.clone(),
            }),
            _ => {}
        }
        if !env.desktop {
            return;
        }
        match action {
            Some("open") => {
                let url = event
                    .url
                    .as_str()
                    .filter(|url| {
                        let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
                        lower.starts_with("https://") || lower.starts_with("http://")
                    })
                    .map(str::to_owned);
                fx.push(Effect::OpenAgentBrowserTab {
                    tab_id: tab_id.clone(),
                    url,
                });
                fx.push(Effect::info(
                    TAB_SOURCE,
                    format!("[acp] agent browser tab opened (tabId={tab_id})"),
                ));
            }
            Some("close") => fx.push(Effect::CloseAgentBrowserTab { tab_id }),
            _ => {}
        }
    }

    /// `_onBrowserTabClosed`: the host dropped the tab; its Stop ownership
    /// and, on the desktop, its workspace tab go too.
    pub(crate) fn on_browser_tab_closed(
        &mut self,
        event: AgentBrowserTabClosedEvent,
        env: &Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(tab_id) = non_empty(&event.tab_id).map(str::to_owned) else {
            return;
        };
        fx.push(Effect::UntrackAgentTab {
            tab_id: tab_id.clone(),
        });
        if env.desktop {
            fx.push(Effect::CloseAgentBrowserTab { tab_id });
        }
    }
}

/// A delta's payload as the handler's event. The TypeScript hands any shape
/// on; the model logs and drops one its handler cannot read.
fn parse<T: DeserializeOwned>(payload: &Value, fx: &mut Vec<Effect>) -> Option<T> {
    match serde_json::from_value(payload.clone()) {
        Ok(event) => Some(event),
        Err(error) => {
            fx.push(Effect::warn(
                WATCH_SOURCE,
                format!("ignored a malformed agents watch delta: {error}"),
            ));
            None
        }
    }
}
