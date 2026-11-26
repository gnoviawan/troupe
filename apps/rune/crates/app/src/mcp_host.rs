//! The acp model's MCP effects on the host, as `acp-mcp-persistence.ts`, `acp-mcp-probe.ts` and
//! the web half of `acp-api.ts` run them: the registry under its shared key, the probe, and the
//! OAuth sign-in with its status poll. Each answer goes back to the model as `McpCommandDone`,
//! `McpCommandFailed` or `McpOAuthAuthUrl`.
//!
//! A write the Settings page asked for carries what the page does once it lands: React awaits
//! the store's promise and then toasts, closes its dialog, or offers Undo.

use client::http::{HttpError, HttpRequest, Method};
use client::ws::Command;
use model::acp::mcp::{McpError, TokenCheck};
use model::acp::{Effect, Event};
use model::persistence;
use model::settings::mcp::{self as rules, KEY};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType, WsWatchName};

use crate::State;
use crate::host::Fetch;
use crate::settings_mcp::Action;
use crate::sync::{Pending, stored_value};

/// `POLL_INTERVAL_MS` and `POLL_TIMEOUT_MS` of the web OAuth flow, in seconds.
const POLL_EVERY: f64 = 1.0;
const POLL_FOR: f64 = 300.0;

/// A web sign-in waiting for its token: `startMcpOAuth`'s poll of `/mcp-servers/oauth/status`.
#[derive(Clone, Debug, PartialEq)]
pub struct Poll {
    start: Effect,
    project_id: String,
    url: String,
    /// The token generation the start saw; only a newer token finishes the flow.
    generation: Option<u64>,
    deadline: f64,
    next: f64,
    asking: bool,
}

/// What a refusal the model raises while it reads an answer shows instead of the composer's
/// "Could not send your message".
#[derive(Clone, Debug, PartialEq)]
pub enum OnReject {
    Toast(String),
    Quiet,
}

fn error_of(e: HttpError) -> McpError {
    match e {
        HttpError::Refused { code, message, .. } => McpError { message, code: Some(code) },
        other => McpError { message: other.to_string(), code: None },
    }
}

fn ws_error(e: WsError) -> McpError {
    McpError { message: e.message, code: None }
}

/// `oauthFailureMessage`.
pub(crate) fn oauth_failure_message(code: Option<&str>) -> &'static str {
    match code {
        Some("MCP_UNSAFE_URL") => "This server's sign-in link isn't a secure https link, so Troupe didn't open it.",
        Some("OAUTH_DISCOVERY_FAILED") => "Could not discover OAuth settings for this MCP server. Check the server URL and try again.",
        Some("OAUTH_REGISTRATION_FAILED") => "This MCP server rejected OAuth client registration or does not support dynamic client registration.",
        Some("OAUTH_DENIED") => "Sign-in was cancelled or denied by the authorization server.",
        Some("OAUTH_EXCHANGE_FAILED") => "Authorization completed, but exchanging the token failed. Try connecting again.",
        Some("OAUTH_TIMEOUT") => "Sign-in timed out before authorization completed. Try connecting again.",
        Some("UNAUTHORIZED_SERVER_URL") => "This MCP server is not enabled in the host server list. Save or enable it, then try again.",
        _ => "OAuth flow failed. Check your browser for the authorization page.",
    }
}

