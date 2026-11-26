//! The `/terminal/ws` contract: terminal requests and their replies, plus the
//! output, replay, gap and lifecycle frames of attached terminals. Fields are
//! camelCase except inside lifecycle event payloads, which are snake_case.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ws::ExtraFields;
use crate::{False, True};

wire_enum! {
    /// A `/terminal/ws` request `type`.
    pub enum WebTerminalRequestType {
        Authenticate = "authenticate",
        Spawn = "spawn",
        Write = "write",
        CommandEditorSubmit = "command_editor_submit",
        CommandEditorRequestCompletion = "command_editor_request_completion",
        CommandEditorCancelCompletion = "command_editor_cancel_completion",
        CommandEditorRestorePrompt = "command_editor_restore_prompt",
        Resize = "resize",
        Kill = "kill",
        Attach = "attach",
        Detach = "detach",
        RotateClaim = "rotate_claim",
        RevokeClaim = "revoke_claim",
        GetCwd = "get_cwd",
        GetGitBranch = "get_git_branch",
        GetGitStatus = "get_git_status",
        GetExitCode = "get_exit_code",
        ListPreserved = "list_preserved",
    }
}

wire_enum! {
    /// SCREAMING_SNAKE_CASE codes of `/terminal/ws` refusals. Handlers also
    /// answer domain codes outside this set (`VALIDATION_ERROR`), so a reply's
    /// `code` stays a string.
    pub enum WebTerminalErrorCode {
        Unauthorized = "UNAUTHORIZED",
        ProtocolVersionMismatch = "PROTOCOL_VERSION_MISMATCH",
        InvalidClientIdentity = "INVALID_CLIENT_IDENTITY",
        Forbidden = "FORBIDDEN",
        OutcomeUnknown = "OUTCOME_UNKNOWN",
        CommandConflict = "COMMAND_CONFLICT",
        InvalidCommandId = "INVALID_COMMAND_ID",
        TerminalLimit = "TERMINAL_LIMIT",
        ProjectBusy = "PROJECT_BUSY",
    }
}

/// A request frame, client to host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebTerminalRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: WebTerminalRequestType,
    /// The request payload; `spawn` sends a [`TerminalSpawnOptions`]. The
    /// host accepts a request without one and reads it as `null`.
    #[serde(default)]
    pub payload: Value,
    /// Client-minted id of a ledgered request (`spawn`, `write`, `kill`,
    /// `rotate_claim`, `revoke_claim`). A resend never runs twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
}

/// A reply frame: `data` on success, a message and a code on failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WebTerminalReply {
    Ok(WebTerminalOkReply),
    Err(WebTerminalErrReply),
}

/// A successful reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WebTerminalOkReply {
    pub id: String,
    pub success: True,
    pub data: Value,
}

/// A failed reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WebTerminalErrReply {
    pub id: String,
    pub success: False,
    pub error: String,
    pub code: String,
}

literal!(
    /// `type` of [`WebTerminalDataFrame`].
    DataTag = str "data"
);
literal!(
    /// `type` of [`WebTerminalReplayFrame`].
    ReplayTag = str "replay"
);
literal!(
    /// `type` of [`WebTerminalGapFrame`].
    TerminalGapTag = str "gap"
);
literal!(
    /// `type` of [`WebTerminalEventFrame`].
    EventTag = str "event"
);

/// One sequenced output chunk of an attached terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebTerminalDataFrame {
    #[serde(rename = "type")]
    pub kind: DataTag,
    pub terminal_id: String,
    pub seq: u64,
    /// Raw output bytes.
    pub data: Vec<u8>,
}

/// One chunk of a replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WebTerminalReplayChunk {
    pub seq: u64,
    pub data: Vec<u8>,
}

/// The chunks a client has not seen, sent on attach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebTerminalReplayFrame {
    #[serde(rename = "type")]
    pub kind: ReplayTag,
    pub terminal_id: String,
    pub chunks: Vec<WebTerminalReplayChunk>,
    pub gap: bool,
    /// The cursor was from another epoch or ahead of the head: discard what
    /// was rendered first.
    pub reset: bool,
    pub log_epoch: String,
    pub latest_seq: u64,
    pub snapshot: TerminalStateSnapshot,
}

/// The output stream lagged and lost chunks after `lastSeq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebTerminalGapFrame {
    #[serde(rename = "type")]
    pub kind: TerminalGapTag,
    pub terminal_id: String,
    pub last_seq: u64,
}

/// A terminal lifecycle or metadata change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WebTerminalEventFrame {
    #[serde(rename = "type")]
    pub kind: EventTag,
    pub payload: WebTerminalEventPayload,
}

/// The payload of a [`WebTerminalEventFrame`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebTerminalEventPayload {
    SessionLifecycle {
        terminal_id: String,
        session_id: String,
        kind: String,
        nonce: Option<String>,
        prompt_suppressed: bool,
        line_empty: bool,
        exit_code: Option<i32>,
        prompt_generation: Option<u64>,
        cwd: Option<String>,
    },
    Exit {
        terminal_id: String,
        exit_code: Option<i32>,
        signal: Option<i32>,
    },
    CwdChanged {
        terminal_id: String,
        cwd: String,
    },
    GitBranchChanged {
        terminal_id: String,
        branch: Option<String>,
    },
    GitStatusChanged {
        terminal_id: String,
        status: Option<GitStatus>,
    },
    ExitCodeChanged {
        terminal_id: String,
        exit_code: i32,
    },
}

/// Any frame the host writes on `/terminal/ws`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WebTerminalFrame {
    Reply(WebTerminalReply),
    Data(WebTerminalDataFrame),
    Replay(WebTerminalReplayFrame),
    Gap(WebTerminalGapFrame),
    Event(WebTerminalEventFrame),
}

/// A terminal's last known state, sent on attach so a late client can seed
/// its store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalStateSnapshot {
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub git_status: Option<GitStatus>,
    pub exit_code: Option<i32>,
    pub exited: bool,
}

/// Change counts of a terminal's git working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub modified: u32,
    pub staged: u32,
    pub untracked: u32,
    pub ahead: u32,
    pub behind: u32,
    pub has_changes: bool,
}

wire_enum! {
    /// What a terminal runs.
    pub enum TerminalKind {
        Shell = "shell",
        Agent = "agent",
    }
}

/// `spawn` request payload. With `program` set the PTY runs that executable
/// with `args` instead of a login shell. Unknown keys pass through.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSpawnOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<TerminalKind>,
    /// Required and non-empty on `/terminal/ws`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `spawn` reply data: the terminal and the claim credential it issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SpawnedTerminal {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    /// 64 hex characters; only spawn and rotate replies carry it.
    pub claim: String,
}

/// `attach` reply data: the replay cursor and the terminal's last known
/// state. It never carries a claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachResult {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    pub latest_seq: u64,
    pub gap: bool,
    pub reset: bool,
    pub log_epoch: String,
    pub snapshot: TerminalStateSnapshot,
}

/// `rotate_claim` reply data: the fresh credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RotatedClaim {
    pub claim: String,
}
