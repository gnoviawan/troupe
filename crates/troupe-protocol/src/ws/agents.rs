//! The `list_agents` reply element and the background command lists it carries.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::payloads::ExtraFields;
use crate::literal::present;

/// Agent capabilities as advertised at `initialize`. Unknown keys pass through.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_session: Option<bool>,
    #[schemars(extend("x-nullable" = true))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub session_capabilities: Option<Option<SessionCapabilities>>,
    #[schemars(extend("x-nullable" = true))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub auth: Option<Option<AuthCapabilities>>,
    #[schemars(extend("x-nullable" = true))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub mcp_capabilities: Option<Option<McpCapabilities>>,
    #[schemars(extend("x-nullable" = true))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub prompt_capabilities: Option<Option<PromptCapabilities>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// Which optional session methods the agent serves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionCapabilities {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub resume: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub close: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub list: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub delete: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub additional_directories: Option<Value>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// Whether the agent can sign out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuthCapabilities {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub logout: Option<Value>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// MCP transports the agent accepts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct McpCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// Prompt content kinds the agent accepts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded_context: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// One element of the `list_agents` reply. Older hosts omit the session lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsAgentSummary {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub capabilities: AgentCapabilities,
    /// Sessions whose workspace roots are registered on this agent's driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owns_session: Option<Vec<String>>,
    /// Owned sessions the agent works in outside a prompt turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_sessions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_commands: Option<AgentBackgroundCommands>,
}

/// The commands an agent left running, by owned session, as of `revision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentBackgroundCommands {
    pub revision: u64,
    pub sessions: BTreeMap<String, Vec<BackgroundCommand>>,
}

/// A command the agent left running after its tool call or turn ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCommand {
    pub tool_call_id: String,
    /// The command's description, else its tool call title.
    pub title: String,
    pub command: Option<String>,
    /// Wall clock when the tool call first appeared.
    pub started_at_ms: u64,
    #[serde(flatten)]
    pub state: BackgroundCommandState,
}

/// Whether a background command still runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum BackgroundCommandState {
    Running,
    #[serde(rename_all = "camelCase")]
    Ended {
        ended_at_ms: u64,
    },
}

/// One session's background commands as of `revision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BackgroundCommandList {
    pub revision: u64,
    pub commands: Vec<BackgroundCommand>,
}
