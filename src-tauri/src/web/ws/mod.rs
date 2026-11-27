//! WS relay protocol — frame envelopes, seq, event log, cursor, tiers (Story 1.4+1.6).
//!
//! One multiplexed bidirectional WebSocket per browser connection carries all
//! sessions (AC1). The wire types live in `crates/troupe-protocol`, which the
//! TypeScript contract is generated from (AC2). The request, reply and event
//! envelopes here stay server types: their `type` and `err` are open strings.
//!
//! # Wire casing (AC3 — deviation from architecture text, MUST follow)
//!
//! The **envelope** fields (`sid`, `seq`, `type`, `payload`) are snake_case.
//! The **payload** is the existing camelCase-serialized ACP event struct
//! `Value` (`fan_out` serializes ONCE, fans out N). This module does NOT
//! re-case payloads.
//!
//! # OS vs human cap boundary (AC8)
//!
//! The server is the ACP client-of-record (thin relay, not pure ACP-over-WS).
//! OS caps ([`OS_FULFILLED_CAPS`]) are fulfilled by the server; only human caps
//! ([`HUMAN_RELAYED_CAPS`]) are relayed to the browser. A browser WS request
//! for an OS cap is rejected with `err.code: "unsupported"`.
//!
//! # Scope fence
//!
//! `authenticate` first validates `protocolVersion` ([`WS_PROTOCOL_VERSION`])
//! and the `deviceId` / `clientInstanceId` identity (see
//! [`validate_authenticate_handshake`]) — on gated AND ungated hosts — and
//! refuses `protocol_version_mismatch` / `invalid_client_identity`, staying
//! pre-auth. It then validates the single-token web auth gate (CAP-1 interim,
//! QA remediation Story 1) when `AppState.web_auth` is `Some` — a wrong/absent
//! token is refused `unauthorized` and the connection stays pre-auth (retry
//! allowed). Ungated servers (`web_auth: None`) accept any token after the
//! handshake checks pass. `subscribe` is wired (Story 1.6):
//! binds the connection to a session log with optional `lastSeq` cursor
//! replay. Unknown request types return `err.code: "not_implemented"`.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};
use troupe_protocol::ws::WsEventType;

use crate::acp::config::AgentConfig;
use crate::acp::provider_errors::ProviderErrorKind;
use crate::acp::{AcpManager, SessionCreationContext};
// Only the harness and the ws tests name these (through `super::*`).
#[cfg(test)]
use crate::acp::{AgentId, SessionId};
use crate::pty::PtyManager;
use crate::trackers::{CwdTracker, ExitCodeTracker, GitTracker, TerminalEventHub};
use crate::web::auth::WebAuth;
use crate::web::capabilities::{CapabilityPolicy, Carrier};
use crate::web::connection_runtime::ConnectionRuntime;
use crate::web::project_registry::ProjectRegistry;
use crate::web::sink::{ClientId, EventStream, ReplayResult, StreamItem, WsRelaySink};
use crate::web::store::WebStore;

mod access;
mod agents;
mod antigravity_accounts;
mod canvas;
pub mod channel;
mod chat_reads;
mod claude_accounts;
mod codex_accounts;
mod commands;
mod dispatch;
mod import_session;
mod lifecycle;
mod mcp_catalog;
mod projects;
mod prompts;
mod relay;
mod sessions;
mod store;
mod subagents;
mod subscribe;
mod watch;

// Handler submodules expose their items as `pub(super)`; these private
// glob re-exports keep bare-name lookups working for sibling submodules and
// for `tests.rs`' `use super::*` without widening visibility past `ws`.
use access::*;
use agents::*;
use antigravity_accounts::*;
use canvas::*;
use chat_reads::*;
use claude_accounts::*;
use codex_accounts::*;
use commands::*;
use dispatch::*;
use import_session::*;
use lifecycle::*;
use projects::*;
use prompts::*;
use relay::*;
use sessions::*;
use store::*;
use subagents::*;
use subscribe::*;
use watch::*;

// The wire-contract `pub` surface stays reachable at `web::ws::*`.
#[cfg(test)]
pub(crate) use relay::{outbound_channel, OutboundReceiver, OUTBOUND_QUEUE_CAPACITY};
pub use relay::{ws_upgrade, AUTH_REQUIRED_TYPE, WS_BATCH_TYPE, WS_GAP_TYPE, WS_HEARTBEAT_TYPE};
pub(crate) use relay::{EnqueueOutcome, Outbound, OutboundSender};

