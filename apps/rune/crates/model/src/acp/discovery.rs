//! `discoverSessions` (`slices/session.ts`): the agent's own sessions in a
//! folder, read page by page through `session/list` with the sign-in retry.
//!
//! Each page is a `listSessions` effect the app answers with
//! `Event::AgentCommandDone` and the page, or `AgentCommandFailed`.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{Effect, State};

/// `session/list` pages read at most.
const MAX_PAGES: usize = 10;

/// A discovery in flight, by its key.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Discovery {
    agent_id: String,
    cwd: String,
    sessions: Vec<Value>,
    seen: BTreeSet<String>,
    pages: usize,
}

fn is_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// `normalizeCwd`: trimmed, forward slashes, no trailing slash but a root's,
/// and lowercased for a Windows path.
pub(crate) fn normalize_cwd(cwd: &str) -> String {
    let trimmed = crate::js_trim(cwd);
    if trimmed.is_empty() {
        return String::new();
    }
    let windows = trimmed.get(..2).is_some_and(is_drive) || trimmed.contains('\\');
    let slashed = trimmed.replace('\\', "/");
    let mut slashed = slashed.trim_end_matches('/').to_owned();
    if slashed.is_empty() {
        slashed = "/".into();
    } else if is_drive(&slashed) {
        slashed.push('/');
    }
    if windows {
        slashed.to_lowercase()
    } else {
        slashed
    }
}

/// `discoveryKey`.
fn discovery_key(agent_id: &str, cwd: &str) -> String {
    format!("{agent_id}\0{}", normalize_cwd(cwd))
}

impl State {
    /// `discoverSessions`: only an agent that advertises `session/list` and
    /// is connected, one discovery per agent and folder at a time.
    pub(crate) fn discover_sessions(
        &mut self,
        agent_id: String,
        cwd: String,
        fx: &mut Vec<Effect>,
    ) {
        let capabilities = self
            .agents
            .get(&agent_id)
            .and_then(|agent| agent.get("capabilities"))
            .filter(|caps| super::js::truthy(caps));
        let lists = capabilities
            .and_then(|caps| caps.pointer("/sessionCapabilities/list"))
            .is_some_and(super::js::truthy);
        if !lists {
            return;
        }
        let key = discovery_key(&agent_id, &cwd);
        if self
            .discovering_keys
            .get(&key)
            .is_some_and(super::js::truthy)
            || self.agent_status.get(&agent_id) != Some(&json!("connected"))
        {
            return;
        }
        self.discovering_keys.insert(key.clone(), json!(true));
        let discovery = Discovery {
            agent_id,
            cwd,
            ..Discovery::default()
        };
        fx.push(list_page(&key, &discovery, None));
        self.book.discoveries.insert(key, discovery);
    }

    /// A page came back: keep its new sessions, then read the next page or
    /// finish.
    pub(crate) fn on_session_list_page(&mut self, key: &str, reply: &Value, fx: &mut Vec<Effect>) {
        // Reading a page that is not there throws, which fails the discovery.
        if reply.is_null() {
            return self.on_session_list_failed(key);
        }
        let Some(discovery) = self.book.discoveries.get_mut(key) else {
            return;
        };
        for info in reply
            .get("sessions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = super::js::string_of(info.get("sessionId"));
            if discovery.seen.insert(id) {
                discovery.sessions.push(info.clone());
            }
        }
        discovery.pages += 1;
        let next = reply.get("nextCursor").filter(|cursor| !cursor.is_null());
        if let Some(cursor) = next
            && discovery.pages < MAX_PAGES
        {
            let cursor = super::js::string_of(Some(cursor));
            return fx.push(list_page(key, discovery, Some(cursor)));
        }
        let discovery = self.book.discoveries.remove(key).expect("found above");
        // A result that outlived its agent's connection is dropped.
        if self.agent_status.get(&discovery.agent_id) == Some(&json!("connected")) {
            self.discovered_sessions
                .insert(key.to_owned(), Value::Array(discovery.sessions));
        }
        self.discovering_keys.remove(key);
    }

    /// The list failed for good: the folder's old result goes too.
    pub(crate) fn on_session_list_failed(&mut self, key: &str) {
        self.book.discoveries.remove(key);
        self.discovered_sessions.remove(key);
        self.discovering_keys.remove(key);
    }
}

fn list_page(key: &str, discovery: &Discovery, cursor: Option<String>) -> Effect {
    Effect::ListSessions {
        agent_id: discovery.agent_id.clone(),
        cwd: Some(discovery.cwd.clone()).filter(|cwd| !cwd.is_empty()),
        cursor,
        key: key.to_owned(),
        retried: false,
    }
}
