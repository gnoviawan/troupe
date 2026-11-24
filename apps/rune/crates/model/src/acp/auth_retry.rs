//! `withAuthRetry` and `authenticateBeforeSession` (`slices/agent.ts`) for
//! the session calls the model sends: `session/set_mode`,
//! `session/set_model`, `session/list` and the import's `session/load`.
//!
//! A call that fails because the agent wants a sign-in runs the
//! authenticate the agent's methods allow, then is sent once more. The
//! authenticate shares the in-flight one of the same agent and method, a
//! click's included, as the TypeScript `inFlightAuth` map does.

use std::collections::BTreeMap;

use serde_json::Value;

use super::discovery::SessionList;
use super::host_error::{self, HostError};
use super::{Effect, FlightId, State};

/// Names one session call waiting on an authenticate on demand.
pub type AuthRetryId = u64;

const SOURCE: &str = "acp-store.withAuthRetry";
const BEFORE_SESSION: &str = "acp-store.authenticateBeforeSession";

/// The registry agent whose key the host keeps (`inlineKeyFormMethodId`),
/// and whose persisted login the session call already tried.
const FACTORY_DROID: &str = "factory-droid";
const FACTORY_KEY_METHOD: &str = "factory-api-key";

/// `authPickerUnavailableError`: the text surface has no method picker.
pub(crate) const PICKER_UNAVAILABLE: &str = "This agent requires sign-in. Start a new chat for this agent to choose a sign-in method, then retry.";

/// What an authenticate sent on demand does on its answer.
#[derive(Clone, Debug, PartialEq)]
pub struct OnDemand {
    /// The method was the remembered one, so a sign-in refusal forgets it.
    remembered: bool,
    /// The host's stored Factory key was used.
    inline_key: bool,
    /// The names an `AmbiguousAuthError` lists.
    names: String,
}

/// Why an authenticate before a session call did not sign in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AuthFailure {
    /// `AmbiguousAuthError`: several methods and none remembered.
    Ambiguous(String),
    Other(String),
}

impl AuthFailure {
    fn ambiguous(names: &str) -> Self {
        Self::Ambiguous(format!(
            "This agent advertises multiple sign-in methods ({names}). Troupe does not choose one automatically; pick one of the methods below to sign in."
        ))
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Ambiguous(message) | Self::Other(message) => message,
        }
    }
}

/// A set command waiting on its authenticate on demand.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AuthRetry {
    command: Effect,
    first_error: HostError,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AuthRetryBook {
    seq: AuthRetryId,
    retries: BTreeMap<AuthRetryId, AuthRetry>,
    /// Per flight, the retries that wait on it.
    waiting: BTreeMap<FlightId, Vec<AuthRetryId>>,
}

/// `op` label of a set command that retries after an authenticate.
fn auth_op(command: &Effect) -> Option<&'static str> {
    match command {
        Effect::SetMode { .. } => Some("session/set_mode"),
        Effect::SetModel { .. } => Some("session/set_model"),
        Effect::ListSessions { .. } => Some("session/list"),
        Effect::ImportAgentSession { .. } => Some("session/load"),
        _ => None,
    }
}

fn agent_of(command: &Effect) -> &str {
    match command {
        Effect::SetMode { agent_id, .. }
        | Effect::SetModel { agent_id, .. }
        | Effect::ListSessions { agent_id, .. }
        | Effect::ImportAgentSession { agent_id, .. } => agent_id,
        _ => "",
    }
}

fn mark_retried(mut command: Effect) -> Effect {
    if let Effect::SetMode { retried, .. }
    | Effect::SetModel { retried, .. }
    | Effect::ListSessions { retried, .. }
    | Effect::ImportAgentSession { retried, .. } = &mut command
    {
        *retried = true;
    }
    command
}

const ENOENT: [&str; 5] = [
    "enoent",
    "no such file",
    "program not found",
    "command not found",
    "failed to spawn",
];

const TRANSPORT: [&str; 18] = [
    "destroyed",
    "broken pipe",
    "econnreset",
    "econnrefused",
    "disconnected",
    "connection closed",
    "connection lost",
    "connection reset",
    "connection refused",
    "connection aborted",
    "stream closed",
    "stream ended",
    "stream destroyed",
    "stream error",
    "transport",
    "agent thread is no longer running",
    "agent thread dropped",
    "pipe",
];

const AUTH: [&str; 7] = [
    "auth",
    "login",
    "credential",
    "unauthoris",
    "unauthoriz",
    "logged out",
    "forbidden",
];

fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `needle` starts somewhere in `text` and the test accepts what follows.
fn find_with(text: &str, needle: &str, rest: impl Fn(&str) -> bool) -> bool {
    text.match_indices(needle)
        .any(|(at, _)| rest(&text[at + needle.len()..]))
}

