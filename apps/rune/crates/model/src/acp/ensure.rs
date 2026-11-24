//! `ensureLiveAgent` (`shared-state.ts`): a connected agent process for a
//! config, a working directory and a Project, spawning at most one per reuse
//! key at a time; and `prewarmAgent`, its first caller.
//!
//! A spawn is the `spawnAgent` command; the ensure waits on its answer the
//! way the TypeScript awaits it. Everyone who asked for the same reuse key
//! meanwhile waits on the same spawn, and each judges its failure by its
//! own options (a silent caller gets no agent and a log line).

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::reuse_key::{agent_reuse_key, detached_reuse_key, parse_reuse_key};
use super::spawn_env::{find_stored_agent_config, resolve_agent_spawn_env, resolve_spawn_project};
use super::types::{Object, SessionStatus};
use super::{Effect, State};

/// Names one spawn an ensure started.
pub type EnsureId = u64;

const SOURCE: &str = "acp-store.ensureLiveAgent";

/// Host error prefixes of a spawn that can never succeed as asked
/// (`DEFINITE_SPAWN_REJECTIONS`).
const DEFINITE_SPAWN_REJECTIONS: [&str; 2] = ["ACP_AGENT_UNAVAILABLE", "ACP_INVALID_AGENT_CONFIG"];

/// Host error prefixes of a provider Account that is signed out.
const SIGNED_OUT: [&str; 3] = [
    "ACP_CLAUDE_ACCOUNT_SIGNED_OUT",
    "ACP_CODEX_ACCOUNT_SIGNED_OUT",
    "ACP_ANTIGRAVITY_ACCOUNT_SIGNED_OUT",
];

fn tagged(message: &str, tags: &[&str]) -> bool {
    tags.iter()
        .any(|tag| message.starts_with(&format!("{tag}: ")))
}

/// What a caller puts on the agent it gets (`AgentHolder`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Holder {
    Chat,
    Warm,
}

impl Holder {
    fn name(holder: Option<Self>) -> &'static str {
        match holder {
            Some(Self::Chat) => "chat",
            Some(Self::Warm) => "warm",
            None => "undefined",
        }
    }
}

/// `EnsureLiveAgentOptions`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EnsureOptions {
    pub register_warm_ui: bool,
    pub silent: bool,
    pub surface_definite: bool,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub holder: Option<Holder>,
}

/// The action an ensure answers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Caller {
    Prewarm,
    ImportList(super::import::ImportId),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Waiter {
    caller: Caller,
    config_id: String,
    reuse_key: String,
    options: EnsureOptions,
}

/// A spawn in flight (`inFlightWarms` and its outcome).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Spawn {
    reuse_key: String,
    launch_id: String,
    signature: String,
    holders: Vec<Holder>,
    register_warm_ui: bool,
    /// The starter first, then whoever joined.
    waiters: Vec<Waiter>,
}

/// The module maps of `shared-state.ts` the ensure reads and writes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EnsureBook {
    seq: EnsureId,
    spawns: BTreeMap<EnsureId, Spawn>,
    /// Reuse key to the spawn now in flight for it.
    in_flight: BTreeMap<String, EnsureId>,
    /// Reuse key to the launch signature the host rejected definitively.
    definite_rejections: BTreeMap<String, String>,
    /// Agent id to the launch signature it was spawned with.
    signatures: BTreeMap<String, String>,
    /// `config\0project` to its credential epoch.
    credential_epochs: BTreeMap<String, u64>,
    /// Agents handed to a chat whose session is not on them yet.
    pending_chat_agents: BTreeSet<String>,
    /// Agents handed to a warm prepare, with a count per prepare.
    pending_warm_agents: BTreeMap<String, u64>,
}

/// `currentLaunch`: what a new agent of the config gets now.
struct Launch {
    config: Object,
    project: Option<Value>,
    project_id: String,
    reuse_key: String,
    env: Object,
    signature: String,
}

/// `heldAgainst`: a caller putting `holder` on an agent `current` holds
/// must get its own.
fn held_against(current: Option<Holder>, holder: Option<Holder>) -> bool {
    current == Some(Holder::Chat) || (current == Some(Holder::Warm) && holder == Some(Holder::Chat))
}

fn spawn_holder(holders: &[Holder]) -> Option<Holder> {
    if holders.contains(&Holder::Chat) {
        Some(Holder::Chat)
    } else {
        holders.first().map(|_| Holder::Warm)
    }
}

