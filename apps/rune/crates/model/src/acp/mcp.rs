//! The MCP server registry, its probes, OAuth sign-ins and the agent's
//! install requests (`slices/mcp.ts`).
//!
//! Every disk, probe and OAuth call is an effect the app answers with
//! `Event::McpCommandDone` or `Event::McpCommandFailed`. Registry writes run
//! one at a time, as the TypeScript promise queue runs them: a write that
//! arrives while one is saving waits, and a failed save puts the list back.
//! An OAuth flow the model aborts (a newer flow, a cancel, a delete) ends as
//! cancelled at once; the app's later answer for it is ignored.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Deserialize;
use serde_json::{Value, json};

use super::js::{self, string_of};
use super::mcp_install::catalog_entry;
use super::types::Object;
use super::{Effect, Env, State};

pub const OAUTH_CANCELLED: &str = "OAUTH_CANCELLED";

/// A failed MCP command: the error's raw message, and the code an
/// `McpOAuthError` carries.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct McpError {
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
}

impl McpError {
    pub(super) fn name(&self) -> &'static str {
        if self.code.is_some() {
            "McpOAuthError"
        } else {
            "Error"
        }
    }

    /// `String(error)`.
    pub(super) fn shown(&self) -> String {
        format!("{}: {}", self.name(), self.message)
    }

    pub(super) fn rejected(&self) -> Effect {
        Effect::ActionRejected {
            message: self.shown(),
        }
    }
}

/// The arguments of `answerMcpInstall`.
pub(crate) struct InstallAnswer {
    pub request_id: String,
    pub decision: String,
    pub inputs: Option<Value>,
    pub secrets: Option<Value>,
    pub plan_id: Option<Value>,
    pub digest: Option<Value>,
}

/// What a token check answers for.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenCheck {
    /// `checkMcpOAuthStatus`.
    Status { server_id: String, deletions: u64 },
    /// A paste-back the host no longer knew: a stored token means it was
    /// used already.
    Paste {
        server_id: String,
        server_name: String,
        error: McpError,
    },
}

/// A registry write waiting for its turn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RegistryOp {
    Save(Object),
    Import(Vec<Object>),
    Catalog(Value),
    SetEnabled(String, Value),
    Delete(String),
    Reload,
}

/// What a saved write does once the save answered.
#[derive(Clone, Debug, PartialEq)]
enum After {
    Save {
        previous: Option<Object>,
        server: Object,
    },
    Import,
    Catalog {
        label: String,
        name: String,
        digest: String,
    },
    SetEnabled,
    Delete(String),
}

impl After {
    /// The log source and the failure line, before the error.
    fn failure(&self) -> (&'static str, String) {
        match self {
            Self::Save { .. } => (
                "acp-store.saveMcpServer",
                "Failed to persist MCP registry".into(),
            ),
            Self::Import => (
                "acp-store.importMcpServers",
                "Failed to persist MCP registry import".into(),
            ),
            Self::Catalog { label, .. } => (
                "acp-store.saveCatalogInstall",
                format!("Failed to persist the catalog install of {label}"),
            ),
            Self::SetEnabled => (
                "acp-store.setMcpServerEnabled",
                "Failed to persist MCP registry toggle".into(),
            ),
            Self::Delete(_) => (
                "acp-store.deleteMcpServer",
                "Failed to persist MCP registry deletion".into(),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Saving {
    previous: Vec<Object>,
    after: After,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Flow {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) url: String,
}

/// The module-level MCP bookkeeping of the TypeScript slice.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct McpBook {
    queue: VecDeque<RegistryOp>,
    saving: Option<Saving>,
    reloading: bool,
    /// Per server id, the completed deletions: a probe or OAuth answer
    /// for a server deleted meanwhile never lands.
    pub(super) deletions: BTreeMap<String, u64>,
    /// The running OAuth flow per server id.
    pub(super) flows: BTreeMap<String, Flow>,
    pub(super) flow_seq: u64,
    /// Servers whose running probe started before a sign-in changed.
    pub(super) reprobe_after: BTreeSet<String>,
    /// Names of the flows this step aborted, whose cancel is still to log.
    pub(super) aborted: Vec<String>,
}

pub(super) fn id_of(server: &Object) -> &str {
    server.get("id").and_then(Value::as_str).unwrap_or_default()
}

/// `probeConfigOf`: the server without the fields a probe ignores.
fn probe_config(server: &Object) -> Object {
    let mut config = server.clone();
    for key in ["id", "enabled", "catalog"] {
        config.remove(key);
    }
    config
}

/// `JSON.stringify` of both probe configs: key order counts, as there.
fn same_probe_config(a: &Object, b: &Object) -> bool {
    serde_json::to_string(&probe_config(a)).ok() == serde_json::to_string(&probe_config(b)).ok()
}

fn objects(value: &Value) -> Vec<Object> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_object().cloned())
        .collect()
}

pub(super) fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

impl State {
    pub(super) fn mcp_server(&self, id: &str) -> Option<&Object> {
        self.mcp_servers.iter().find(|server| id_of(server) == id)
    }