/// `a.*b` on one line, as `.` stops at a line break.
fn on_a_line(text: &str, a: &str, b: &str) -> bool {
    text.split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .any(|line| find_with(line, a, |rest| rest.contains(b)))
}

/// `word[\s-]?word`.
fn joined(text: &str, head: &str, tail: &str) -> bool {
    find_with(text, head, |rest| {
        let rest = rest
            .strip_prefix(|c: char| c == '-' || c.is_whitespace())
            .unwrap_or(rest);
        rest.starts_with(tail)
    }) || text.contains(&format!("{head}{tail}"))
}

/// `\bcode\b`.
fn whole_number(text: &str, code: &str) -> bool {
    text.match_indices(code).any(|(at, _)| {
        let before = text[..at].chars().next_back().is_none_or(|c| !word(c));
        let after = text[at + code.len()..]
            .chars()
            .next()
            .is_none_or(|c| !word(c));
        before && after
    })
}

/// `isAuthRetriableSessionError`: the explicit auth-required signal, or a
/// failure `classifySetupError` files under `auth`, in its order.
pub(crate) fn is_auth_retriable(error: &HostError) -> bool {
    let code = error.code.as_deref();
    if code == Some(host_error::AMBIGUOUS_AUTH) {
        return false;
    }
    if code == Some(host_error::AUTH_REQUIRED) || error.message.starts_with("ACP_AUTH_REQUIRED") {
        return true;
    }
    if error.is(&host_error::SIGNED_OUT) {
        return true;
    }
    if error.is(&host_error::DEFINITE_SPAWN_REJECTION)
        || code.is_some_and(|code| host_error::PROVIDER.contains(&code))
    {
        return false;
    }
    let text = error.message.to_ascii_lowercase();
    if ENOENT.iter().any(|needle| text.contains(needle)) || on_a_line(&text, "spawn", "fail") {
        return false;
    }
    let pipe = find_with(&text, "pipe", |rest| {
        rest.chars().next().is_none_or(|c| !word(c))
    });
    let transport = TRANSPORT[..17].iter().any(|needle| text.contains(needle))
        || pipe
        || on_a_line(&text, "connection", "timed out");
    if transport {
        return false;
    }
    AUTH.iter().any(|needle| text.contains(needle))
        || joined(&text, "sign", "in")
        || joined(&text, "api", "key")
        || text.contains("not logged in")
        || whole_number(&text, "401")
        || whole_number(&text, "403")
}

/// `withAuthRetry` logs a retry that went through.
pub(crate) fn note_retry_succeeded(command: &Effect, fx: &mut Vec<Effect>) {
    if let Some(op) = auth_op(command).filter(|_| command.retried()) {
        fx.push(Effect::info(
            SOURCE,
            format!("{op} succeeded after authenticate-on-demand"),
        ));
    }
}

impl State {
    /// `forgetAuthMethodForConfig`.
    pub(crate) fn forget_auth_method(&mut self, config_id: &str, fx: &mut Vec<Effect>) {
        if self
            .book
            .remembered_auth_methods
            .remove(config_id)
            .is_some()
        {
            fx.push(Effect::PersistAuthMethods {
                methods: self.book.remembered_auth_methods.clone(),
            });
        }
    }

    /// A session call failed. One that wanted a sign-in authenticates and
    /// goes again; a retry that fails is final.
    pub(crate) fn on_session_call_failed(
        &mut self,
        command: Effect,
        error: HostError,
        fx: &mut Vec<Effect>,
    ) {
        if let Some(op) = auth_op(&command) {
            if command.retried() {
                fx.push(Effect::warn(
                    SOURCE,
                    format!("{op} failed again after the authenticate-on-demand retry"),
                ));
                // A method that signs in but cannot authorize is a dead pick.
                if is_auth_retriable(&error)
                    && let Some(config_id) = self.config_id_for_agent(agent_of(&command))
                {
                    self.forget_auth_method(&config_id, fx);
                }
            } else if is_auth_retriable(&error) {
                return self.reauthenticate(command, error, fx);
            }
        }
        self.control_rejected(command, error, fx);
    }

    /// The session call's failure reaches its caller.
    fn control_rejected(&mut self, command: Effect, error: HostError, fx: &mut Vec<Effect>) {
        match command {
            Effect::ListSessions {
                list: SessionList::Discovery(key),
                ..
            } => self.on_discovery_failed(&key),
            Effect::ListSessions {
                list: SessionList::Import(import),
                ..
            } => self.on_import_list_failed(import, &error, fx),
            Effect::ImportAgentSession {
                import, session_id, ..
            } => self.on_import_session_failed(import, session_id, error, fx),
            command => match command.run() {
                Some(run) => self.on_run_command(run, Some(error.message), fx),
                None => fx.push(Effect::rejected_by(&error)),
            },
        }
    }

