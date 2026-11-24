//! The user's agent commands: `spawnAgent`, `killAgent`,
//! `authenticateAgent`, `logoutAgent`, `respondAgentSignIn` and
//! `completeBrowserAuth` in `slices/agent.ts`, with the auth bookkeeping
//! beside them (`authenticatedAgents`, the in-flight authenticate per agent
//! and method, the remembered method per config, and the auto-open grant of
//! a sign-in flight).
//!
//! A command is two steps, as sending is. The action returns the command
//! effect; the app sends it and hands the same effect back as
//! `Event::AgentCommandDone` with the reply, or `Event::AgentCommandFailed`
//! with the error's message.

use serde_json::{Map, Value, json};

use super::auth_retry::AuthFailure;
use super::ensure::EnsureId;
use super::types::SessionStatus;
use super::{Effect, State};

/// Names one authenticate round-trip; a browser open during it may open by
/// itself once.
pub type FlightId = u64;

/// `a ?? b`: `a` unless it is absent or null.
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

fn flight_key(agent_id: &str, method_id: &str) -> String {
    format!("{agent_id}\0{method_id}")
}

impl State {
    /// The `type` of the advertised method `method_id`; `None` for a method
    /// the agent does not list or one without a type.
    fn auth_method_type(&self, agent_id: &str, method_id: &str) -> Option<String> {
        self.agents
            .get(agent_id)
            .and_then(|agent| agent.get("authMethods"))
            .and_then(Value::as_array)?
            .iter()
            .find(|method| {
                method
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| crate::js_trim(id) == method_id)
            })
            .and_then(|method| present(method.get("type")))
            .map(|kind| super::js::string_of(Some(kind)))
    }

    /// A method that can run again by itself (an agent or untyped one).
    fn auto_runs(&self, agent_id: &str, method_id: &str) -> bool {
        self.auth_method_type(agent_id, method_id)
            .is_none_or(|kind| kind == "agent")
    }

    /// `rememberAuthMethodForConfig`: the next process for the config may
    /// sign in with this method without asking.
    fn remember_auth_method(&mut self, config_id: String, method_id: &str, fx: &mut Vec<Effect>) {
        let remembered = &mut self.book.remembered_auth_methods;
        if remembered.get(&config_id).map(String::as_str) == Some(method_id) {
            return;
        }
        remembered.insert(config_id, method_id.to_owned());
        fx.push(Effect::PersistAuthMethods {
            methods: remembered.clone(),
        });
    }

    /// The process is gone: a re-spawn signs in again.
    pub(crate) fn forget_agent_auth(&mut self, agent_id: &str) {
        let book = &mut self.book;
        book.authenticated_agents.remove(agent_id);
        let prefix = format!("{agent_id}\0");
        book.auth_flights.retain(|key, _| !key.starts_with(&prefix));
        book.last_auth_attempt.remove(agent_id);
        book.auto_open_granted.remove(agent_id);
    }

    /// `currentAuthFlight`: an authenticate of the agent still in flight.
    pub(crate) fn current_auth_flight(&self, agent_id: &str) -> Option<FlightId> {
        let prefix = format!("{agent_id}\0");
        self.book
            .auth_flights
            .iter()
            .find(|(key, _)| key.starts_with(&prefix))
            .map(|(_, flight)| *flight)
    }

    pub(crate) fn spawn_agent(
        &mut self,
        config: Value,
        project_id: Option<Value>,
        session_id: Option<String>,
        ensure: Option<EnsureId>,
        fx: &mut Vec<Effect>,
    ) {
        let name = super::js::string_of(config.get("name"));
        self.agent_status.insert(name, json!("spawning"));
        fx.push(Effect::SpawnAgent {
            config,
            project_id,
            session_id: session_id.filter(|id| !id.is_empty()),
            ensure,
        });
    }

    pub(crate) fn kill_agent(
        &mut self,
        agent_id: String,
        ensure: Option<EnsureId>,
        fx: &mut Vec<Effect>,
    ) {
        fx.push(Effect::KillAgent { agent_id, ensure });
    }

    pub(crate) fn authenticate_agent(
        &mut self,
        agent_id: String,
        method_id: &str,
        input: Option<Value>,
        fx: &mut Vec<Effect>,
    ) {
        let method_id = crate::js_trim(method_id);
        if method_id.is_empty() {
            fx.push(Effect::rejected(
                "Cannot sign in: the agent advertised an empty authentication method id.",
            ));
            return;
        }
        let advertised: Vec<&str> = self
            .agents
            .get(&agent_id)
            .and_then(|agent| agent.get("authMethods"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|method| method.get("id").and_then(Value::as_str))
            .map(crate::js_trim)
            .filter(|id| !id.is_empty())
            .collect();
        if !advertised.is_empty() && !advertised.contains(&method_id) {
            fx.push(Effect::rejected(
                "Cannot sign in: this authentication method is no longer advertised.",
            ));
            return;
        }
        // A click while the same method's authenticate runs shares it.
        let key = flight_key(&agent_id, method_id);
        if let Some(flight) = self.book.auth_flights.get(&key) {
            *self.book.auth_waiters.entry(*flight).or_default() += 1;
            return;
        }
        let flight = self.book.next_flight();
        self.book.auth_flights.insert(key, flight);
        self.book
            .last_auth_attempt
            .insert(agent_id.clone(), method_id.to_owned());
        fx.push(Effect::Authenticate {
            agent_id,
            method_id: method_id.to_owned(),
            input: input.filter(super::js::truthy),
            flight,
            on_demand: None,
        });
    }

    pub(crate) fn logout_agent(&mut self, agent_id: String, fx: &mut Vec<Effect>) {
        let supports = self
            .agents
            .get(&agent_id)
            .and_then(|agent| agent.pointer("/capabilities/auth/logout"))
            .is_some_and(super::js::truthy);
        if !supports {
            fx.push(Effect::rejected("This agent does not support sign out."));
            return;
        }
        fx.push(Effect::Logout { agent_id });
    }

    /// Answer an agent's own sign-in prompt. An accepted prompt stays up:
    /// the user still needs its code until the host reports the sign-in
    /// finished.
    pub(crate) fn respond_agent_sign_in(
        &mut self,
        agent_id: String,
        action: String,
        fx: &mut Vec<Effect>,
    ) {
        let Some(prompt) = self
            .pending_sign_in
            .get(&agent_id)
            .filter(|prompt| prompt.get("kind").and_then(Value::as_str) == Some("elicitation"))
        else {
            return;
        };
        let request_id = super::js::string_of(prompt.get("requestId"));
        if action == "cancel" {
            self.clear_pending_sign_in(&agent_id);
        }
        fx.push(Effect::info(
            "acp-store:respondAgentSignIn",
            format!("[acp] sign-in prompt {action} (agentId={agent_id}, requestId={request_id})"),
        ));
        fx.push(Effect::RespondElicitation {
            agent_id,
            request_id,
            action,
        });
    }

    /// The OAuth redirect reached the agent: the sign-in finished without an
    /// authenticate reply, so the attempted method is remembered here.
    pub(crate) fn complete_browser_auth(&mut self, agent_id: &str, fx: &mut Vec<Effect>) {
        self.book.authenticated_agents.insert(agent_id.to_owned());
        if let Some(attempted) = self.book.last_auth_attempt.remove(agent_id)
            && let Some(config_id) = self.config_id_for_agent(agent_id)
            && self.auto_runs(agent_id, &attempted)
        {
            self.remember_auth_method(config_id, &attempted, fx);
        }
        self.clear_pending_sign_in(agent_id);
    }

    /// The command an agent command effect sent answered `reply`.
    pub(crate) fn on_agent_command_done(
        &mut self,
        command: Effect,
        reply: Value,
        fx: &mut Vec<Effect>,
    ) {
        match command {
            Effect::ListSessions { ref key, .. } => {
                super::auth_retry::note_retry_succeeded(&command, fx);
                self.on_session_list_page(key, &reply, fx);
            }
            Effect::SpawnAgent { config, ensure, .. } => {
                let agent_id = self.on_agent_spawn_reply(&config, &reply);
                if let Some(ensure) = ensure {
                    self.on_ensure_spawned(ensure, agent_id, fx);
                }
            }
            Effect::KillAgent { agent_id, ensure } => {
                self.on_agent_killed(&agent_id);
                if let Some(ensure) = ensure {
                    self.finish_spawn(ensure, Ok(None), fx);
                }
            }
            Effect::Authenticate {
                agent_id,
                method_id,
                input,
                flight,
                ..
            } => {
                // An on-demand method always runs by itself without input,
                // so the click's rule remembers it too.
                self.book.authenticated_agents.insert(agent_id.clone());
                if let Some(config_id) = self.config_id_for_agent(&agent_id)
                    && input.is_none()
                    && method_id != "gateway"
                    && self.auto_runs(&agent_id, &method_id)
                {
                    self.remember_auth_method(config_id, &method_id, fx);
                }
                self.clear_pending_sign_in(&agent_id);
                self.settle_flight(&agent_id, &method_id, flight);
                self.settle_auth_retries(flight, Ok(true), fx);
            }
            Effect::Logout { agent_id } => {
                self.book.authenticated_agents.remove(&agent_id);
            }
            _ => {}
        }
    }

    /// The command an agent command effect sent failed with `message`.
    pub(crate) fn on_agent_command_failed(
        &mut self,
        command: Effect,
        message: String,
        fx: &mut Vec<Effect>,
    ) {
        let mut waiters = 0;
        let mut message = message;
        match command {
            Effect::ListSessions { .. } => {
                return self.on_session_call_failed(command, message, fx);
            }
            Effect::SpawnAgent { config, ensure, .. } => {
                let name = super::js::string_of(config.get("name"));
                self.agent_status.insert(name, json!("error"));
                if let Some(ensure) = ensure {
                    return self.finish_spawn(ensure, Err(message), fx);
                }
            }
            // The ensure's cleanup kill is best effort.
            Effect::KillAgent {
                ensure: Some(ensure),
                ..
            } => return self.finish_spawn(ensure, Ok(None), fx),
            Effect::Authenticate {
                agent_id,
                method_id,
                flight,
                on_demand: Some(on_demand),
                ..
            } => {
                let failure = self.on_demand_failed(&agent_id, &on_demand, message, fx);
                message = failure.message().to_owned();
                // Only the clicks that shared it reject; its owner is a retry.
                let clicks = self.settle_flight(&agent_id, &method_id, flight);
                for _ in 0..clicks {
                    fx.push(Effect::rejected(&message));
                }
                return self.settle_auth_retries(flight, Err(failure), fx);
            }
            Effect::Authenticate {
                agent_id,
                method_id,
                flight,
                ..
            } => {
                // No method id and no reason: an agent's auth failure may
                // echo credentials.
                fx.push(Effect::warn(
                    "acp-store.authenticateAgent",
                    "Agent authenticate request failed".into(),
                ));
                waiters = self.settle_flight(&agent_id, &method_id, flight);
                for _ in 0..=waiters {
                    fx.push(Effect::rejected(&message));
                }
                return self.settle_auth_retries(flight, Err(AuthFailure::Other(message)), fx);
            }
            Effect::Logout { agent_id } => fx.push(Effect::warn(
                "acp.logoutAgent",
                format!("Logout failed for agent {agent_id}: {message}"),
            )),
            Effect::RespondElicitation {
                agent_id,
                request_id,
                ..
            } => fx.push(Effect::warn(
                "acp-store:respondAgentSignIn",
                format!(
                    "[acp] sign-in prompt response failed (agentId={agent_id}, requestId={request_id})"
                ),
            )),
            _ => {}
        }
        for _ in 0..=waiters {
            fx.push(Effect::rejected(&message));
        }
    }

    /// The flight's authenticate settled: forget it unless a teardown
    /// already did, and return how many shared clicks waited on it.
    fn settle_flight(&mut self, agent_id: &str, method_id: &str, flight: FlightId) -> usize {
        let key = flight_key(agent_id, method_id);
        if self.book.auth_flights.get(&key) == Some(&flight) {
            self.book.auth_flights.remove(&key);
        }
        self.book.auth_waiters.remove(&flight).unwrap_or(0)
    }

    /// The spawn reply is the one source sure to reach the caller; an
    /// `agent_spawned` that came first only fills what the reply left out.
    fn on_agent_spawn_reply(&mut self, config: &Value, reply: &Value) -> String {
        let name = super::js::string_of(config.get("name"));
        let agent_id = super::js::string_of(reply.get("agentId"));
        self.agent_status.remove(&name);
        self.agent_status
            .insert(agent_id.clone(), json!("connected"));
        let existing = self.agents.get(&agent_id);
        let field = |key: &str| {
            present(reply.get(key))
                .or_else(|| existing.and_then(|agent| present(agent.get(key))))
                .cloned()
        };
        let mut agent = Map::new();
        agent.insert("id".into(), json!(agent_id));
        if let Some(capabilities) = field("capabilities") {
            agent.insert("capabilities".into(), capabilities);
        }
        agent.insert(
            "authMethods".into(),
            field("authMethods").unwrap_or_else(|| json!([])),
        );
        agent.insert(
            "hostAuthReady".into(),
            field("hostAuthReady").unwrap_or(json!(false)),
        );
        self.agents.insert(agent_id.clone(), Value::Object(agent));
        agent_id
    }

    /// `killAgent` after the host confirmed: the agent, its reuse keys and
    /// warm slots go, its chats close, and its prompts and sign-in drop.
    fn on_agent_killed(&mut self, agent_id: &str) {
        self.forget_agent_auth(agent_id);
        self.agents.remove(agent_id);
        self.agent_status.remove(agent_id);
        self.config_to_live_agent
            .retain(|_, live| live.as_str() != Some(agent_id));
        let owned: Vec<String> = self
            .sessions
            .values()
            .filter(|session| session.agent_id == agent_id)
            .map(|session| session.id.clone())
            .collect();
        for id in &owned {
            if let Some(session) = self.sessions.get_mut(id) {
                session.status = SessionStatus::Closed;
                session.active_turn = false;
                session.open_turn_id = None;
                session.replaying = Some(None);
            }
        }
        self.prepared_sessions.retain(|_, id| !owned.contains(id));
        self.pending_permissions
            .retain(|_, p| p.agent_id != agent_id);
        self.pending_questions.retain(|_, p| p.agent_id != agent_id);
        self.pending_elicitations
            .retain(|_, p| p.agent_id != agent_id);
        self.clear_pending_sign_in(agent_id);
    }
}