// ---------------------------------------------------------------------------
// Sequenced event — the wire envelope (AC2 + AC3)
// ---------------------------------------------------------------------------

/// A sequenced event ready for fan-out + cursor replay.
///
/// Serializes to the WS event envelope `{sid, seq, type, payload}` (snake_case
/// envelope; `payload` is the camelCase ACP event struct `Value` passed through
/// verbatim). `seq` is `0` for agent-level (`sid: None`) + relay-level events.
#[derive(Debug, Clone, Serialize)]
pub struct SequencedEvent {
    /// Session id, or `None` for agent-level / relay-level events.
    pub sid: Option<String>,
    /// Per-session monotonic sequence (starts at 1). `0` for agent-level.
    pub seq: u64,
    /// Event `type` (prefix-dropped snake_case, e.g. `message_chunk`).
    #[serde(rename = "type")]
    pub type_: String,
    /// The camelCase ACP event struct value (passed through verbatim).
    pub payload: Value,
}

impl SequencedEvent {
    /// Build a sequenced event from a prefix-dropped type + payload.
    #[must_use]
    pub fn new(sid: Option<String>, seq: u64, type_: impl Into<String>, payload: Value) -> Self {
        Self {
            sid,
            seq,
            type_: type_.into(),
            payload,
        }
    }
}

// ---------------------------------------------------------------------------
// Reliability tiers (AC5) — the crate's tier of each event type
// ---------------------------------------------------------------------------

pub use troupe_protocol::ws::ReliabilityTier;

/// Map a prefix-dropped event `type` to its [`ReliabilityTier`]
/// ([`WsEventType::tier`]). An unknown type is [`ReliabilityTier::Reliable`]:
/// the relay never drops an event it does not recognize.
#[must_use]
pub fn tier_of(type_: &str) -> ReliabilityTier {
    static TIER_BY_TYPE: std::sync::LazyLock<
        std::collections::HashMap<&'static str, ReliabilityTier>,
    > = std::sync::LazyLock::new(|| {
        WsEventType::ALL
            .iter()
            .map(|known| (known.as_str(), known.tier()))
            .collect()
    });
    TIER_BY_TYPE
        .get(type_)
        .copied()
        .unwrap_or(ReliabilityTier::Reliable)
}

// ---------------------------------------------------------------------------
// Request / reply / error envelope structs (AC2 + AC10)
// ---------------------------------------------------------------------------

pub use troupe_protocol::version::{MAX_CLIENT_ID_LEN, WS_PROTOCOL_VERSION};

/// Client identity presented on `authenticate` (AD-4). Neither id is a
/// credential, and the host stores both exactly as received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    /// Per install / browser profile; partitions view state and presence.
    pub device_id: String,
    /// Per tab or window; stamps writes.
    pub client_instance_id: String,
}

impl ClientIdentity {
    /// Log-safe label for one id: the first 8 characters. The ids are stable
    /// per profile, so the full values stay out of service logs.
    #[must_use]
    pub fn log_label(id: &str) -> String {
        id.chars().take(8).collect()
    }
}

/// Why an `authenticate` handshake was refused before the token gate ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeRefusal {
    /// Stable `/ws` error code ([`WsErrorCode::ProtocolVersionMismatch`] or
    /// [`WsErrorCode::InvalidClientIdentity`]); `/terminal/ws` maps it to its
    /// SCREAMING wire string via [`HandshakeRefusal::terminal_code`].
    pub code: WsErrorCode,
    /// The payload field that failed (`protocolVersion`, `deviceId`,
    /// `clientInstanceId`) — safe to log.
    pub check: &'static str,
    /// Human-readable message for the refusal reply.
    pub message: String,
}

impl HandshakeRefusal {
    /// The `/terminal/ws` SCREAMING_SNAKE_CASE wire code for this refusal.
    #[must_use]
    pub const fn terminal_code(&self) -> &'static str {
        match self.code {
            WsErrorCode::ProtocolVersionMismatch => "PROTOCOL_VERSION_MISMATCH",
            _ => "INVALID_CLIENT_IDENTITY",
        }
    }
}