    /// `reauthenticateOnDemand`.
    fn reauthenticate(&mut self, command: Effect, first_error: HostError, fx: &mut Vec<Effect>) {
        let op = auth_op(&command).unwrap_or_default();
        fx.push(Effect::info(
            SOURCE,
            format!("{op} reported auth required; authenticating on demand"),
        ));
        let agent_id = agent_of(&command).to_owned();
        // The failed call proved the process is not signed in.
        self.book.authenticated_agents.remove(&agent_id);
        let book = &mut self.book.auth_retry;
        book.seq += 1;
        let retry = book.seq;
        book.retries.insert(
            retry,
            AuthRetry {
                command,
                first_error,
            },
        );
        self.authenticate_before_session(retry, &agent_id, None, fx);
    }

    /// `authenticateBeforeSession` for the retry. `factory_key` is the
    /// host's answer once the stored-key check ran.
    fn authenticate_before_session(
        &mut self,
        retry: AuthRetryId,
        agent_id: &str,
        factory_key: Option<bool>,
        fx: &mut Vec<Effect>,
    ) {
        let outcome = self.select_auth_method(retry, agent_id, factory_key, fx);
        if let Some(outcome) = outcome {
            self.resume_auth_retry(retry, outcome, fx);
        }
    }

    /// Picks the method and sends (or joins) its authenticate; `Some` when
    /// the outcome is known without one.
    fn select_auth_method(
        &mut self,
        retry: AuthRetryId,
        agent_id: &str,
        factory_key: Option<bool>,
        fx: &mut Vec<Effect>,
    ) -> Option<Result<bool, AuthFailure>> {
        if self.book.authenticated_agents.contains(agent_id) {
            return Some(Ok(false));
        }
        let agent = self.agents.get(agent_id);
        if agent.and_then(|agent| agent.get("hostAuthReady")) == Some(&Value::Bool(true)) {
            return Some(Ok(false));
        }
        let valid: Vec<Value> = agent
            .and_then(|agent| agent.get("authMethods"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|method| {
                method
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !crate::js_trim(id).is_empty())
            })
            .cloned()
            .collect();
        if valid.is_empty() {
            return Some(Ok(false));
        }
        let id_of = |method: &Value| {
            crate::js_trim(method.get("id").and_then(Value::as_str).unwrap_or_default()).to_owned()
        };
        let names = valid
            .iter()
            .map(|method| match method.get("name") {
                None | Some(Value::Null) => String::new(),
                name => super::js::string_of(name),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let config_id = self.config_id_for_agent(agent_id);
        let factory = self.config_to_live_agent.iter().any(|(key, live)| {
            live.as_str() == Some(agent_id)
                && key
                    .split('\0')
                    .next()
                    .map(|id| id.strip_prefix("acp-registry:").unwrap_or(id))
                    == Some(FACTORY_DROID)
        });
        let mut method = None;
        let mut inline_key = false;
        let key_method = valid
            .iter()
            .find(|method| method.get("id").and_then(Value::as_str) == Some(FACTORY_KEY_METHOD));
        if factory && let Some(key_method) = key_method {
            match factory_key {
                None => {
                    fx.push(Effect::ReadFactoryKeyStatus { retry });
                    return None;
                }
                Some(true) => {
                    method = Some(key_method.clone());
                    inline_key = true;
                }
                Some(false) => {}
            }
        }
        let mut remembered = false;
        if method.is_none() && factory && valid.len() > 1 {
            fx.push(Effect::info(
                BEFORE_SESSION,
                "Persisted-credentials policy: leaving sign-in to the banner".into(),
            ));
            return Some(Ok(false));
        }
        if method.is_none() {
            let saved = config_id
                .as_ref()
                .and_then(|config| self.book.remembered_auth_methods.get(config))
                .cloned();
            if valid.len() == 1 {
                let only = valid[0].clone();
                if let (Some(saved), Some(config)) = (&saved, &config_id)
                    && *saved != id_of(&only)
                {
                    self.forget_auth_method(&config.clone(), fx);
                }
                method = Some(only);
            } else {
                let hit = saved
                    .as_ref()
                    .and_then(|saved| valid.iter().find(|method| id_of(method) == *saved));
                let Some(hit) = hit.cloned() else {
                    if let (Some(_), Some(config)) = (&saved, &config_id) {
                        self.forget_auth_method(&config.clone(), fx);
                    }
                    return Some(Err(AuthFailure::ambiguous(&names)));
                };
                if !runs_by_itself(&hit) {
                    if let Some(config) = &config_id {
                        self.forget_auth_method(&config.clone(), fx);
                    }
                    return Some(Err(AuthFailure::ambiguous(&names)));
                }
                fx.push(Effect::info(
                    BEFORE_SESSION,
                    format!(
                        "Auto-authenticating with the remembered sign-in method '{}'",
                        id_of(&hit)
                    ),
                ));
                method = Some(hit);
                remembered = true;
            }
        }
        let method = method.expect("chosen above");
        if !runs_by_itself(&method) {
            return Some(Ok(false));
        }
        let method_id = id_of(&method);
        if method_id == "gateway" {
            if let Some(config) = &config_id {
                self.forget_auth_method(&config.clone(), fx);
            }
            return Some(Ok(false));
        }
        let key = format!("{agent_id}\0{method_id}");
        let flight = match self.book.auth_flights.get(&key) {
            Some(flight) => *flight,
            None => {
                let flight = self.book.next_flight();
                self.book.auth_flights.insert(key, flight);
                self.book
                    .last_auth_attempt
                    .insert(agent_id.to_owned(), method_id.clone());
                fx.push(Effect::Authenticate {
                    agent_id: agent_id.to_owned(),
                    method_id,
                    input: None,
                    flight,
                    on_demand: Some(OnDemand {
                        remembered,
                        inline_key,
                        names,
                    }),
                });
                flight
            }
        };
        self.book
            .auth_retry
            .waiting
            .entry(flight)
            .or_default()
            .push(retry);
        None
    }

    /// The host answered whether it stores a Factory key.
    pub(crate) fn on_factory_key_status(
        &mut self,
        read: Effect,
        stored: Option<bool>,
        fx: &mut Vec<Effect>,
    ) {
        let Effect::ReadFactoryKeyStatus { retry } = read else {
            return;
        };
        let Some(agent_id) = self
            .book
            .auth_retry
            .retries
            .get(&retry)
            .map(|waiting| agent_of(&waiting.command).to_owned())
        else {
            return;
        };
        if stored.is_none() {
            fx.push(Effect::warn(
                BEFORE_SESSION,
                "Factory credential status could not be read".into(),
            ));
        }
        self.authenticate_before_session(retry, &agent_id, Some(stored.unwrap_or(false)), fx);
    }

    /// An on-demand authenticate failed: the error its waiters see.
    pub(crate) fn on_demand_failed(
        &mut self,
        agent_id: &str,
        on_demand: &OnDemand,
        error: HostError,
        fx: &mut Vec<Effect>,
    ) -> AuthFailure {
        fx.push(Effect::warn(
            BEFORE_SESSION,
            "Agent authenticate request failed before session creation".into(),
        ));
        if on_demand.inline_key {
            return AuthFailure::Other(
                "Factory API key authentication failed. Enter a new key or choose Login.".into(),
            );
        }
        if on_demand.remembered && is_auth_retriable(&error) {
            if let Some(config_id) = self.config_id_for_agent(agent_id) {
                self.forget_auth_method(&config_id, fx);
            }
            return AuthFailure::ambiguous(&on_demand.names);
        }
        AuthFailure::Other(error.message)
    }

    /// The flight settled: every retry waiting on it moves on.
    pub(crate) fn settle_auth_retries(
        &mut self,
        flight: FlightId,
        outcome: Result<bool, AuthFailure>,
        fx: &mut Vec<Effect>,
    ) {
        for retry in self
            .book
            .auth_retry
            .waiting
            .remove(&flight)
            .unwrap_or_default()
        {
            self.resume_auth_retry(retry, outcome.clone(), fx);
        }
    }

    /// The authenticate before the retry is over: send the call again, or
    /// fail it (the first error when nothing signed in).
    fn resume_auth_retry(
        &mut self,
        retry: AuthRetryId,
        outcome: Result<bool, AuthFailure>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(waiting) = self.book.auth_retry.retries.remove(&retry) else {
            return;
        };
        match outcome {
            Ok(true) => fx.push(mark_retried(waiting.command)),
            Ok(false) => self.control_rejected(waiting.command, waiting.first_error, fx),
            Err(AuthFailure::Ambiguous(_)) => {
                let error = HostError::new(PICKER_UNAVAILABLE.into(), None);
                self.control_rejected(waiting.command, error, fx);
            }
            Err(AuthFailure::Other(message)) => {
                self.control_rejected(waiting.command, HostError::new(message, None), fx);
            }
        }
    }
}

/// An agent or untyped method, which can run without the user.
fn runs_by_itself(method: &Value) -> bool {
    match method.get("type") {
        None | Some(Value::Null) => true,
        Some(kind) => kind.as_str() == Some("agent"),
    }
}
