//! `/ws` frame envelopes: the request a client sends, the reply it gets, and
//! the frames the host pushes without a request.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::names::{WsEventType, WsRequestType};
use super::payloads::ProjectBusyDetails;
use super::watch::WsWatchFrame;
use crate::literal::present;
use crate::{False, True};

/// A request frame, client to host. `id` is echoed in the reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: WsRequestType,
    /// The request payload; its shape depends on `type`. The host accepts a
    /// request without one and reads it as `null`.
    #[serde(default)]
    pub payload: Value,
    /// Client-minted id of a mutating request (1-128 characters of
    /// `[A-Za-z0-9_-]`). A resend with the same id never runs twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
}

/// A reply frame, host to client: success with an optional payload, or a
/// failure with a machine code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WsReply {
    Ok(WsOkReply),
    Err(WsErrReply),
}

/// A successful reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsOkReply {
    pub id: String,
    pub ok: True,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub payload: Option<Value>,
    /// The session log seq the effect is visible at, on session commands.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub visible_at: Option<Value>,
}

/// A failed reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsErrReply {
    pub id: String,
    pub ok: False,
    pub err: WsError,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub visible_at: Option<Value>,
}

/// The `err` object of a failed reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsError {
    /// A `WsErrorCode` string, or a SCREAMING_SNAKE_CASE domain code from the
    /// store and install handlers.
    pub code: String,
    pub message: String,
    /// Set on `project_busy`: the live entities bound to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ProjectBusyDetails>,
}

/// A session log event, host to client. `sid` is `null` and `seq` is `0` for
/// agent-level and relay-level events; `payload` is the camelCase ACP event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsEvent {
    pub sid: Option<String>,
    pub seq: u64,
    #[serde(rename = "type")]
    pub kind: WsEventType,
    pub payload: Value,
}

literal!(
    /// `type` of [`WsEventBatchFrame`].
    EventsTag = str "events"
);
literal!(
    /// `type` of [`WsGapFrame`].
    GapTag = str "gap"
);
literal!(
    /// `type` of [`WsHeartbeatFrame`].
    HeartbeatTag = str "heartbeat"
);
literal!(
    /// `type` of [`WsCanvasCallFrame`].
    CanvasCallTag = str "canvas_call"
);
literal!(
    /// `type` of [`WsCanvasLeaseLostFrame`].
    CanvasLeaseLostTag = str "canvas_lease_lost"
);

/// Several events queued together, written as one frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsEventBatchFrame {
    #[serde(rename = "type")]
    pub kind: EventsTag,
    pub events: Vec<WsEvent>,
}

/// Lossy events of `sid` were dropped for this connection; the client installs
/// a fresh snapshot of the session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsGapFrame {
    #[serde(rename = "type")]
    pub kind: GapTag,
    pub sid: String,
}

/// The host is alive; written every 10 s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WsHeartbeatFrame {
    #[serde(rename = "type")]
    pub kind: HeartbeatTag,
}

/// One canvas control call for the connection that holds `projectId`'s canvas
/// at `mountSeq`, answered with a `canvas_reply` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsCanvasCallFrame {
    #[serde(rename = "type")]
    pub kind: CanvasCallTag,
    pub call_id: u64,
    pub project_id: String,
    pub mount_seq: u64,
    pub method: String,
    /// JSON text, never parsed on the way.
    pub params: String,
    /// How long the host still waits for the reply, in ms. Absent on a notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

wire_enum! {
    /// Why a renderer lost its canvas mount.
    pub enum CanvasLeaseLostReason {
        TakenOver = "takenOver",
        Evicted = "evicted",
    }
}

/// The mount at `mountSeq` was saved and unmounted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsCanvasLeaseLostFrame {
    #[serde(rename = "type")]
    pub kind: CanvasLeaseLostTag,
    pub project_id: String,
    pub mount_seq: u64,
    pub reason: CanvasLeaseLostReason,
}

/// Any frame the host writes on `/ws`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WsServerFrame {
    Reply(WsReply),
    Batch(WsEventBatchFrame),
    Gap(WsGapFrame),
    Heartbeat(WsHeartbeatFrame),
    Watch(WsWatchFrame),
    CanvasCall(WsCanvasCallFrame),
    CanvasLeaseLost(WsCanvasLeaseLostFrame),
    Event(WsEvent),
}