/// Validate the `authenticate` payload's `protocolVersion`, `deviceId` and
/// `clientInstanceId` (shared by `/ws` and `/terminal/ws`). Runs BEFORE any
/// token check so a stale tab learns it must reload even on a token mismatch.
/// A missing, non-numeric or different `protocolVersion` is a mismatch; each
/// id must be a non-empty string of at most [`MAX_CLIENT_ID_LEN`] bytes with
/// no control characters (the host logs ids, so a newline must not forge a
/// log line). The ids are returned exactly as received (the host never mints
/// or rewrites).
pub fn validate_authenticate_handshake(
    payload: &Value,
) -> Result<ClientIdentity, HandshakeRefusal> {
    if payload["protocolVersion"].as_u64() != Some(u64::from(WS_PROTOCOL_VERSION)) {
        return Err(HandshakeRefusal {
            code: WsErrorCode::ProtocolVersionMismatch,
            check: "protocolVersion",
            message: format!(
                "protocol version mismatch: this host speaks version {WS_PROTOCOL_VERSION}; reload the client"
            ),
        });
    }
    let client_id = |field: &'static str| -> Result<String, HandshakeRefusal> {
        match payload[field].as_str() {
            Some(value)
                if !value.is_empty()
                    && value.len() <= MAX_CLIENT_ID_LEN
                    && !value.chars().any(char::is_control) =>
            {
                Ok(value.to_string())
            }
            _ => Err(HandshakeRefusal {
                code: WsErrorCode::InvalidClientIdentity,
                check: field,
                message: format!(
                    "{field} must be a non-empty string of at most {MAX_CLIENT_ID_LEN} bytes without control characters"
                ),
            }),
        }
    };
    Ok(ClientIdentity {
        device_id: client_id("deviceId")?,
        client_instance_id: client_id("clientInstanceId")?,
    })
}

/// A WS request frame `{id, type, payload, command_id?}` sent client→server.
#[derive(Debug, Clone, Deserialize)]
pub struct WsRequest {
    /// Client-chosen correlation id (echoed in the reply).
    pub id: String,
    /// Request `type` (prefix-dropped snake_case).
    #[serde(rename = "type")]
    pub type_: String,
    /// Request payload (shape depends on `type`).
    #[serde(default = "Value::default")]
    pub payload: Value,
    /// Client-minted command id for a mutating request (1-128 characters of
    /// `[A-Za-z0-9_-]`). Ledgered commands are recorded under it before they
    /// run; `send_prompt`'s `turnId` is its command id. A request without one
    /// keeps the pre-receipt behavior.
    #[serde(default)]
    pub command_id: Option<String>,
}

/// A WS reply frame `{id, ok, payload?, err?}` sent server→client.
#[derive(Debug, Clone, Serialize)]
pub struct WsReply {
    /// Echoes the request `id`.
    pub id: String,
    /// `true` for success, `false` for failure.
    pub ok: bool,
    /// Success payload (omitted on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// Failure detail (omitted on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<WsError>,
    /// Receipt visibility marker: the session log seq the effect is visible
    /// at, set on ok replies of session-scoped commands (other replies omit
    /// it until watch revisions exist).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_at: Option<Value>,
}

impl WsReply {
    /// Build a success reply.
    #[must_use]
    pub fn ok(id: impl Into<String>, payload: Option<Value>) -> Self {
        Self {
            id: id.into(),
            ok: true,
            payload,
            err: None,
            visible_at: None,
        }
    }

    /// Build a failure reply with a stable code + human message.
    #[must_use]
    pub fn err(id: impl Into<String>, code: WsErrorCode, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ok: false,
            payload: None,
            visible_at: None,
            err: Some(WsError {
                code: code.as_str().to_string(),
                message: message.into(),
                details: None,
            }),
        }
    }

    /// Build a failure reply whose `err.details` carries structured context
    /// (the `project_busy` entity list).
    #[must_use]
    pub fn err_with_details(
        id: impl Into<String>,
        code: WsErrorCode,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        let mut reply = Self::err(id, code, message);
        if let Some(err) = reply.err.as_mut() {
            err.details = Some(details);
        }
        reply
    }

    /// Build a failure reply with a raw (SCREAMING_SNAKE_CASE) code string.
    ///
    /// CAP-6 / Story 9: the install handler carries transport-identical codes
    /// (`INTEGRITY_MISMATCH`, `INTEGRITY_METADATA_MISSING`, …) matching the
    /// Tauri `IpcResult.code` + HTTP `IpcBody.code` byte-for-byte. The
    /// protocol-level `WsErrorCode` enum is snake_case (e.g. `unsupported`),
    /// so the install codes cannot be expressed as enum variants without
    /// breaking the wire contract. This constructor accepts a raw string so
    /// the install handler's `err.code` is byte-identical across transports.
    #[must_use]
    pub fn err_with_code(
        id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            payload: None,
            visible_at: None,
            err: Some(WsError {
                code: code.into(),
                message: message.into(),
                details: None,
            }),
        }
    }
}