    pub(super) fn mcp_deletions(&self, id: &str) -> u64 {
        self.book.mcp.deletions.get(id).copied().unwrap_or(0)
    }

    /// The server's OAuth URL, for an `http` or `sse` server that has one.
    pub(super) fn mcp_oauth_target(&self, id: &str) -> Option<(String, String)> {
        let server = self.mcp_server(id)?;
        let url = server.get("url").and_then(Value::as_str)?;
        (!url.is_empty()).then(|| (string_of(server.get("name")), url.to_owned()))
    }

    /// `loadMcpServers`.
    pub(crate) fn load_mcp_servers(&self, fx: &mut Vec<Effect>) {
        fx.push(Effect::LoadMcpServers { reload: false });
    }

    /// A registry write: run now, or after the write in flight.
    pub(crate) fn queue_mcp_write(
        &mut self,
        op: RegistryOp,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        if matches!(&op, RegistryOp::Import(servers) if servers.is_empty()) {
            return;
        }
        self.book.mcp.queue.push_back(op);
        self.run_mcp_writes(env, fx);
    }

    fn run_mcp_writes(&mut self, env: &mut Env<'_>, fx: &mut Vec<Effect>) {
        while self.book.mcp.saving.is_none() && !self.book.mcp.reloading {
            let Some(op) = self.book.mcp.queue.pop_front() else {
                return;
            };
            self.run_mcp_write(op, env, fx);
        }
    }

    fn run_mcp_write(&mut self, op: RegistryOp, env: &mut Env<'_>, fx: &mut Vec<Effect>) {
        let previous = self.mcp_servers.clone();
        let (next, after) = match op {
            RegistryOp::Reload => {
                self.book.mcp.reloading = true;
                return fx.push(Effect::LoadMcpServers { reload: true });
            }
            RegistryOp::Save(mut server) => {
                if present(server.get("enabled")).is_none() {
                    server.insert("enabled".into(), json!(true));
                }
                let id = id_of(&server).to_owned();
                let mut next = previous.clone();
                let before = next.iter().position(|item| id_of(item) == id);
                match before {
                    Some(index) => next[index] = server.clone(),
                    None => next.push(server.clone()),
                }
                let previous = before.map(|index| previous[index].clone());
                (next, After::Save { previous, server })
            }
            RegistryOp::Import(servers) => {
                let mut next = previous.clone();
                next.extend(servers);
                (next, After::Import)
            }
            RegistryOp::Catalog(result) => {
                if !self.mcp_servers_loaded {
                    return fx.push(Effect::rejected("The MCP server list has not loaded yet"));
                }
                let id = env.ids.next_id();
                let entry = match catalog_entry(&result, &previous, id) {
                    Ok(entry) => entry,
                    Err(message) => return fx.push(Effect::rejected(message)),
                };
                let provenance = result.get("provenance");
                let label = format!(
                    "'{}' {}",
                    string_of(provenance.and_then(|p| p.get("name"))),
                    string_of(provenance.and_then(|p| p.get("version")))
                );
                let digest = string_of(result.get("launchDigest"));
                let after = After::Catalog {
                    label,
                    name: string_of(entry.get("name")),
                    digest: js::head(&digest, 12).to_owned(),
                };
                let mut next = previous.clone();
                next.push(entry);
                (next, after)
            }
            RegistryOp::SetEnabled(id, enabled) => {
                let mut next = previous.clone();
                for server in next.iter_mut().filter(|server| id_of(server) == id) {
                    server.insert("enabled".into(), enabled.clone());
                }
                (next, After::SetEnabled)
            }
            RegistryOp::Delete(id) => {
                let next = previous
                    .iter()
                    .filter(|server| id_of(server) != id)
                    .cloned()
                    .collect();
                (next, After::Delete(id))
            }
        };
        self.mcp_servers = next.clone();
        self.book.mcp.saving = Some(Saving { previous, after });
        fx.push(Effect::SaveMcpServers { servers: next });
    }