impl State {
    /// Runs an MCP effect; any other effect comes back.
    pub(crate) fn run_mcp_effect(&mut self, effect: Effect) -> Option<Effect> {
        match &effect {
            Effect::LoadMcpServers { .. } => {
                self.call(WsRequestType::StoreRead, json!({ "key": KEY }), Pending::Mcp(Box::new(effect)));
            }
            Effect::SaveMcpServers { servers } => {
                let action = self.mcp_ui.actions.pop_front();
                let Some(base) = self.mcp_ui.revision else {
                    let error = McpError { message: "The MCP server list has not loaded yet".into(), code: None };
                    self.mcp_answer(effect, Err(error), action);
                    return None;
                };
                let list = rules::normalize_registry(&Value::Array(servers.iter().cloned().map(Value::Object).collect()));
                self.mcp_ui.saving = true;
                let payload = json!({ "key": KEY, "value": persistence::stored(json!(list)), "baseRevision": base });
                self.call(WsRequestType::StoreWrite, payload, Pending::McpSave(Box::new(effect), action));
            }
            Effect::WatchSharedMcpServers => {
                self.send(Command::Watch { watch: WsWatchName::Store, scope: KEY.to_owned() });
            }
            Effect::ProbeMcpServer { config, .. } => {
                let body = Value::Object(config.clone());
                self.mcp_post("/mcp-servers/probe", body, Fetch::Mcp(Box::new(effect)));
            }
            Effect::StartMcpOAuth { project_id, url, .. } => {
                let body = json!({ "projectId": project_id, "serverUrl": url });
                self.mcp_post("/mcp-servers/oauth/start", body, Fetch::Mcp(Box::new(effect)));
            }
            Effect::AbortMcpOAuth { url } => self.mcp_ui.polls.retain(|p| p.url != *url),
            Effect::CompleteMcpOAuth { project_id, callback_url, .. } => {
                let body = json!({ "projectId": project_id, "callbackUrl": callback_url });
                self.mcp_post("/mcp-servers/oauth/complete", body, Fetch::Mcp(Box::new(effect)));
            }
            Effect::HasMcpOAuthToken { project_id, url, .. } => {
                let body = json!({ "projectId": project_id, "serverUrl": url });
                self.mcp_post("/mcp-servers/oauth/status", body, Fetch::Mcp(Box::new(effect)));
            }
            Effect::DisconnectMcpOAuth { project_id, url, .. } => {
                let body = json!({ "projectId": project_id, "serverUrl": url });
                self.mcp_post("/mcp-servers/oauth/disconnect", body, Fetch::Mcp(Box::new(effect)));
            }
            _ => return Some(effect),
        }
        None
    }

    fn mcp_post(&mut self, path: &str, body: Value, tag: Fetch) {
        #[cfg(test)]
        self.mcp_ui.posted.push((path.to_owned(), body.clone()));
        let request = HttpRequest { method: Method::Post, path: path.to_owned(), body: Some(body), command_id: None };
        if let Some(host) = &mut self.host {
            host.request(request, tag);
        }
    }

    /// Steps the model with an MCP answer; a refusal it raises shows as `on_reject`.
    fn mcp_step(&mut self, event: Event, on_reject: OnReject) {
        self.mcp_ui.rejection = Some(on_reject);
        self.acp_step(event);
        self.mcp_ui.rejection = None;
    }

    /// A refusal the model raised while it read an MCP answer: true when it was shown here.
    pub(crate) fn mcp_rejected(&mut self, message: &str) -> bool {
        let Some(on_reject) = self.mcp_ui.rejection.take() else { return false };
        log::warn!("[settings.McpServersSettings] {message}");
        if let OnReject::Toast(text) = on_reject {
            self.toasts.push(self.now, ui::overlay::ToastKind::Error, &text, None);
        }
        true
    }

    /// The store's answer to a registry read.
    pub(crate) fn mcp_loaded(&mut self, effect: Effect, result: Result<Option<Value>, WsError>) {
        let outcome = match result {
            Ok(reply) => {
                let revision = reply.as_ref().and_then(|r| r.get("revision")).and_then(Value::as_u64).unwrap_or(0);
                self.mcp_ui.revision = Some(revision);
                let raw = stored_value(reply).unwrap_or(Value::Null);
                let (list, dropped) = rules::read_registry(&raw);
                if dropped {
                    log::info!("[acp-mcp-persistence.dropLegacyCanvasMcpEntries] legacy canvas-mcp-* entries skipped; the next save drops them");
                }
                Ok(json!(list))
            }
            Err(e) => Err(ws_error(e)),
        };
        self.mcp_answer(effect, outcome, None);
    }

