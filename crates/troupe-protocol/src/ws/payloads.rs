//! Typed `/ws` request payloads and reply payloads. Requests not listed here
//! carry a payload the contract leaves open. The host parses the request
//! payloads marked `deny_unknown_fields` with these types and refuses a
//! payload that carries any other field.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::frames::WsEvent;
use super::names::HistoryMode;
use crate::literal::present;
use crate::{False, True};

/// Open JSON object fields a type does not name.
pub type ExtraFields = BTreeMap<String, Value>;

/// `authenticate` request payload. `token` is absent only when the host has
/// no web auth token. Neither id is a credential.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcpAuthenticatePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub protocol_version: u32,
    /// Per install or browser profile.
    pub device_id: String,
    /// Per tab or window.
    pub client_instance_id: String,
}

/// `authenticate` reply payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcpAuthenticateReply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_mode: Option<HistoryMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_policy: Option<AcpRuntimePolicy>,
    /// The protocol version the host speaks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<u32>,
}

/// Turn and keepalive budgets the host hands out at `authenticate`, in ms.
/// `0` means no limit for the two turn budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcpRuntimePolicy {
    pub turn_timeout_ms: u64,
    pub prompt_inactivity_timeout_ms: u64,
    pub ping_interval_ms: u64,
    pub pong_timeout_ms: u64,
}

/// `delete_session` request payload. The reply is `true` when the record was
/// deleted and `false` when it was already absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteSessionPayload {
    pub session_id: String,
}

/// `record_agent_switch` request payload: the host appends one durable
/// `agent_switch` record built from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordAgentSwitchPayload {
    pub session_id: String,
    pub from_config_id: String,
    pub to_config_id: String,
    /// The session the conversation continues in.
    pub new_session_id: String,
    pub summary_text: String,
}

/// `mark_chat_read` request payload and reply: a chat's shared last-read
/// marker. `logEpoch` is absent only from a peer that predates it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatReadMark {
    pub session_id: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_epoch: Option<String>,
}

/// `subscribe` reply. The `pending*` lists hold the original request payloads
/// still waiting for an answer, in queue order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSubscribeReply {
    pub session_id: String,
    pub replayed: u64,
    pub log_epoch: String,
    pub head_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_permissions: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_questions: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_elicitations: Option<Vec<Value>>,
    /// MCP install approvals still waiting. The hand-written type lacks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_mcp_installs: Option<Vec<Value>>,
}

/// Atomic stale-recovery payload: the next cursor is `(logEpoch, watermark)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshotEvent {
    pub session_id: String,
    pub watermark: u64,
    pub head_seq: u64,
    pub log_epoch: String,
    pub events: Vec<WsEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_permissions: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_questions: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_elicitations: Option<Vec<Value>>,
    /// MCP install approvals still waiting. The hand-written type lacks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_mcp_installs: Option<Vec<Value>>,
}

/// `get_session_cursor` reply. An unknown session has no epoch and no head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionCursorReply {
    pub session_id: String,
    pub watermark: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_seq: Option<u64>,
}

wire_enum! {
    /// Lifecycle state of a persisted session.
    pub enum PersistedSessionStatus {
        Active = "active",
        Closed = "closed",
        Error = "error",
    }
}

wire_enum! {
    /// Who named a chat; a local alias wins over every other source.
    pub enum TitleSource {
        BackgroundGenerated = "background_generated",
        AgentSupplied = "agent_supplied",
        DerivedFirstMessage = "derived_first_message",
        LocalAlias = "local_alias",
    }
}

literal!(
    /// `kind` of a chat that has no project.
    QuickKind = str "quick"
);

/// One entry of `list_persisted_sessions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSessionSummary {
    pub storage_key: String,
    pub session_id: String,
    pub stable_agent_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub cwd: String,
    pub title: Option<String>,
    /// Who named the chat. The hand-written type lacks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_source: Option<TitleSource>,
    pub created_at: u64,
    pub last_activity_at: u64,
    pub status: PersistedSessionStatus,
    pub message_count: u64,
    pub tool_count: u64,
    pub last_seq: u64,
    /// True while a prompt turn is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovered: Option<bool>,
    pub resume_eligible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// The session log's epoch. The hand-written type lacks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_epoch: Option<String>,
    /// The session this one continues after an in-chat agent switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
    /// Host-minted folder id of a quick chat. The hand-written type lacks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_chat_id: Option<String>,
    /// The chat whose agent started this one with `start_chat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<QuickKind>,
}

/// The payload of a `user_prompt` event: the user input the host recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptEvent {
    pub agent_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub content: Vec<Value>,
}

/// `store_read` request payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StoreReadPayload {
    pub key: String,
}

/// `store_read` reply. Revision `0` means the key is absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreReadReply {
    pub value: Value,
    pub revision: u64,
    pub client_instance_id: Option<String>,
}

/// `store_write` request payload. `expected: null` (key must be absent)
/// differs from no `expected`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoreWritePayload {
    pub key: String,
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub expected: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
}

/// `store_write` reply for a device key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoreDeviceWriteReply {
    pub revision: u64,
}

/// `store_write` reply for a shared key: the stored, possibly merged value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StoreSharedWriteReply {
    pub value: Value,
    pub revision: u64,
    pub merged: bool,
}

/// `store_delete` request payload; shared keys follow the write preconditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoreDeletePayload {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub expected: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
}

/// `store_delete` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoreDeleteReply {
    pub existed: bool,
}

/// `add_project` request payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddProjectPayload {
    pub id: String,
    pub name: String,
    pub path: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
}

/// `update_project` request payload. `cascade: true` stops the project's live
/// sessions and terminals instead of refusing `project_busy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProjectPayload {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade: Option<bool>,
}

/// `remove_project` request payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveProjectPayload {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade: Option<bool>,
}

/// `err.details` of a `project_busy` refusal: the live sessions and terminals
/// bound to the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectBusyDetails {
    pub sessions: Vec<String>,
    pub terminals: Vec<String>,
}

/// `canvas_mount` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WsCanvasMountReply {
    Attached(WsCanvasAttached),
    Refused(WsCanvasRefused),
}

/// The connection now runs the mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WsCanvasAttached {
    pub attached: True,
}

/// The host has no such mount for this renderer. `heldElsewhere`: another
/// window holds the project's lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsCanvasRefused {
    pub attached: False,
    pub held_elsewhere: bool,
}

/// `canvas_reply` request payload: the result's JSON text, or a typed error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WsCanvasReplyPayload {
    Ok(WsCanvasResult),
    Err(WsCanvasFailure),
}

/// A result, or one slice of it when `partial` is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsCanvasResult {
    pub call_id: u64,
    pub mount_seq: u64,
    pub ok: True,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<True>,
}

/// A failed canvas call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsCanvasFailure {
    pub call_id: u64,
    pub mount_seq: u64,
    pub ok: False,
    pub error: WsCanvasError,
}

/// The error of a failed canvas call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WsCanvasError {
    pub code: String,
    pub message: String,
}