/// The `err` object inside a failing [`WsReply`].
#[derive(Debug, Clone, Serialize)]
pub struct WsError {
    /// Stable machine string (one of [`WsErrorCode`]).
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Optional structured context for the code (additive): a `project_busy`
    /// refusal lists the bound entities as
    /// `{"sessions":[sessionId…],"terminals":[terminalId…]}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub use troupe_protocol::ws::WsErrorCode;

// ---------------------------------------------------------------------------
// OS vs human cap boundary (AC8)
// ---------------------------------------------------------------------------

/// ACP caps the SERVER fulfills locally (the browser cannot perform them).
/// `terminal/*` is a prefix — every cap under `terminal/` is OS-fulfilled.
pub const OS_FULFILLED_CAPS: &[&str] = &["fs/read_text_file", "fs/write_text_file", "terminal/*"];

/// ACP caps RELAYED to the browser (human-in-the-loop).
pub const HUMAN_RELAYED_CAPS: &[&str] = &["session_notification", "request_permission"];

/// Whether `cap` matches an OS-fulfilled cap entry (exact, or prefix match for
/// entries ending in `/*`). Enforced at the request-handling layer (AC8).
#[must_use]
pub fn is_os_fulfilled_cap(cap: &str) -> bool {
    OS_FULFILLED_CAPS.iter().copied().any(|entry| {
        entry == cap || entry.ends_with("/*") && cap.starts_with(&entry[..entry.len() - 1])
    })
}

/// Whether `cap` matches a human-relayed cap entry (exact match).
#[must_use]
pub fn is_human_relayed_cap(cap: &str) -> bool {
    HUMAN_RELAYED_CAPS.iter().copied().any(|entry| entry == cap)
}

/// Map an `AcpManager` prompt error to a stable WS `err.code` (Story 1.7 T7.1).
///
/// `AcpManager::send_prompt` (via `DriverState::try_begin_turn`) rejects a
/// concurrent prompt on the same session with the string
/// `"ACP_TURN_IN_PROGRESS: session {id}"`. The renderer keys on that stable
/// code (`ACP_TURN_IN_PROGRESS_CODE`). For the WS path (Story 1.8's
/// `send_prompt`), this maps it to [`WsErrorCode::RateLimited`] (the closest
/// stable `err.code` for "try again shortly" — the architecture's set has no
/// dedicated turn-busy code). Returns `None` for any other error string.
#[must_use]
pub fn map_prompt_error_code(err: &str) -> Option<WsErrorCode> {
    if err.starts_with("ACP_TURN_IN_PROGRESS") {
        Some(WsErrorCode::RateLimited)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Router state (AC1 + AC7)
// ---------------------------------------------------------------------------

/// Shared Axum state for the standalone server: the ACP manager + the live
/// WS relay sink + the host's durable project registry. Typed
/// struct (preferred over tuple state past 2 fields).
#[derive(Clone)]
pub struct AppState {
    /// The ACP manager (server is the ACP client-of-record).
    pub acp: Arc<AcpManager>,
    /// Interactive PTYs exposed on the separate `/terminal/ws` endpoint.
    pub pty: Arc<PtyManager>,
    pub terminal_events: TerminalEventHub,
    pub cwd_tracker: Arc<CwdTracker>,
    pub git_tracker: Arc<GitTracker>,
    pub exit_code_tracker: Arc<ExitCodeTracker>,
    /// The live WS relay sink (owns per-session logs + seq counters + subs).
    pub relay: Arc<WsRelaySink>,
    /// The host's durable project registry — source for `GET /projects`,
    /// default-project resolution and the fs/git boundary.
    pub registry: Arc<ProjectRegistry>,
    /// Deployment history provider exposed to authenticated browser clients.
    pub history_mode: HistoryMode,
    /// Host-owned ACP catalog service (CAP-6 / Story 8). `None` when the
    /// desktop could not open `AcpCatalogService` at startup (degraded mode —
    /// routes return `ACP_CATALOG_UNAVAILABLE`). The web/remote client reads
    /// the resolved catalog through `GET /acp/catalog` + WS
    /// `list_acp_catalog`; the desktop renderer sends the same WS requests over
    /// its channel carrier.
    pub acp_catalog: Option<Arc<crate::acp::AcpCatalogService>>,
    /// Host-owned verified-atomic ACP install service (CAP-6 / Story 9). `None`
    /// when the desktop could not open `AcpInstallService` at startup (degraded
    /// mode — the `install_acp_agent` handler returns
    /// `ACP_INSTALL_UNAVAILABLE`). The web/remote client installs a catalog
    /// agent through `POST /acp/install` (catalog_api sibling) + WS
    /// `install_acp_agent`; the desktop renderer sends the same WS request over
    /// its channel carrier.
    pub acp_install: Option<Arc<crate::acp::install::AcpInstallService>>,
    /// Issue #613: server-side generic key-value store for web-client state
    /// (terminal layout, settings, editor state, command history, snapshots,
    /// SSH profiles). `None` when a server does not attach a store — the
    /// `store_*` WS handlers return `STORE_UNAVAILABLE` (degraded mode). The
    /// standalone binary + desktop shared-live host both attach one.
    pub store: Option<Arc<WebStore>>,
    /// Operator opt-in: admit non-loopback peers on the loopback-guarded
    /// write routes (fs/git/workspace/projects/host-state). Default `false`
    /// (the CWE-306 guard stays on); set `true` only by the standalone
    /// `termul-server` `--allow-remote-writes` flag. The desktop shared-live
    /// host leaves this `false` (LAN clients stay view-only for mutations).
    pub allow_remote_writes: bool,
    /// Deployment-mode deny: when `true`, the write guard refuses ALL write
    /// routes BEFORE evaluating the peer address or `allow_remote_writes`.
    /// Set `true` by the desktop shared-live host — it binds localhost and a
    /// cloudflared quick-tunnel forwards public traffic to it from a loopback
    /// source, so `is_loopback()` cannot distinguish cloudflared-forwarded
    /// requests from genuine local callers. The standalone `termul-server`
    /// sets this `false` (its `--allow-remote-writes` opt-in is the
    /// admission path there). Closes the cloudflared loopback bypass.
    pub shared_live_writes_denied: bool,
    /// Pending MCP OAuth flows keyed by their CSRF `state`, each holding the
    /// session (PKCE verifier, client id) built at start. The web callback and
    /// paste-back routes take a flow out to finish the token exchange. The
    /// desktop path doesn't use this (it runs the full flow in the
    /// `acp_mcp_oauth_start` command).
    pub pending_oauth_flows: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, crate::acp::mcp_oauth::PendingOAuthFlow>,
        >,
    >,
    /// Base URL for OAuth redirect URIs on the web path:
    /// `http://127.0.0.1:{port}` on the standalone server (authorization
    /// servers accept plain http only on loopback). The OAuth callback route
    /// lives at `{base}/oauth/callback`.
    pub oauth_base_url: String,
    /// The web auth gate (CAP-1 interim, QA remediation Story 1). `Some` on
    /// a gated server (explicit token configured, or public bind with a
    /// generated/loaded token — see `web::auth::resolve`): the pre-auth
    /// `authenticate` branch validates `payload.token` in constant time, and
    /// `/terminal/ws` + the gated HTTP API routes enforce the same token.
    /// `None` keeps the legacy ungated behavior (accept-any-token
    /// `authenticate`, no `/terminal/ws` connection gate).
    pub web_auth: Option<Arc<WebAuth>>,
    /// Standalone `termul-server` host-administration context (gh-1105): the
    /// resolved state dir + the effective setup answers the `/server/config`
    /// reconfigure routes read, persist, and restart onto. `Some` only on the
    /// standalone server; `None` on the desktop shared-live host (and the
    /// desktop's in-process channel state) so `/server/config` stays
    /// unreachable there and `/health` reports `setupRequired: false`.
    pub server_admin: Option<crate::web::server_config_file::ServerAdminContext>,
    /// Host shutdown coordination + per-connection overflow tolerance, shared
    /// by every upgraded `/ws` and `/terminal/ws` socket (AD-19).
    pub connections: ConnectionRuntime,
}

pub use troupe_protocol::ws::{AcpRuntimePolicy as RuntimePolicy, HistoryMode};

/// The turn and keepalive budgets this host hands out at `authenticate`.
/// `turn_timeout_ms` is the hard turn ceiling the client's inactivity refresh
/// must not extend past; `prompt_inactivity_timeout_ms` is the window that
/// matching-session activity refreshes. `0` means no limit for both.
#[must_use]
pub fn resolved_runtime_policy() -> RuntimePolicy {
    let turn_timeout = crate::acp::manager::resolved_turn_timeout();
    let turn_idle = crate::acp::manager::turn_idle_timeout();
    let turn_timeout_ms = turn_timeout.map(|d| d.as_millis() as u64).unwrap_or(0);
    // Inactivity budget published to the client. Preserve the original
    // `hard/2` derivation when a hard cap is configured (bounded, strictly
    // shorter than the ceiling); otherwise use the idle timeout when it is
    // configured (bounded); otherwise 0 (unlimited — no client-side
    // inactivity timer). 0 keeps the client's `setTimeout` well under the
    // browser 32-bit ceiling and defers entirely to the server.
    let prompt_inactivity_timeout_ms = match (turn_timeout, turn_idle) {
        (Some(hard), _) => (hard.as_millis() as u64 / 2).max(1),
        (None, Some(idle)) => idle.as_millis() as u64,
        (None, None) => 0,
    };
    RuntimePolicy {
        turn_timeout_ms,
        prompt_inactivity_timeout_ms,
        ping_interval_ms: PING_INTERVAL.as_millis() as u64,
        pong_timeout_ms: PONG_TIMEOUT.as_millis() as u64,
    }
}

/// Cheaply extract the `type` field of a client WS text frame (CAP-3 control
/// signals). Returns `None` for non-JSON or frames without a string `type`.
/// The read task uses this to recognize id-less `background`/`foreground`
/// lifecycle frames before the strict `WsRequest` parse (which requires `id`).
pub(super) fn peer_frame_type(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    value.get("type")?.as_str().map(str::to_owned)
}

/// Monotonic millis elapsed since `epoch`, read from the tokio clock so a
/// paused test runtime (`start_paused`) advances the watchdog
/// deterministically. Each relay connection captures its own `epoch` and uses
/// it for every watchdog timestamp, so the value never depends on process-wide
/// state or on which runtime first called it (several paused test runtimes
/// each own an independent clock). Unlike `SystemTime` it cannot jump
/// backwards, and `saturating_sub` keeps the compare safe regardless.
pub(super) fn elapsed_ms(epoch: tokio::time::Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

/// Map an `AcpManager` `Err(String)` to a `WsReply` err. Story 1.8 review:
/// map recognizable agent-manager error strings to their stable `err.code`
/// (so the browser's error routing keys on the right category — not every
/// runtime failure is "not_implemented"). `send_prompt`'s concurrent-turn
/// rejection (`"ACP_TURN_IN_PROGRESS: …"`) → `RateLimited`; agent-side ACP
/// `AuthRequired` (-32000) failures — tagged `"ACP_AUTH_REQUIRED: …"` at the
/// manager boundary, or the bare default `"Authentication required"` message
/// (exact match) for pre-collapsed paths — → `AgentAuthRequired` (Story 7);
/// `"unknown agent: …"` / `"unknown permission request: …"` and agent-side
/// `ResourceNotFound` (-32002, tagged `"ACP_RESOURCE_NOT_FOUND: …"`) → `NotFound`;
/// capability-gate failures (`"agent does not support …"`) → `Unsupported`;
/// `"ACP_PROVIDER_*: …"` provider failures → `provider_quota_exceeded` /
/// `provider_session_limit` / `provider_request_rejected`.
/// Unrecognized errors fall back to `NotImplemented` (preserves the human
/// message verbatim).
pub(super) fn acp_err_to_reply(id: String, err: String) -> WsReply {
    if let Some(code) = map_prompt_error_code(&err) {
        return WsReply::err(id, code, err);
    }
    // AD-5 deterministic spawn rejections: the stable code carries the verdict,
    // so the message drops the manager's transport prefix.
    if let Some((failure, message)) = crate::acp::manager::classify_spawn_error(&err) {
        let code = match failure {
            crate::acp::manager::SpawnFailure::AgentUnavailable => WsErrorCode::AgentUnavailable,
            crate::acp::manager::SpawnFailure::InvalidAgentConfig => {
                WsErrorCode::InvalidAgentConfig
            }
        };
        return WsReply::err(id, code, message.to_string());
    }
    // Upstream provider refusals: the stable code carries the verdict, so the
    // message drops the manager's transport prefix.
    if let Some((kind, message)) = crate::acp::provider_errors::split_provider_tag(&err) {
        let code = match kind {
            ProviderErrorKind::QuotaExceeded => WsErrorCode::ProviderQuotaExceeded,
            ProviderErrorKind::SessionLimit => WsErrorCode::ProviderSessionLimit,
            ProviderErrorKind::RequestRejected => WsErrorCode::ProviderRequestRejected,
        };
        return WsReply::err(id, code, message.to_string());
    }
    if let Some(message) = crate::acp::claude_accounts::strip_signed_out_tag(&err) {
        return WsReply::err(id, WsErrorCode::ClaudeAccountSignedOut, message.to_string());
    }
    if let Some(message) = crate::acp::codex_accounts::strip_signed_out_tag(&err) {
        return WsReply::err(id, WsErrorCode::CodexAccountSignedOut, message.to_string());
    }
    if let Some(message) = crate::acp::antigravity_accounts::strip_signed_out_tag(&err) {
        return WsReply::err(
            id,
            WsErrorCode::AntigravityAccountSignedOut,
            message.to_string(),
        );
    }
    // Issue #837 split-brain guard: the manager tags a rejected reopen with
    // `ACP_SESSION_OWNED_BY_OTHER` when the session belongs to a different
    // live agent mid-turn.
    if err.starts_with(crate::acp::manager::ACP_SESSION_OWNED_BY_OTHER) {
        return WsReply::err(id, WsErrorCode::SessionOwnedByOther, err);
    }
    if let Some(message) = err
        .strip_prefix(crate::acp::manager::ACP_RESOURCE_NOT_FOUND_PREFIX)
        .and_then(|rest| rest.strip_prefix(": "))
    {
        return WsReply::err(id, WsErrorCode::NotFound, message.to_string());
    }
    let code = if err
        .strip_prefix(crate::acp::manager::ACP_AUTH_REQUIRED_PREFIX)
        .is_some_and(|rest| rest.starts_with(": "))
        || err == "Authentication required"
    {
        WsErrorCode::AgentAuthRequired
    } else if err.starts_with("unknown agent") || err.contains("unknown permission request") {
        WsErrorCode::NotFound
    } else if err.contains("agent does not support") || err.contains("capability") {
        WsErrorCode::Unsupported
    } else {
        WsErrorCode::NotImplemented
    };
    WsReply::err(id, code, err)
}

/// Serialize a `Serialize` success value into a `WsReply::ok` payload, or reply
/// `err` on serialization failure (never `null` — mirrors `fan_out` semantics).
pub(super) fn ok_with_payload<T: serde::Serialize>(id: String, value: &T) -> WsReply {
    match serde_json::to_value(value) {
        Ok(v) => WsReply::ok(id, Some(v)),
        Err(e) => WsReply::err(
            id,
            WsErrorCode::Unsupported,
            format!("failed to serialize reply payload: {e}"),
        ),
    }
}

#[cfg(test)]
mod agent_overlay_tests;
#[cfg(test)]
mod authenticate_agent_tests;
#[cfg(test)]
mod canvas_tests;
#[cfg(test)]
mod capability_tests;
#[cfg(test)]
pub(crate) mod harness;
#[cfg(test)]
mod host_local_tests;
#[cfg(test)]
mod host_ticket_route_tests;
#[cfg(test)]
mod mcp_install_route_tests;
#[cfg(test)]
mod list_sessions_tests;
#[cfg(test)]
mod pending_permission_tests;
#[cfg(test)]
mod project_binding_tests;
#[cfg(test)]
mod project_command_tests;
#[cfg(test)]
pub(crate) mod protocol_fixture_tests;
#[cfg(test)]
mod provider_error_tests;
#[cfg(test)]
mod registry_check_tests;
#[cfg(test)]
mod spawn_rejection_tests;
#[cfg(test)]
mod tests;
