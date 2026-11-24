//! `createSession` (`slices/session.ts`): open a new agent session with
//! `session/new`, through the sign-in retry, and add its chat.
//!
//! The call is the `newSession` effect the app answers with
//! `agentCommandDone` and the reply, or `agentCommandFailed`. The action
//! answers its caller with `actionResolved` and the session id, or
//! `actionRejected`.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::auth_retry::{SetupCategory, setup_category};
use super::host_error::HostError;
use super::import::RequestId;
use super::session::{SessionCreatedEvent, add_advertised_options, creation_option_defaults};
use super::types::{Session, SessionStatus};
use super::{Effect, Env, State, js};

/// Names one `createSession` call.
pub type CreateId = u64;

const LOOKUP_SOURCE: &str = "acp.createSession.lookup";

/// The project a quick chat belongs to (`QUICK_CHAT_PROJECT_ID`).
const QUICK_CHAT_PROJECT: &str = "__unassigned__";

/// `createSession`'s options.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOptions {
    /// A warm-pool session: not persisted, never the active chat.
    #[serde(default)]
    pub ephemeral: bool,
    /// The host keeps the session out of the history.
    #[serde(default)]
    pub backend_ephemeral: bool,
    #[serde(default)]
    pub promotable: bool,
    #[serde(default)]
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub worktree_branch: Option<String>,
    /// `quick` for a quick chat.
    #[serde(default)]
    pub kind: Option<String>,
}

/// The arguments of `createSession`, in its order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CreateRequest(
    pub String,
    pub String,
    /// The MCP servers to start with; `None` picks the registry's.
    #[serde(default)]
    pub Option<Vec<Value>>,
    #[serde(default)] pub Option<String>,
    #[serde(default)] pub Option<CreateOptions>,
    /// The app's name for the call, echoed on its answer.
    #[serde(default)]
    pub RequestId,
);

