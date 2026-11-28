//! Watch frames: the full state of a watch, then one delta per change, each
//! numbered by `(hostEpoch, n)`.

use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::names::WsWatchName;
use crate::literal::present;
use crate::True;

literal!(
    /// `type` of a watch frame. The `watch` request shares the name; a reply
    /// has an `id`, a watch frame does not.
    WatchTag = str "watch"
);

/// Which serve start produced a frame, and its position in the subscription's
/// stream: `n` is `1` for the full state and then rises by one per frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsWatchRevision {
    pub host_epoch: String,
    pub n: u64,
}

/// A watch frame: exactly one of the full state and a delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WsWatchFrame {
    State(WsWatchStateFrame),
    Delta(WsWatchDeltaFrame),
}

/// The full state of a watch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = forbid_delta)]
pub struct WsWatchStateFrame {
    #[serde(rename = "type")]
    pub kind: WatchTag,
    pub watch: WsWatchName,
    pub scope: String,
    pub revision: WsWatchRevision,
    pub reset: True,
    pub state: Value,
}

/// One change after the state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = forbid_state)]
pub struct WsWatchDeltaFrame {
    #[serde(rename = "type")]
    pub kind: WatchTag,
    pub watch: WsWatchName,
    pub scope: String,
    pub revision: WsWatchRevision,
    pub delta: Value,
}

fn forbid_delta(schema: &mut Schema) {
    forbid(schema, &["delta"]);
}

fn forbid_state(schema: &mut Schema) {
    forbid(schema, &["reset", "state"]);
}

/// The host writes exactly one of `state` and `delta`. Each branch of
/// [`WsWatchFrame`] gives the other branch's fields the `false` schema, so a
/// frame with both payloads matches neither branch.
fn forbid(schema: &mut Schema, names: &[&str]) {
    let properties = schema
        .ensure_object()
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(properties) = properties {
        for name in names {
            properties.insert((*name).to_owned(), Value::Bool(false));
        }
    }
}

/// `watch` reply payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WsWatchReply {
    pub watch: WsWatchName,
    pub scope: String,
    pub revision: WsWatchRevision,
}

/// The delta the host sends when it could not queue the one before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WatchResyncDelta {
    pub resync: True,
}

literal!(
    /// `event` of a `chat_history` delta for a worktree branch rename.
    WorktreeBranchRenamedTag = str "worktree_branch_renamed"
);

/// A `chat_history` delta: the sessions whose index entry changed, or an
/// empty list when the index changed in bulk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChatHistoryWatchDelta {
    pub changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<WorktreeBranchRenamedTag>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub payload: Option<Value>,
}

wire_enum! {
    /// The host fact an `agents` delta reports.
    pub enum AgentsWatchEvent {
        AgentSpawned = "agent_spawned",
        AgentCrashed = "agent_crashed",
        AgentError = "agent_error",
        AgentDisconnected = "agent_disconnected",
        BrowserOpenRequest = "browser_open_request",
        AgentSignInRequest = "agent_sign_in_request",
        AgentSignInResolved = "agent_sign_in_resolved",
        BrowserPolicyChanged = "browser_policy_changed",
        ComuPolicyChanged = "comu_policy_changed",
        AutomationChanged = "automation_changed",
        AutomationResync = "automation_resync",
    }
}

/// An `agents` delta: an agent-level fact with its event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsWatchDelta {
    pub event: AgentsWatchEvent,
    pub payload: Value,
}

/// The newest turn end the host saw for a chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurnEnd {
    pub seq: u64,
    pub log_epoch: String,
}

/// What every chat list shows for one agent chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatActivityRecord {
    /// A prompt turn is open or the agent works outside one.
    pub busy: bool,
    /// A permission, question or elicitation waits for an answer.
    pub needs_you: bool,
    /// The newest turn end is past the shared last-read marker.
    pub unread: bool,
    pub turn_end: Option<ChatTurnEnd>,
    /// The project the session was created in; `null` when the host has no
    /// persisted record of it.
    #[schemars(extend("x-nullable" = true))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub project_id: Option<Option<String>>,
}

/// `chat_activity` watch state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChatActivityWatchState {
    pub sessions: BTreeMap<String, ChatActivityRecord>,
}

/// A `chat_activity` delta: one session's record as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatActivityWatchDelta {
    pub session_id: String,
    #[serde(flatten)]
    pub record: ChatActivityRecord,
}

/// `fs` watch state: the canonical root being watched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FsWatchState {
    pub root: String,
}

/// An `fs` delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FsWatchDelta {
    Change(FsWatchChangeDelta),
    Ended(FsWatchEndedDelta),
}

/// One debounced batch of changed paths under the project root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FsWatchChangeDelta {
    pub root: String,
    pub paths: Vec<String>,
}

/// The last `fs` delta: the project is no longer watchable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FsWatchEndedDelta {
    pub ended: True,
    pub reason: String,
}

/// `store` watch state and delta: whether the key holds a value, and which.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StoreWatchState {
    pub exists: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub value: Option<Value>,
}

/// One terminal of the `terminals` watch. It never carries a claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsTerminalSetEntry {
    pub terminal_id: String,
    pub project_id: String,
    pub cwd: String,
    /// A web connection holds a live attachment.
    pub attached: bool,
}

/// `terminals` watch state and delta: the project's terminals as they are now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TerminalsWatchState {
    pub terminals: Vec<WsTerminalSetEntry>,
}