    /// The registry was read: for `loadMcpServers`, or a reload in the
    /// write queue.
    fn on_mcp_servers_loaded(
        &mut self,
        reload: bool,
        outcome: Result<Value, McpError>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        match outcome {
            Ok(list) => {
                self.mcp_servers = objects(&list);
                self.mcp_servers_loaded = true;
                if !reload && !self.book.mcp_shared_sync {
                    self.book.mcp_shared_sync = true;
                    fx.push(Effect::WatchSharedMcpServers);
                }
            }
            Err(error) if !reload => {
                fx.push(Effect::error(
                    "acp-store.loadMcpServers",
                    format!("Failed to load MCP registry ({})", error.shown()),
                ));
                fx.push(Effect::Toast {
                    level: "error".into(),
                    message: "Could not load MCP servers. Try reopening Settings.".into(),
                });
            }
            Err(_) => {}
        }
        if reload {
            self.book.mcp.reloading = false;
            self.run_mcp_writes(env, fx);
        }
    }

    fn on_mcp_saved(&mut self, failure: Option<McpError>, env: &mut Env<'_>, fx: &mut Vec<Effect>) {
        let Some(Saving { previous, after }) = self.book.mcp.saving.take() else {
            return;
        };
        if let Some(error) = failure {
            self.mcp_servers = previous;
            let (source, line) = after.failure();
            fx.push(Effect::error(source, format!("{line} ({})", error.shown())));
            fx.push(error.rejected());
            return self.run_mcp_writes(env, fx);
        }
        match after {
            After::Save {
                previous: Some(previous),
                server,
            } if !same_probe_config(&previous, &server) => self.reprobe_edited(&server, fx),
            After::Catalog {
                label,
                name,
                digest,
            } => fx.push(Effect::info(
                "acp-store.saveCatalogInstall",
                format!("Saved the catalog install of {label} as '{name}' digest={digest}"),
            )),
            After::Delete(id) => {
                *self.book.mcp.deletions.entry(id.clone()).or_default() += 1;
                self.abort_oauth_flow(&id, fx);
                self.drop_probe_state(&id);
                self.mcp_oauth_connected.remove(&id);
                self.mcp_oauth_connecting.remove(&id);
                self.mcp_oauth_paste_back.remove(&id);
                self.log_aborted_flows(fx);
            }
            _ => {}
        }
        self.run_mcp_writes(env, fx);
    }

    fn drop_probe_state(&mut self, id: &str) {
        self.mcp_probe_status.remove(id);
        self.mcp_tools.remove(id);
        self.mcp_tools_loaded.remove(id);
        self.mcp_probing.remove(id);
        self.mcp_probe_error.remove(id);
    }

    /// The shared registry another browser or device saved.
    pub(crate) fn on_mcp_shared_changed(&mut self, change: Value, fx: &mut Vec<Effect>) {
        if !self.book.mcp_shared_sync {
            return;
        }
        let previous = std::mem::take(&mut self.mcp_servers);
        self.mcp_servers = if change.get("exists").is_some_and(js::truthy) {
            change.get("data").map(objects).unwrap_or_default()
        } else {
            Vec::new()
        };
        for server in self.mcp_servers.clone() {
            let before = previous.iter().find(|item| id_of(item) == id_of(&server));
            if before.is_some_and(|before| !same_probe_config(before, &server)) {
                self.reprobe_edited(&server, fx);
            }
        }
    }

    /// `reprobeEditedServer`: forget what the old config probed.
    fn reprobe_edited(&mut self, server: &Object, fx: &mut Vec<Effect>) {
        let id = id_of(server).to_owned();
        self.mcp_probe_status.remove(&id);
        self.mcp_tools.insert(id.clone(), json!([]));
        self.mcp_tools_loaded.insert(id.clone(), json!(false));
        self.mcp_probe_error.remove(&id);
        if server.get("enabled") != Some(&json!(false)) {
            self.probe_mcp_server(&id, fx);
        }
    }

    /// `probeMcpServer`: one probe per server at a time.
    pub(crate) fn probe_mcp_server(&mut self, id: &str, fx: &mut Vec<Effect>) {
        if self.mcp_probing.get(id).is_some_and(js::truthy) {
            return;
        }
        let Some(server) = self.mcp_server(id).cloned() else {
            return;
        };
        let deletions = self.mcp_deletions(id);
        self.mcp_probing.insert(id.to_owned(), json!(true));
        fx.push(Effect::ProbeMcpServer {
            config: probe_config(&server),
            server,
            deletions,
        });
    }

