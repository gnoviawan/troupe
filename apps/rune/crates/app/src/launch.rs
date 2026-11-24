//! Starting a chat from the launcher's composer (`AgentLauncher.launch`): spawn the picked
//! agent for the project, create a session in the project's folder, open its tab, then send
//! the draft through `model::acp` as a chat's composer does. The model's launch slice (warm
//! sessions, configs) is a later R6a unit; until then the app runs these three commands.

use model::acp::Event;
use model::acp::prompt::SendOptions;
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};
use ui::composer::Send;
use ui::composer::field;

use crate::State;
use crate::composer::Host;
use crate::sync::Pending;

/// A launch in flight: the draft's send, waiting for its session.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub project_id: String,
    /// The stored config's id: the new chat's chip names it before its history says.
    pub config_id: String,
    pub cwd: String,
    pub send: Send,
}

impl State {
    pub(crate) fn launch_from(&mut self, host: &Host) {
        let Host::Launcher(project_id) = host else { return };
        if self.composer.launching.is_some() || !self.launcher_ready(project_id) {
            return;
        }
        let Some(draft) = self.draft(host) else { return };
        let doc = field::doc_of(&draft.edit);
        let send = match ui::composer::compose(&doc, &[], None, &draft.attachments) {
            Ok(Some(send)) => send,
            Ok(None) => return,
            Err(e) => {
                self.toast_error(&e.to_string(), "");
                return;
            }
        };
        let picked = self.composer.picked.clone();
        let agents = &self.composer.agents;
        let Some((config_id, config)) = picked
            .and_then(|id| agents.iter().find(|a| a.id == id))
            .or_else(|| agents.first())
            .map(|a| (a.id.clone(), spawn_config(&a.raw)))
        else {
            self.toast_error("No agent to start", "Add an agent in Settings first.");
            return;
        };
        let cwd = self
            .projects
            .store
            .projects
            .iter()
            .find(|p| &p.id == project_id)
            .and_then(|p| p.path.clone())
            .unwrap_or_default();
        log::info!("[launch] starting {config_id} in {project_id}");
        self.composer.launching = Some(Launch {
            project_id: project_id.clone(),
            config_id,
            cwd,
            send,
        });
        self.command(
            WsRequestType::SpawnAgent,
            json!({ "config": config, "projectId": project_id }),
            Pending::LaunchSpawn,
        );
    }

    /// A launch needs the saved agent read, and `project_id`'s own workspace tree showing, so
    /// the new tab lands in that project.
    pub(crate) fn launcher_ready(&self, project_id: &str) -> bool {
        self.composer.picked_read && self.workspace.project() == Some(project_id)
    }

    fn launch_failed(&mut self, step: &str, e: &WsError) {
        log::warn!("[launch] {step} failed: {}: {}", e.code, e.message);
        self.composer.launching = None;
        self.toast_error("Could not start the chat", &e.message);
    }

    pub(crate) fn launch_spawned(&mut self, result: Result<Option<Value>, WsError>) {
        let reply = match result {
            Ok(reply) => reply.unwrap_or(Value::Null),
            Err(e) => return self.launch_failed("spawning the agent", &e),
        };
        let (Some(agent_id), Some(launch)) = (
            reply.get("agentId").and_then(Value::as_str).map(str::to_owned),
            self.composer.launching.clone(),
        ) else {
            self.composer.launching = None;
            return log::warn!("[launch] the spawn reply names no agent");
        };
        // The spawn reply is the agent's registration (CAP-4): its capabilities decide images.
        match serde_json::from_value::<model::acp::agents::AgentSpawnedEvent>(reply.clone()) {
            Ok(spawned) => self.acp_step(Event::AgentSpawned(spawned)),
            Err(e) => log::warn!("[launch] unreadable spawn reply for {agent_id}: {e}"),
        }
        let image_capable = reply.pointer("/capabilities/promptCapabilities/image").and_then(Value::as_bool) == Some(true);
        if sends_image(&launch.send) && !image_capable {
            log::warn!("[launch] {agent_id} takes no images; the draft stays");
            self.composer.launching = None;
            self.toast_error(crate::composer_agents::NO_IMAGES, "");
            return;
        }
        self.command(
            WsRequestType::CreateSession,
            json!({ "agentId": agent_id, "cwd": launch.cwd, "projectId": launch.project_id }),
            Pending::LaunchSession(agent_id),
        );
    }

    pub(crate) fn launch_created(&mut self, agent_id: String, result: Result<Option<Value>, WsError>) {
        let reply = match result {
            Ok(reply) => reply.unwrap_or(Value::Null),
            Err(e) => return self.launch_failed("creating the session", &e),
        };
        let Some(launch) = self.composer.launching.take() else {
            return;
        };
        let Some(session_id) = reply.get("sessionId").and_then(Value::as_str).map(str::to_owned) else {
            return log::warn!("[launch] the session reply names no session");
        };
        log::info!("[launch] chat {session_id} on agent {agent_id}");
        self.install_new_chat(&session_id, &agent_id);
        self.composer
            .namespaces
            .insert(session_id.clone(), format!("config:{}", launch.config_id));
        self.composer.launched.insert(
            session_id.clone(),
            crate::composer::Launched { project_id: launch.project_id.clone(), cwd: launch.cwd.clone() },
        );
        if self.workspace.project() != Some(launch.project_id.as_str()) {
            // Another project came on screen while the agent spawned: open the chat in its own
            // project, as the web client's route to the new chat does.
            self.select_project(&launch.project_id);
        }
        let host = Host::Launcher(launch.project_id.clone());
        let draft = self.draft_mut(&host);
        draft.edit.clear();
        draft.attachments.clear();
        if self.workspace.project() == Some(launch.project_id.as_str()) {
            self.workspace
                .step(model::workspace::Event::AddAgentChatTab(session_id.clone(), None));
        } else {
            log::warn!("[launch] {} is not on screen; chat {session_id} opens from the rail", launch.project_id);
        }
        let event = match launch.send {
            Send::Text { text } => Event::SendPrompt(session_id, text),
            Send::Blocks { wire, display } => Event::SendPromptBlocks(
                session_id,
                wire,
                Some(SendOptions {
                    display_blocks: display,
                }),
            ),
        };
        self.acp_step(event);
    }
}

fn sends_image(send: &Send) -> bool {
    match send {
        Send::Text { .. } => false,
        Send::Blocks { wire, .. } => wire.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("image")),
    }
}

/// The `spawn_agent` config of a stored agent (`StoredAgentConfig` to `AgentConfig`): the host
/// takes `configId`, `name`, `command`, `args`, `env` and `allowTerminal`, nothing else.
fn spawn_config(stored: &Value) -> Value {
    let mut config = serde_json::Map::new();
    config.insert("configId".into(), stored.get("id").cloned().unwrap_or(Value::Null));
    for field in ["name", "command", "args", "env", "allowTerminal"] {
        if let Some(v) = stored.get(field) {
            config.insert(field.into(), v.clone());
        }
    }
    Value::Object(config)
}