/// Who a `createSession` answers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CreateCaller {
    /// The user's action, by the app's request id.
    Action(RequestId),
    OneShot(super::oneshot::OneShotId),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Create {
    by: CreateCaller,
    agent_id: String,
    cwd: String,
    project_id: Option<String>,
    options: CreateOptions,
    mcp_server_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CreateBook {
    seq: CreateId,
    creates: BTreeMap<CreateId, Create>,
    /// Calls whose agent is being killed after a broken connection, with
    /// the error their caller sees once the kill is over.
    evicting: BTreeMap<CreateId, HostError>,
}

/// `selectMcpServersForAgent`: the enabled registry servers the agent can
/// start, as the wire sends them, and the names of the ones it cannot.
pub(crate) fn select_mcp_servers(
    registry: &[super::types::Object],
    capabilities: Option<&Value>,
) -> (Vec<Value>, Vec<String>) {
    let pending = capabilities.is_none_or(Value::is_null);
    let mut servers = Vec::new();
    let mut skipped = Vec::new();
    for entry in registry {
        if entry.get("enabled") == Some(&Value::Bool(false)) {
            continue;
        }
        let transport = entry
            .get("type")
            .filter(|kind| !kind.is_null())
            .map_or_else(|| "stdio".to_owned(), |kind| js::string_of(Some(kind)));
        let supported = capabilities
            .and_then(|caps| caps.get("mcpCapabilities"))
            .and_then(|caps| caps.get(&transport))
            == Some(&Value::Bool(true));
        if transport == "stdio" || pending || supported {
            servers.push(wire_server(entry, &transport));
        } else {
            skipped.push(js::string_of(entry.get("name")));
        }
    }
    (servers, skipped)
}

/// `toWireServer`: the ACP `McpServer`, with the lists the disk omits.
fn wire_server(entry: &super::types::Object, transport: &str) -> Value {
    let list = |key: &str| entry.get(key).cloned().unwrap_or_else(|| json!([]));
    let name = entry.get("name").cloned().unwrap_or(Value::Null);
    match transport {
        "stdio" => json!({
            "type": "stdio",
            "name": name,
            "command": entry.get("command").cloned().unwrap_or(Value::Null),
            "args": list("args"),
            "env": list("env"),
        }),
        _ => json!({
            "type": transport,
            "name": name,
            "url": entry.get("url").cloned().unwrap_or(Value::Null),
            "headers": list("headers"),
        }),
    }
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

impl State {
    /// `additionalWorkspaceRoots`: the Project's folder and its worktrees,
    /// for an agent that advertises `additionalDirectories`.
    pub(crate) fn additional_workspace_roots(
        &self,
        agent_id: &str,
        cwd: &str,
        project_id: Option<&str>,
    ) -> Vec<Value> {
        let supports = self
            .agents
            .get(agent_id)
            .and_then(|agent| {
                agent.pointer("/capabilities/sessionCapabilities/additionalDirectories")
            })
            .is_some_and(js::truthy);
        let project = project_id.filter(|id| !id.is_empty()).and_then(|id| {
            self.book
                .projects
                .iter()
                .find(|project| project.get("id").and_then(Value::as_str) == Some(id))
        });
        let (true, Some(project)) = (supports, project) else {
            return Vec::new();
        };
        let worktrees = project.get("worktrees").and_then(Value::as_array);
        let candidates = std::iter::once(project.get("path"))
            .chain(worktrees.into_iter().flatten().map(|item| item.get("path")));
        let mut roots: Vec<Value> = Vec::new();
        for path in candidates.flatten().filter_map(Value::as_str) {
            if path.is_empty()
                || path == cwd
                || !is_absolute(path)
                || roots.iter().any(|root| root == path)
            {
                continue;
            }
            roots.push(json!(path));
        }
        roots
    }

    /// `createSession`.
    pub(crate) fn create_session(&mut self, request: CreateRequest, fx: &mut Vec<Effect>) {
        let by = CreateCaller::Action(request.5);
        self.create_session_for(request, by, fx);
    }

    /// `createSession` for `by`, which its answer goes to.
    pub(crate) fn create_session_for(
        &mut self,
        request: CreateRequest,
        by: CreateCaller,
        fx: &mut Vec<Effect>,
    ) {
        let CreateRequest(agent_id, cwd, mcp_servers, project_id, options, _) = request;
        let options = options.unwrap_or_default();
        let servers = match mcp_servers {
            Some(servers) => servers,
            None => {
                let capabilities = self
                    .agents
                    .get(&agent_id)
                    .and_then(|agent| agent.get("capabilities"));
                let pending = capabilities.is_none_or(Value::is_null);
                let (servers, skipped) = select_mcp_servers(&self.mcp_servers, capabilities);
                if !pending && !skipped.is_empty() {
                    fx.push(Effect::Toast {
                        level: "warning".into(),
                        message: "Some MCP servers were skipped".into(),
                    });
                }
                servers
            }
        };
        let additional_directories =
            self.additional_workspace_roots(&agent_id, &cwd, project_id.as_deref());
        let book = &mut self.book.creates;
        book.seq += 1;
        let create = book.seq;
        book.creates.insert(
            create,
            Create {
                by,
                agent_id: agent_id.clone(),
                cwd: cwd.clone(),
                project_id: project_id.clone(),
                options: options.clone(),
                mcp_server_count: servers.len(),
            },
        );
        let present = |value: Option<String>| value.filter(|value| !value.is_empty());
        fx.push(Effect::NewSession {
            agent_id,
            cwd,
            mcp_servers: servers,
            ephemeral: options.backend_ephemeral,
            promotable: options.promotable,
            project_id: present(project_id),
            worktree_path: present(options.worktree_path),
            worktree_branch: present(options.worktree_branch),
            session_kind: options.kind.filter(|kind| kind == "quick"),
            additional_directories,
            create,
            retried: false,
        });
    }

    /// `session/new` answered: add the chat, or, for a resend the host
    /// answered from its records (`lookup`), name the session it made.
    pub(crate) fn on_session_new(
        &mut self,
        create: CreateId,
        reply: &Value,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(call) = self.book.creates.creates.get(&create).cloned() else {
            return;
        };
        let session_id = js::string_of(reply.get("sessionId"));
        if reply.get("lookup").is_some_and(js::truthy) {
            let agent_id = &call.agent_id;
            if call.options.ephemeral || call.options.backend_ephemeral {
                fx.push(Effect::warn(
                    LOOKUP_SOURCE,
                    format!("session/new for agent {agent_id} was answered by entity lookup (session {session_id}) but the session is ephemeral; it cannot be reopened from history"),
                ));
                let error = HostError::new(
                    "The temporary session could not be confirmed after the connection dropped. Try again.".into(),
                    None,
                );
                return self.on_create_failed(create, error, fx);
            }
            fx.push(Effect::warn(
                LOOKUP_SOURCE,
                format!("session/new for agent {agent_id} was answered by entity lookup (session {session_id}); opening it from history"),
            ));
            return self.open_looked_up_session(session_id, create, fx);
        }
        self.invalidate_session_reopen(&session_id);
        self.add_created_session(&call, &session_id, reply, env);
        if !call.options.ephemeral {
            fx.push(Effect::PersistSession {
                session_id: session_id.clone(),
            });
        }
        self.cache_options_from_session(&session_id, env, fx);
        self.resolve_create(create, session_id, env, fx);
    }

    /// The chat record, merged with what an event set while the call ran.
    fn add_created_session(
        &mut self,
        call: &Create,
        session_id: &str,
        reply: &Value,
        env: &Env<'_>,
    ) {
        let existing = self.sessions.get(session_id);
        let advertised = |key: &str| reply.get(key).filter(|value| !value.is_null()).cloned();
        let mut session = Session {
            id: session_id.to_owned(),
            agent_id: call.agent_id.clone(),
            status: match existing.map(|session| session.status) {
                Some(SessionStatus::Closed) => SessionStatus::Closed,
                _ => SessionStatus::Active,
            },
            title: existing.and_then(|session| session.title.clone()),
            active_turn: existing.is_some_and(|session| session.active_turn),
            open_turn_id: existing.and_then(|session| session.open_turn_id.clone()),
            agent_working: Some(
                self.book
                    .host_reports
                    .get(&(session_id.to_owned(), call.agent_id.clone()))
                    .and_then(|reports| reports.working)
                    .is_some_and(|(working, _)| working),
            ),
            replaying: Some(None),
            opencode_notice: None,
            last_error: Some(existing.and_then(|session| session.last_error.clone().flatten())),
            last_error_code: existing.and_then(|session| session.last_error_code),
            last_error_data: existing.and_then(|session| session.last_error_data.clone()),
            background_commands: None,
            rest: Default::default(),
        };
        let carried = |key: &str| {
            existing
                .and_then(|session| session.rest.get(key))
                .filter(|value| !value.is_null())
                .cloned()
        };
        let cwd = reply
            .get("cwd")
            .filter(|cwd| js::truthy(cwd))
            .cloned()
            .unwrap_or_else(|| json!(call.cwd));
        let quick = call.options.kind.as_deref() == Some("quick");
        let rest = &mut session.rest;
        rest.insert("cwd".into(), cwd);
        match (quick, &call.project_id) {
            (true, _) => {
                rest.insert("projectId".into(), json!(QUICK_CHAT_PROJECT));
            }
            (false, Some(project_id)) => {
                rest.insert("projectId".into(), json!(project_id));
            }
            (false, None) => {}
        }
        rest.insert("mcpServerCount".into(), json!(call.mcp_server_count));
        for key in ["modes", "models", "configOptions"] {
            let value = existing.and_then(|session| session.rest.get(key)).cloned();
            rest.insert(
                key.into(),
                value.unwrap_or_else(|| {
                    if key == "configOptions" {
                        json!([])
                    } else {
                        Value::Null
                    }
                }),
            );
        }
        let created = SessionCreatedEvent {
            agent_id: call.agent_id.clone(),
            session_id: session_id.to_owned(),
            modes: advertised("modes"),
            models: advertised("models"),
            config_options: advertised("configOptions")
                .and_then(|options| options.as_array().cloned()),
        };
        add_advertised_options(&mut session, &created);
        let defaults = carried("creationOptionDefaults").unwrap_or_else(|| {
            let options = created
                .config_options
                .clone()
                .or_else(|| {
                    carried("configOptions").and_then(|options| options.as_array().cloned())
                })
                .unwrap_or_default();
            creation_option_defaults(
                created.modes.clone().or_else(|| carried("modes")).as_ref(),
                created
                    .models
                    .clone()
                    .or_else(|| carried("models"))
                    .as_ref(),
                &options,
            )
        });
        let rest = &mut session.rest;
        rest.insert("creationOptionDefaults".into(), defaults);
        rest.insert(
            "createdAt".into(),
            carried("createdAt").unwrap_or_else(|| json!(env.now)),
        );
        for (key, value) in [
            ("worktreePath", call.options.worktree_path.clone()),
            ("worktreeBranch", call.options.worktree_branch.clone()),
            ("kind", call.options.kind.clone()),
        ] {
            if let Some(value) = value.map(Value::String).or_else(|| carried(key)) {
                rest.insert(key.into(), value);
            }
        }
        self.sessions.insert(session_id.to_owned(), session);
        self.messages.entry(session_id.to_owned()).or_default();
        if !call.options.ephemeral && self.active_session_id.is_none() {
            self.active_session_id = Some(session_id.to_owned());
        }
    }

    pub(crate) fn resolve_create(
        &mut self,
        create: CreateId,
        session_id: String,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        match self
            .book
            .creates
            .creates
            .remove(&create)
            .map(|call| call.by)
        {
            Some(CreateCaller::Action(request)) => fx.push(Effect::ActionResolved {
                request,
                value: json!(session_id),
            }),
            Some(CreateCaller::OneShot(id)) => self.on_oneshot_session(id, session_id, env, fx),
            None => {}
        }
    }

    /// `session/new` failed for good: a broken connection discards the
    /// process so a retry spawns a fresh one, and a sign-in failure lets a
    /// manual sign-in run again. The caller sees the error after that.
    pub(crate) fn on_create_failed(
        &mut self,
        create: CreateId,
        error: HostError,
        fx: &mut Vec<Effect>,
    ) {
        let Some(call) = self.book.creates.creates.get(&create) else {
            return;
        };
        let agent_id = call.agent_id.clone();
        match setup_category(&error) {
            SetupCategory::Transport => {
                // `evictAgentForTransport`: forget the sign-in, then kill.
                let book = &mut self.book;
                book.authenticated_agents.remove(&agent_id);
                let prefix = format!("{agent_id}\0");
                book.auth_flights.retain(|key, _| !key.starts_with(&prefix));
                book.last_auth_attempt.remove(&agent_id);
                book.creates.evicting.insert(create, error);
                return fx.push(Effect::KillAgent {
                    agent_id,
                    by: super::Killer::Evict(create),
                });
            }
            SetupCategory::Auth => {
                self.book.authenticated_agents.remove(&agent_id);
            }
            _ => {}
        }
        self.reject_create(create, &error, fx);
    }

    /// The eviction's kill is over, done or not: the caller sees the error.
    pub(crate) fn on_create_evicted(&mut self, create: CreateId, fx: &mut Vec<Effect>) {
        if let Some(error) = self.book.creates.evicting.remove(&create) {
            self.reject_create(create, &error, fx);
        }
    }

    fn reject_create(&mut self, create: CreateId, error: &HostError, fx: &mut Vec<Effect>) {
        match self
            .book
            .creates
            .creates
            .remove(&create)
            .map(|call| call.by)
        {
            Some(CreateCaller::Action(_)) => fx.push(Effect::rejected_by(error)),
            Some(CreateCaller::OneShot(id)) => {
                let message = super::effect::error_text(error);
                self.on_oneshot_session_failed(id, message, fx);
            }
            None => {}
        }
    }
}