    /// `loadMcpTools`: probe unless a reachable result is already in.
    pub(crate) fn load_mcp_tools(&mut self, id: &str, fx: &mut Vec<Effect>) {
        if self.mcp_probing.get(id).is_some_and(js::truthy) {
            return;
        }
        let reachable = self.mcp_tools_loaded.get(id).is_some_and(js::truthy)
            && self.mcp_probe_status.get(id) != Some(&json!("disconnected"));
        if !reachable {
            self.probe_mcp_server(id, fx);
        }
    }

    pub(super) fn probe_after_in_flight(&mut self, id: &str, fx: &mut Vec<Effect>) {
        if self.mcp_probing.get(id).is_some_and(js::truthy) {
            self.book.mcp.reprobe_after.insert(id.to_owned());
        } else {
            self.probe_mcp_server(id, fx);
        }
    }

    fn on_mcp_probed(
        &mut self,
        probed: Object,
        deletions: u64,
        outcome: Result<Value, McpError>,
        fx: &mut Vec<Effect>,
    ) {
        let id = id_of(&probed).to_owned();
        if let Err(error) = &outcome {
            fx.push(Effect::error(
                "acp-store.probeMcpServer",
                format!(
                    "MCP probe failed for server '{}' ({})",
                    string_of(probed.get("name")),
                    error.shown()
                ),
            ));
        }
        if self.settle_stale_probe(&probed, deletions, fx) {
            return;
        }
        self.mcp_probing.insert(id.clone(), json!(false));
        let Ok(result) = outcome else {
            self.mcp_probe_status
                .insert(id.clone(), json!("disconnected"));
            self.mcp_tools.insert(id.clone(), json!([]));
            self.mcp_tools_loaded.insert(id.clone(), json!(false));
            self.mcp_probe_error.remove(&id);
            return;
        };
        let status = result.get("status").cloned();
        set_or_remove(&mut self.mcp_probe_status, &id, status.clone());
        set_or_remove(&mut self.mcp_tools, &id, result.get("tools").cloned());
        self.mcp_tools_loaded.insert(id.clone(), json!(true));
        let error = match status.as_ref().and_then(Value::as_str) {
            Some("connected") => None,
            _ => result.get("error").cloned(),
        };
        set_or_remove(&mut self.mcp_probe_error, &id, error);
    }

    /// `settleStaleProbe`: true when the result must not land, because the
    /// server was deleted, edited, or signed in or out while it probed.
    fn settle_stale_probe(
        &mut self,
        probed: &Object,
        deletions: u64,
        fx: &mut Vec<Effect>,
    ) -> bool {
        let id = id_of(probed).to_owned();
        let reprobe = self.book.mcp.reprobe_after.remove(&id);
        let deleted = self.mcp_deletions(&id) != deletions;
        let Some(current) = self.mcp_server(&id).cloned() else {
            return deleted;
        };
        if !reprobe && same_probe_config(&current, probed) {
            return false;
        }
        self.mcp_probing.insert(id.clone(), json!(false));
        if current.get("enabled") != Some(&json!(false)) {
            self.probe_mcp_server(&id, fx);
        }
        true
    }

    /// The answer to an MCP command effect.
    pub(crate) fn on_mcp_command(
        &mut self,
        command: Effect,
        outcome: Result<Value, McpError>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        match command {
            Effect::LoadMcpServers { reload } => {
                self.on_mcp_servers_loaded(reload, outcome, env, fx)
            }
            Effect::SaveMcpServers { .. } => self.on_mcp_saved(outcome.err(), env, fx),
            Effect::ProbeMcpServer {
                server, deletions, ..
            } => self.on_mcp_probed(server, deletions, outcome, fx),
            Effect::StartMcpOAuth { .. }
            | Effect::CompleteMcpOAuth { .. }
            | Effect::HasMcpOAuthToken { .. }
            | Effect::DisconnectMcpOAuth { .. } => self.on_mcp_oauth_command(command, outcome, fx),
            Effect::AnswerMcpInstall { request, decision } => {
                self.on_mcp_install_answered(&request, &decision, outcome, fx);
            }
            _ => {}
        }
    }
}

fn set_or_remove(map: &mut BTreeMap<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(value) => map.insert(key.to_owned(), value),
        None => map.remove(key),
    };
}