    /// The store's answer to a registry write.
    pub(crate) fn mcp_saved(&mut self, effect: Effect, action: Option<Action>, result: Result<Option<Value>, WsError>) {
        self.mcp_ui.saving = false;
        let merged = match &result {
            Ok(reply) => {
                if let Some(revision) = reply.as_ref().and_then(|r| r.get("revision")).and_then(Value::as_u64) {
                    self.mcp_ui.revision = Some(revision);
                }
                reply.as_ref().filter(|r| r.get("merged").and_then(Value::as_bool) == Some(true)).and_then(|r| r.get("value")).cloned().map(persistence::unwrap_stored)
            }
            Err(_) => None,
        };
        let outcome = result.map(|_| Value::Null).map_err(ws_error);
        self.mcp_answer(effect, outcome, action);
        if let Some(data) = merged {
            // The host kept entries another device wrote since our read: show them.
            self.acp_step(Event::McpSharedChanged(json!({ "exists": true, "data": data })));
            self.mcp_after();
        }
    }

    /// A watched change of the shared registry; this client's own write settles it instead.
    pub(crate) fn mcp_watched(&mut self, state: &Value) {
        if self.mcp_ui.saving {
            return;
        }
        let exists = state.get("exists").and_then(Value::as_bool) == Some(true);
        let data = state.get("value").cloned().map(persistence::unwrap_stored).unwrap_or(Value::Null);
        self.acp_step(Event::McpSharedChanged(json!({ "exists": exists, "data": data })));
        self.mcp_after();
    }

    /// An HTTP answer to an MCP effect.
    pub(crate) fn mcp_fetched(&mut self, effect: Effect, result: Result<Value, HttpError>) {
        match &effect {
            Effect::ProbeMcpServer { server, .. } => {
                let reply = result.unwrap_or_else(|e| {
                    let name = server.get("name").and_then(Value::as_str).unwrap_or_default();
                    log::warn!("[acp-mcp-probe.probeMcpServer] MCP probe transport failed for server '{name}' ({e})");
                    let detail = match e {
                        HttpError::Refused { message, code, .. } => if message.is_empty() { code } else { message },
                        other => other.to_string(),
                    };
                    json!({ "status": "disconnected", "tools": [], "error": detail })
                });
                self.mcp_answer(effect, Ok(reply), None);
            }
            Effect::StartMcpOAuth { project_id, url, .. } => match result {
                Ok(reply) => {
                    let auth_url = reply["authUrl"].as_str().unwrap_or_default().to_owned();
                    let redirect = reply["redirectUri"].as_str().unwrap_or_default().to_owned();
                    self.mcp_ui.polls.retain(|p| p.url != *url);
                    self.mcp_ui.polls.push(Poll {
                        start: effect.clone(),
                        project_id: project_id.clone(),
                        url: url.clone(),
                        generation: reply["tokenGeneration"].as_u64(),
                        deadline: self.now + POLL_FOR,
                        next: self.now + POLL_EVERY,
                        asking: false,
                    });
                    self.acp_step(Event::McpOAuthAuthUrl(effect, auth_url, redirect));
                    self.mcp_after();
                }
                Err(e) => self.mcp_answer(effect, Err(error_of(e)), None),
            },
            Effect::HasMcpOAuthToken { .. } => {
                let outcome = result.map(|r| r.get("hasToken").cloned().unwrap_or(json!(false))).map_err(error_of);
                self.mcp_answer(effect, outcome, None);
            }
            _ => self.mcp_answer(effect, result.map_err(error_of), None),
        }
    }

