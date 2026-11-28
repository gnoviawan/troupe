//! The `ssh` watch and the host-key answer: what a `host_local` client learns
//! about its SSH sessions while they connect and run.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

wire_enum! {
    /// Why a host key needs a person. A trusted key is never asked about and
    /// a revoked one is refused without asking.
    pub enum SshHostKeyStatus {
        // No known_hosts entry for this host and key type.
        Unknown = "unknown",
        // An entry exists for this host and key type with a different key.
        Changed = "changed",
    }
}

/// A host-key question waiting for an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyPrompt {
    pub session_id: String,
    pub status: SshHostKeyStatus,
    pub host: String,
    pub port: u16,
    /// OpenSSH key type name, e.g. `ssh-ed25519`.
    pub key_type: String,
    /// `SHA256:<base64>` of the key the server presented.
    pub fingerprint: String,
    /// `SHA256:<base64>` of each key known_hosts records for this host and key
    /// type. Empty unless `status` is `changed`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_fingerprints: Vec<String>,
}

/// `ssh` watch state: the host-key questions open now. A client that missed
/// a `host_key` delta finds the question here when it watches again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshWatchState {
    pub host_keys: Vec<SshHostKeyPrompt>,
}

/// A connect moved to its next stage (`connecting`, `verifyingHostKey`,
/// `authenticating`, `startingShell`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshSessionProgress {
    pub session_id: String,
    pub stage: String,
}

/// A host-key question was answered, cancelled or timed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyResolved {
    pub session_id: String,
}

/// A session's shell ended: `exited` (with its code), `lost` (with a
/// detail) or `closed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshSessionEnded {
    pub session_id: String,
    pub terminal_id: Option<String>,
    pub reason: String,
    pub exit_code: Option<i32>,
    pub detail: Option<String>,
}

/// Progress of one SFTP upload or download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshTransferProgress {
    /// The session the transfer runs on.
    pub connection_id: String,
    pub remote_path: String,
    pub local_path: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    /// `upload` or `download`.
    pub direction: String,
    /// `in-progress`, `completed`, `failed` or `cancelled`.
    pub status: String,
    pub error: Option<String>,
}

/// An `ssh` delta: one SSH fact, tagged by `event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SshWatchDelta {
    Progress(SshSessionProgress),
    HostKey(SshHostKeyPrompt),
    HostKeyResolved(SshHostKeyResolved),
    Ended(SshSessionEnded),
    Transfer(SshTransferProgress),
}

/// `ssh_session_respond_host_key` payload. Trusting over `/ws` names the
/// fingerprint the person saw; the host refuses a trust whose fingerprint is
/// not the one of the question open for that session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyAnswer {
    pub session_id: String,
    pub trust: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}