fn text(config: &Object, key: &str) -> String {
    super::js::string_of(config.get(key))
}

impl State {
    /// `renewAgentCredentials`: every agent of the config spawned for the
    /// Project before this is on old credentials.
    pub(crate) fn renew_agent_credentials(&mut self, config_id: &str, project_id: &str) {
        *self
            .book
            .ensure
            .credential_epochs
            .entry(format!("{config_id}\0{project_id}"))
            .or_default() += 1;
    }

    /// `spawnSignature`: the config's launch, the env and the credential
    /// epoch, compared as one string.
    fn spawn_signature(
        &self,
        config: &Object,
        env: &Object,
        config_id: &str,
        project_id: &str,
    ) -> String {
        let epoch = self
            .book
            .ensure
            .credential_epochs
            .get(&format!("{config_id}\0{project_id}"))
            .copied()
            .unwrap_or(0);
        let field = |key: &str| config.get(key).cloned().unwrap_or(Value::Null);
        json!([
            field("name"),
            field("command"),
            field("args"),
            env,
            field("allowTerminal"),
            epoch
        ])
        .to_string()
    }

    fn current_launch(
        &self,
        config_id: &str,
        cwd: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Option<Launch> {
        let config = find_stored_agent_config(&self.agent_configs, config_id)?.clone();
        let project = resolve_spawn_project(&self.book.projects, cwd, project_id);
        let project_id = project
            .as_ref()
            .and_then(|project| project.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let env = resolve_agent_spawn_env(
            project.as_ref().and_then(|project| project.get("envVars")),
            config.get("env"),
        );
        let launch_id = text(&config, "id");
        let scope = (!project_id.is_empty()).then_some(project_id.as_str());
        let reuse_key = agent_reuse_key(&launch_id, cwd, scope, session_id);
        let signature = self.spawn_signature(&config, &env, &launch_id, &project_id);
        Some(Launch {
            config,
            project,
            project_id,
            reuse_key,
            env,
            signature,
        })
    }

    /// `runsCurrentLaunch`: an agent the model did not spawn counts as
    /// current.
    pub(crate) fn runs_current_launch(
        &self,
        agent_id: &str,
        config_id: &str,
        cwd: &str,
        project_id: Option<&str>,
    ) -> bool {
        let Some(recorded) = self.book.ensure.signatures.get(agent_id) else {
            return true;
        };
        self.current_launch(config_id, crate::js_trim(cwd), project_id, None)
            .is_some_and(|launch| launch.signature == *recorded)
    }

    fn hold_agent(&mut self, agent_id: &str, holder: Holder) {
        let book = &mut self.book.ensure;
        match holder {
            Holder::Chat => {
                book.pending_chat_agents.insert(agent_id.to_owned());
            }
            Holder::Warm => {
                *book
                    .pending_warm_agents
                    .entry(agent_id.to_owned())
                    .or_default() += 1
            }
        }
    }

    /// `agentHolder`: what the agent serves now. Warm-pool sessions are not
    /// ported yet (PR 8), so every session counts as a chat.
    fn agent_holder(&mut self, agent_id: &str) -> Option<Holder> {
        let mut chats = self
            .sessions
            .values()
            .filter(|session| session.agent_id == agent_id)
            .peekable();
        let book = &mut self.book.ensure;
        if chats.peek().is_some() {
            let open = chats.any(|session| session.status != SessionStatus::Closed);
            book.pending_chat_agents.remove(agent_id);
            if open {
                return Some(Holder::Chat);
            }
        } else if book.pending_chat_agents.contains(agent_id) {
            return Some(Holder::Chat);
        }
        if book.pending_warm_agents.contains_key(agent_id) {
            return Some(Holder::Warm);
        }
        let hosts_warm = self.prepared_sessions.values().any(|session_id| {
            self.sessions
                .get(session_id)
                .is_some_and(|session| session.agent_id == agent_id)
        });
        hosts_warm.then_some(Holder::Warm)
    }

    /// `otherProjectHoldsAgent`.
    fn other_project_holds_agent(&self, config_id: &str, cwd: &str, project_id: &str) -> bool {
        self.config_to_live_agent.keys().any(|key| {
            let parsed = parse_reuse_key(key);
            parsed.detached_agent_id.is_none()
                && parsed.config_id == config_id
                && parsed.cwd == cwd
                && parsed
                    .project_id
                    .is_some_and(|id| !id.is_empty() && id != project_id)
        })
    }

    fn live_agent(&self, reuse_key: &str) -> Option<String> {
        self.config_to_live_agent
            .get(reuse_key)
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    }

    /// `ensureLiveAgent`: answer `caller` with the agent for the launch.
    pub(crate) fn ensure_live_agent(
        &mut self,
        config_id: &str,
        cwd: &str,
        options: EnsureOptions,
        caller: Caller,
        fx: &mut Vec<Effect>,
    ) {
        let cwd = crate::js_trim(cwd);
        let launch = (!cwd.is_empty())
            .then(|| {
                self.current_launch(
                    config_id,
                    cwd,
                    options.project_id.as_deref(),
                    options.session_id.as_deref(),
                )
            })
            .flatten();
        let mut waiter = Waiter {
            caller,
            config_id: config_id.to_owned(),
            reuse_key: String::new(),
            options,
        };
        let Some(launch) = launch else {
            return self.ensured(waiter, Ok(None), fx);
        };
        let key = launch.reuse_key.clone();
        waiter.reuse_key.clone_from(&key);
        let holder = waiter.options.holder;
        if let Some(current) = self.live_agent(&key) {
            let stale = self
                .book
                .ensure
                .signatures
                .get(&current)
                .is_some_and(|recorded| *recorded != launch.signature);
            if stale || held_against(self.agent_holder(&current), holder) {
                self.config_to_live_agent.shift_remove(&key);
                self.config_to_live_agent
                    .insert(detached_reuse_key(&key, &current), json!(current));
                let message = if stale {
                    format!(
                        "Detached agent {current}: its launch (env or Claude account) no longer matches the project"
                    )
                } else {
                    format!("Detached agent {current} so the next Agent chat gets its own process")
                };
                fx.push(Effect::info(SOURCE, message));
            }
        }
        if let Some(existing) = self.live_agent(&key)
            && self.agent_status.get(&existing) == Some(&json!("connected"))
        {
            if let Some(holder) = holder {
                self.hold_agent(&existing, holder);
            }
            return self.ensured(waiter, Ok(Some(existing)), fx);
        }
        if let Some(&running) = self.book.ensure.in_flight.get(&key) {
            let spawn = self
                .book
                .ensure
                .spawns
                .get_mut(&running)
                .expect("in flight");
            let outdated = spawn.signature != launch.signature;
            let held = held_against(spawn_holder(&spawn.holders), holder);
            if !outdated && !held {
                spawn.holders.extend(holder);
                spawn.waiters.push(waiter);
                return;
            }
            let message = if outdated {
                format!(
                    "Not joining the running spawn for {key}: it started with an outdated launch"
                )
            } else {
                format!(
                    "Not joining the running spawn for {key}: a {} holds it, so this {} gets its own process",
                    Holder::name(spawn_holder(&spawn.holders)),
                    Holder::name(holder)
                )
            };
            fx.push(Effect::info(SOURCE, message));
        }
        let options = &waiter.options;
        if options.silent
            && !options.surface_definite
            && self.book.ensure.definite_rejections.get(&key) == Some(&launch.signature)
        {
            return self.ensured(waiter, Ok(None), fx);
        }
        if options.register_warm_ui {
            self.warming_configs.insert(key.clone(), json!(true));
        }
        let launch_id = text(&launch.config, "id");
        if !launch.project_id.is_empty()
            && self.other_project_holds_agent(&launch_id, cwd, &launch.project_id)
        {
            fx.push(Effect::info(
                SOURCE,
                format!(
                    "Project {} spawns its own agent for {config_id} at {cwd}; another project already holds that cwd",
                    launch.project_id
                ),
            ));
        }
        self.start_spawn(launch, waiter, fx);
    }

    fn start_spawn(&mut self, launch: Launch, waiter: Waiter, fx: &mut Vec<Effect>) {
        let env_vars = launch
            .project
            .as_ref()
            .and_then(|project| project.get("envVars"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if env_vars > 0 {
            fx.push(Effect::info(
                SOURCE,
                format!(
                    "Spawning {} with the env of project {} ({env_vars} var(s))",
                    text(&launch.config, "name"),
                    super::js::string_of(launch.project.as_ref().and_then(|p| p.get("id")))
                ),
            ));
        }
        let launch_id = text(&launch.config, "id");
        let mut config = Map::new();
        config.insert("configId".into(), json!(launch_id));
        for key in ["name", "command", "args"] {
            if let Some(value) = launch.config.get(key) {
                config.insert(key.into(), value.clone());
            }
        }
        config.insert("env".into(), Value::Object(launch.env));
        if let Some(value) = launch.config.get("allowTerminal") {
            config.insert("allowTerminal".into(), value.clone());
        }
        let book = &mut self.book.ensure;
        book.seq += 1;
        let id = book.seq;
        book.in_flight.insert(launch.reuse_key.clone(), id);
        book.spawns.insert(
            id,
            Spawn {
                reuse_key: launch.reuse_key,
                launch_id,
                signature: launch.signature,
                holders: waiter.options.holder.into_iter().collect(),
                register_warm_ui: waiter.options.register_warm_ui,
                waiters: Vec::new(),
            },
        );
        let project_id = (!launch.project_id.is_empty()).then(|| json!(launch.project_id));
        let session_id = waiter.options.session_id.clone();
        book.spawns
            .get_mut(&id)
            .expect("inserted above")
            .waiters
            .push(waiter);
        self.spawn_agent(Value::Object(config), project_id, session_id, Some(id), fx);
    }

    /// The ensure's spawn answered with this agent.
    pub(crate) fn on_ensure_spawned(
        &mut self,
        id: EnsureId,
        agent_id: String,
        fx: &mut Vec<Effect>,
    ) {
        let Some(spawn) = self.book.ensure.spawns.get(&id).cloned() else {
            return;
        };
        let book = &mut self.book.ensure;
        book.definite_rejections.remove(&spawn.reuse_key);
        let still_configured = self.agent_configs.iter().any(|config| {
            config.get("id").and_then(Value::as_str) == Some(spawn.launch_id.as_str())
        });
        if !still_configured {
            return self.kill_agent(agent_id, Some(id), fx);
        }
        book.signatures
            .insert(agent_id.clone(), spawn.signature.clone());
        // A newer launch took the key while this spawn ran: keep this agent
        // for its own callers only.
        let key = if book.in_flight.get(&spawn.reuse_key) == Some(&id) {
            spawn.reuse_key.clone()
        } else {
            detached_reuse_key(&spawn.reuse_key, &agent_id)
        };
        self.config_to_live_agent.insert(key, json!(agent_id));
        for holder in spawn.holders {
            self.hold_agent(&agent_id, holder);
        }
        self.finish_spawn(id, Ok(Some(agent_id)), fx);
    }

    /// The ensure's spawn is over: answered, failed, or its agent killed
    /// because the config went meanwhile.
    pub(crate) fn finish_spawn(
        &mut self,
        id: EnsureId,
        outcome: Result<Option<String>, String>,
        fx: &mut Vec<Effect>,
    ) {
        let book = &mut self.book.ensure;
        let Some(spawn) = book.spawns.remove(&id) else {
            return;
        };
        if let Err(message) = &outcome
            && tagged(message, &DEFINITE_SPAWN_REJECTIONS)
        {
            book.definite_rejections
                .insert(spawn.reuse_key.clone(), spawn.signature.clone());
        }
        if book.in_flight.get(&spawn.reuse_key) == Some(&id) {
            book.in_flight.remove(&spawn.reuse_key);
        }
        if spawn.register_warm_ui {
            self.warming_configs.remove(&spawn.reuse_key);
        }
        for waiter in spawn.waiters {
            self.ensured(waiter, outcome.clone(), fx);
        }
    }

    /// `asCallerView`, then the caller's own continuation.
    fn ensured(
        &mut self,
        waiter: Waiter,
        outcome: Result<Option<String>, String>,
        fx: &mut Vec<Effect>,
    ) {
        let options = &waiter.options;
        let outcome = match outcome {
            Err(message)
                if options.silent
                    && !(options.surface_definite
                        && (tagged(&message, &DEFINITE_SPAWN_REJECTIONS)
                            || tagged(&message, &SIGNED_OUT))) =>
            {
                fx.push(Effect::warn(
                    SOURCE,
                    format!(
                        "Silent spawn failure for config {} ({}): {message}",
                        waiter.config_id, waiter.reuse_key
                    ),
                ));
                Ok(None)
            }
            outcome => outcome,
        };
        match (waiter.caller, outcome) {
            (Caller::Prewarm, Ok(_)) => {}
            (Caller::Prewarm, Err(message)) => fx.push(Effect::rejected(message)),
            (Caller::ImportList(id), Ok(agent_id)) => self.on_import_agent(id, agent_id, fx),
            (Caller::ImportList(id), Err(message)) => self.on_import_list_failed(id, message, fx),
        }
    }
}