    /// Hands an answer to the model, with the toast, dialog close or Undo the page waits for.
    fn mcp_answer(&mut self, effect: Effect, outcome: Result<Value, McpError>, action: Option<Action>) {
        let on_reject = match (&effect, &outcome) {
            (Effect::SaveMcpServers { .. }, _) if matches!(action, Some(Action::Install)) => OnReject::Quiet,
            (Effect::SaveMcpServers { .. }, _) => OnReject::Toast(action.as_ref().map_or("Could not save the MCP servers.", Action::failure).to_owned()),
            (Effect::StartMcpOAuth { .. }, Err(e)) => OnReject::Toast(oauth_failure_message(e.code.as_deref()).to_owned()),
            (Effect::DisconnectMcpOAuth { .. }, _) => OnReject::Toast("Could not disconnect OAuth. Try again.".into()),
            (Effect::CompleteMcpOAuth { .. }, Err(e)) => {
                self.paste_failed(e);
                OnReject::Quiet
            }
            (Effect::HasMcpOAuthToken { check: TokenCheck::Paste { server_name, .. }, .. }, _) => {
                OnReject::Toast(format!("The sign-in to {server_name} expired. Press Connect to start again."))
            }
            _ => OnReject::Quiet,
        };
        let connected_before = match &effect {
            Effect::StartMcpOAuth { server_id, .. } => Some((server_id.clone(), self.mcp_flag(&self.chat.acp.mcp_oauth_connecting, server_id))),
            _ => None,
        };
        let ok = outcome.is_ok();
        let event = match outcome {
            Ok(value) => Event::McpCommandDone(effect, value),
            Err(error) => Event::McpCommandFailed(effect, error),
        };
        self.mcp_step(event, on_reject);
        if let Some((id, true)) = connected_before {
            let acp = &self.chat.acp;
            if ok && !self.mcp_flag(&acp.mcp_oauth_connecting, &id) && self.mcp_flag(&acp.mcp_oauth_connected, &id) {
                let name = acp.mcp_servers.iter().find(|s| s["id"] == json!(id)).and_then(|s| s["name"].as_str()).unwrap_or_default().to_owned();
                self.toasts.push(self.now, ui::overlay::ToastKind::Success, format!("Connected to {name}"), None);
            }
        }
        match action {
            Some(action) if ok => self.mcp_action_landed(action),
            Some(Action::Add(_) | Action::Edit) => self.mcp_dialog_settled(),
            Some(Action::Install) => self.install_saved(false),
            _ => {}
        }
        self.mcp_after();
    }

    /// After the model read an MCP answer: the paste-back dialog and the page follow it.
    fn mcp_after(&mut self) {
        self.paste_sync();
        self.mcp_settle();
        self.install_follow();
    }

    pub(crate) fn mcp_flag(&self, map: &std::collections::BTreeMap<String, Value>, id: &str) -> bool {
        map.get(id).is_some_and(|v| v.as_bool() == Some(true))
    }

    /// Asks each due sign-in poll for its token; returns when the next one is due.
    pub(crate) fn mcp_tick(&mut self, now: f64) -> Option<f64> {
        let mut due = Vec::new();
        for poll in &mut self.mcp_ui.polls {
            if !poll.asking && poll.next <= now {
                poll.asking = true;
                due.push((poll.url.clone(), json!({ "projectId": poll.project_id, "serverUrl": poll.url })));
            }
        }
        for (url, body) in due {
            self.mcp_post("/mcp-servers/oauth/status", body, Fetch::McpPoll(url));
        }
        self.mcp_ui.polls.iter().filter(|p| !p.asking).map(|p| p.next).reduce(f64::min)
    }

    /// A poll's status answer: a fresh token finishes the flow; the deadline fails it.
    pub(crate) fn mcp_polled(&mut self, url: &str, result: Result<Value, HttpError>) {
        let Some(at) = self.mcp_ui.polls.iter().position(|p| p.url == url) else { return };
        let poll = &mut self.mcp_ui.polls[at];
        poll.asking = false;
        poll.next = self.now + POLL_EVERY;
        let fresh = result.as_ref().ok().filter(|r| r["hasToken"] == json!(true)).is_some_and(|r| {
            match (poll.generation, r["tokenGeneration"].as_u64()) {
                (Some(start), Some(now)) => now > start,
                _ => true,
            }
        });
        if fresh {
            let poll = self.mcp_ui.polls.remove(at);
            return self.mcp_answer(poll.start, Ok(Value::Null), None);
        }
        if self.now >= poll.deadline {
            let poll = self.mcp_ui.polls.remove(at);
            let error = McpError { message: "OAuth flow timed out — user did not complete authorization in time".into(), code: Some("OAUTH_TIMEOUT".into()) };
            self.mcp_answer(poll.start, Err(error), None);
        }
    }
}
