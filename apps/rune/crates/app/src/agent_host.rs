//! The model's agent and session commands on the host (`acp-api.ts` over the web transport):
//! `spawn_agent`, `create_session`, `kill_agent`, `promote_session`,
//! `dispose_ephemeral_session` and `record_agent_switch`, each answered back to the model as
//! `AgentCommandDone` or `AgentCommandFailed`, and the workspace effects an agent switch asks
//! for (the tab remap, the draft delete, its toasts).

use model::acp::{Effect, Event};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use crate::State;
use crate::sync::Pending;

/// The `record_agent_switch` fields of a switch marker.
const MARKER_FIELDS: [&str; 4] = ["fromConfigId", "toConfigId", "newSessionId", "summaryText"];

impl State {
    /// Runs an agent, session or switch effect; any other effect comes back.
    pub(crate) fn run_agent_command(&mut self, effect: Effect) -> Option<Effect> {
        let (kind, payload) = match &effect {
            Effect::SpawnAgent { config, project_id, session_id, .. } => (
                WsRequestType::SpawnAgent,
                json!({ "config": config, "projectId": project_id, "sessionId": session_id }),
            ),
            Effect::NewSession { agent_id, cwd, mcp_servers, ephemeral, promotable, project_id, additional_directories, .. } => {
                let mut payload = json!({ "agentId": agent_id, "cwd": cwd, "mcpServers": mcp_servers, "ephemeral": ephemeral });
                if *promotable {
                    payload["promotable"] = json!(true);
                }
                if let Some(project_id) = project_id {
                    payload["projectId"] = json!(project_id);
                }
                if !additional_directories.is_empty() {
                    payload["additionalDirectories"] = json!(additional_directories);
                }
                (WsRequestType::CreateSession, payload)
            }
            Effect::KillAgent { agent_id, .. } => (WsRequestType::KillAgent, json!({ "agentId": agent_id })),
            Effect::PromoteSession { agent_id, session_id } => {
                (WsRequestType::PromoteSession, json!({ "agentId": agent_id, "sessionId": session_id }))
            }
            Effect::DisposeEphemeralSession { agent_id, session_id, .. } => {
                (WsRequestType::DisposeEphemeralSession, json!({ "agentId": agent_id, "sessionId": session_id }))
            }
            Effect::RecordAgentSwitch { session_id, marker, .. } => {
                let mut payload = json!({ "sessionId": session_id });
                for field in MARKER_FIELDS {
                    payload[field] = marker.get(field).cloned().unwrap_or(Value::Null);
                }
                (WsRequestType::RecordAgentSwitch, payload)
            }
            Effect::DeletePersisted { key } => {
                let key = self.device_key(key);
                self.call(WsRequestType::StoreDelete, json!({ "key": key }), Pending::Write(key.clone()));
                return None;
            }
            Effect::RemapChatTab { from, to } => {
                log::info!("[chat] the tab of {from} now shows {to}");
                self.workspace.step(model::workspace::Event::RemapAgentChatSession(from.clone(), to.clone(), None));
                return None;
            }
            Effect::RevealChatTab { session_id } => {
                self.workspace.step(model::workspace::Event::AddAgentChatTab(session_id.clone(), None));
                return None;
            }
            Effect::Toast { level, message } => {
                let kind = match level.as_str() {
                    "error" => ui::overlay::ToastKind::Error,
                    "success" => ui::overlay::ToastKind::Success,
                    _ => ui::overlay::ToastKind::Warning,
                };
                self.toasts.push(self.now, kind, message, None);
                return None;
            }
            Effect::ActionResolved { value, .. } => {
                log::info!("[chat] action resolved: {value}");
                return None;
            }
            _ => return Some(effect),
        };
        log::info!("[chat] {kind:?}");
        self.command(kind, payload, Pending::Agent(Box::new(effect)));
        None
    }

    /// The host's answer to an agent, session or switch command.
    pub(crate) fn agent_command_settled(&mut self, effect: Effect, result: Result<Option<Value>, WsError>) {
        match result {
            Ok(reply) => {
                let reply = reply.unwrap_or(Value::Null);
                self.follow_created_session(&effect, &reply);
                self.acp_step(Event::AgentCommandDone(effect, reply));
            }
            Err(e) => {
                log::warn!("[chat] an agent command was refused: {}: {}", e.code, e.message);
                self.acp_step(Event::AgentCommandFailed(effect, e.message, Some(e.code)));
            }
        }
    }

    /// A durable session the model created or promoted is followed live, as the web transport
    /// subscribes after `create_session`; its tab then shows the model's transcript, not a
    /// fresh read of its history.
    fn follow_created_session(&mut self, effect: &Effect, reply: &Value) {
        let session_id = match effect {
            Effect::NewSession { ephemeral: false, .. } => reply.get("sessionId").and_then(Value::as_str).map(str::to_owned),
            Effect::PromoteSession { session_id, .. } => Some(session_id.clone()),
            _ => None,
        };
        if let Some(session_id) = session_id {
            self.follow_session(&session_id);
        }
    }
}
