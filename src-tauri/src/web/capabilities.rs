//! Connection capabilities + the single remote-write predicate.
//!
//! Every web carrier (`/ws`, `/terminal/ws`, the HTTP write routes) decides
//! "may this peer mutate host state?" with [`write_decision`]. WebSocket
//! connections evaluate it ONCE at `authenticate` (carrier, deployment flags
//! and peer address) and keep the result as [`Capabilities`]; HTTP routes
//! evaluate it per request through `fs_api::check_local_only`.
//!
//! A connection without [`Capabilities::write`] is *view-only*: it may read,
//! ping, subscribe and write `device/<its deviceId>/…` store keys, and every
//! other mutation is refused with the stable `forbidden` code.
//!
//! [`Capabilities::host_local`] is the narrower grant for requests that act
//! with the host's own credentials: a `/ws` connection from a loopback peer,
//! on a token-gated host that does not deny writes, whose upgrade names no
//! proxy ([`PROXY_HEADERS`]), and that presented the host-local credential
//! (`web::host_local`) at `authenticate`. `--allow-remote-writes` never
//! grants it.

use std::net::SocketAddr;

use axum::http::HeaderMap;

use crate::web::ws::{AppState, ClientIdentity};

/// Which WebSocket carrier a connection arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    /// The multiplexed ACP relay (`/ws`).
    Ws,
    /// The desktop's in-process Tauri `ipc::Channel` (`ws_channel_*`): the
    /// same relay as `/ws`, reached without a network socket.
    Ipc,
    /// The dedicated interactive terminal socket (`/terminal/ws`).
    TerminalWs,
}

impl Carrier {
    /// Stable label for logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ws => "ws",
            Self::TerminalWs => "terminal_ws",
            Self::Ipc => "ipc",
        }
    }
}

/// The deployment flags the write predicate reads (copied from [`AppState`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteFlags {
    /// Operator opt-in (`--allow-remote-writes`): admit non-loopback peers.
    pub allow_remote_writes: bool,
    /// Deployment-mode deny (desktop shared-live): refuse every write before
    /// the peer address is even looked at.
    pub shared_live_writes_denied: bool,
}

impl WriteFlags {
    /// Read the flags off the router state.
    #[must_use]
    pub fn from_state(state: &AppState) -> Self {
        Self {
            allow_remote_writes: state.allow_remote_writes,
            shared_live_writes_denied: state.shared_live_writes_denied,
        }
    }
}

/// Outcome of the shared write predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDecision {
    /// Loopback peer on a deployment that does not deny writes.
    Allowed,
    /// Non-loopback peer admitted by the `--allow-remote-writes` opt-in.
    AdmittedByOptIn,
    /// Shared-live deployment: all writes denied regardless of peer.
    DeniedSharedLive,
    /// Non-loopback peer without the opt-in.
    DeniedRemotePeer,
}

impl WriteDecision {
    /// Whether the peer may mutate host state.
    #[must_use]
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed | Self::AdmittedByOptIn)
    }

    /// Human-readable refusal reason (`None` when allowed).
    #[must_use]
    pub fn refusal_message(self, peer: SocketAddr) -> Option<String> {
        match self {
            Self::Allowed | Self::AdmittedByOptIn => None,
            Self::DeniedSharedLive => {
                Some("shared-live deployment mode denies all remote writes".to_string())
            }
            Self::DeniedRemotePeer => Some(format!(
                "write routes are localhost-only (peer {peer} is not loopback)"
            )),
        }
    }
}

/// THE remote-write predicate. Deployment-mode deny comes first: the desktop
/// shared-live host binds localhost and a cloudflared quick-tunnel forwards
/// public traffic to it from a loopback source, so `is_loopback()` cannot
/// tell a tunnelled browser from a genuine local caller.
#[must_use]
pub fn write_decision(peer: SocketAddr, flags: WriteFlags) -> WriteDecision {
    if flags.shared_live_writes_denied {
        return WriteDecision::DeniedSharedLive;
    }
    if peer.ip().is_loopback() {
        WriteDecision::Allowed
    } else if flags.allow_remote_writes {
        WriteDecision::AdmittedByOptIn
    } else {
        WriteDecision::DeniedRemotePeer
    }
}

/// What a connection may do, computed once at `authenticate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// The carrier the connection arrived on.
    pub carrier: Carrier,
    /// May mutate host state (otherwise view-only).
    pub write: bool,
    /// Runs on the host as the server's user and may act with the host's own
    /// credentials (see the module doc).
    pub host_local: bool,
}

impl Capabilities {
    /// Nothing granted — the pre-authenticate state.
    #[must_use]
    pub const fn none(carrier: Carrier) -> Self {
        Self {
            carrier,
            write: false,
            host_local: false,
        }
    }

    /// Resolve a connection's capabilities from its carrier, deployment flags
    /// and peer address. Logs the decision at the connection boundary (peer +
    /// carrier only — never a token).
    #[must_use]
    pub fn resolve(carrier: Carrier, peer: SocketAddr, flags: WriteFlags) -> Self {
        let decision = write_decision(peer, flags);
        match decision {
            WriteDecision::Allowed => {}
            WriteDecision::AdmittedByOptIn => tracing::warn!(
                target: "termul::web::capabilities",
                carrier = carrier.as_str(),
                peer = %peer,
                "connection granted write capability (--allow-remote-writes)",
            ),
            WriteDecision::DeniedSharedLive | WriteDecision::DeniedRemotePeer => tracing::info!(
                target: "termul::web::capabilities",
                carrier = carrier.as_str(),
                peer = %peer,
                reason = ?decision,
                "connection is view-only (write capability denied)",
            ),
        }
        Self {
            carrier,
            write: decision.is_allowed(),
            host_local: false,
        }
    }
}

/// Headers a reverse proxy or tunnel adds to a forwarded request. Their
/// presence on a loopback upgrade means the client is somewhere else.
/// Defense in depth behind the host-local credential: nginx adds none of
/// them by default.
pub const PROXY_HEADERS: [&str; 11] = [
    "forwarded",
    "via",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
    "x-client-ip",
    "true-client-ip",
    "cf-connecting-ip",
    "cf-ray",
    "tailscale-user-login",
];

/// Whether `headers` carry any of [`PROXY_HEADERS`].
#[must_use]
pub fn names_a_proxy(headers: &HeaderMap) -> bool {
    PROXY_HEADERS.iter().any(|name| headers.contains_key(*name))
}

/// Everything needed to resolve [`Capabilities`] at `authenticate`, captured
/// when the socket upgrades.
#[derive(Debug, Clone, Copy)]
pub struct CapabilityPolicy {
    pub carrier: Carrier,
    pub peer: SocketAddr,
    pub flags: WriteFlags,
    /// The host requires the bearer token at `authenticate`.
    pub token_gated: bool,
    /// The upgrade carried a [`PROXY_HEADERS`] header.
    pub proxied: bool,
}

impl CapabilityPolicy {
    /// Policy for a freshly upgraded socket.
    #[must_use]
    pub fn for_connection(carrier: Carrier, peer: SocketAddr, state: &AppState) -> Self {
        Self {
            carrier,
            peer,
            flags: WriteFlags::from_state(state),
            token_gated: state.web_auth.is_some(),
            proxied: false,
        }
    }

    /// The same policy for an upgrade that did (`true`) or did not carry a
    /// proxy header.
    #[must_use]
    pub const fn through_proxy(mut self, proxied: bool) -> Self {
        self.proxied = proxied;
        self
    }

    /// Resolve the connection's capabilities (called once, at authenticate).
    /// `host_credential`: the client presented the host-local credential.
    #[must_use]
    pub fn resolve(&self, host_credential: bool) -> Capabilities {
        let mut capabilities = Capabilities::resolve(self.carrier, self.peer, self.flags);
        if !host_credential {
            return capabilities;
        }
        let refusal = if self.carrier != Carrier::Ws {
            Some("not the /ws carrier")
        } else if !self.token_gated {
            Some("the host is not token-gated")
        } else if self.proxied {
            Some("the upgrade carried a proxy header")
        } else if write_decision(self.peer, self.flags) != WriteDecision::Allowed {
            Some("the peer is not a loopback writer")
        } else {
            None
        };
        capabilities.host_local = refusal.is_none();
        match refusal {
            None => tracing::info!(
                target: "termul::web::capabilities",
                carrier = self.carrier.as_str(),
                peer = %self.peer,
                "connection is host-local",
            ),
            Some(reason) => tracing::warn!(
                target: "termul::web::capabilities",
                carrier = self.carrier.as_str(),
                peer = %self.peer,
                reason,
                "host-local credential presented but refused",
            ),
        }
        capabilities
    }
}

/// The `store_*` key prefix a view-only connection may still write:
/// `device/<deviceId>/`. `None` when the id contains `/` — such an id could
/// alias another device's namespace (`device/a/b/…` belongs to device `a/b`
/// AND would be reachable by device `a`), so it gets no private namespace.
#[must_use]
pub fn device_store_prefix(device_id: &str) -> Option<String> {
    if device_id.contains('/') {
        return None;
    }
    Some(format!("device/{device_id}/"))
}

/// What a `/ws` request type needs from the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// Reads, ping, subscribe and a chat read (`mark_chat_read` moves only
    /// the shared unread marker a viewer clears by reading): any
    /// authenticated connection.
    View,
    /// `store_write` / `store_delete`: view-only connections only for keys
    /// under their own `device/<deviceId>/` prefix.
    DeviceStore,
    /// Every other mutation: needs [`Capabilities::write`].
    Write,
    /// Acts with the host's own credentials or changes who can reach the
    /// host (the SSH requests, the `ssh` watch, the remote access
    /// controls): needs [`Capabilities::host_local`].
    HostLocal,
}

/// Classify a `/ws` request type. Allow-list of the view-level types; every
/// other type (including unknown ones) needs write, so a new mutating request
/// is guarded by default.
#[must_use]
pub fn ws_requirement(request_type: &str) -> Requirement {
    match request_type {
        "authenticate"
        | "ping"
        | "subscribe"
        // The watches (projects, chat history, agents, file changes, store
        // keys, terminals) only route frames to this connection, like
        // `subscribe`.
        | "watch"
        | "unwatch"
        | "list_persisted_sessions"
        | "open_persisted_session"
        | "get_session_payload"
        | "get_session_payload_tail"
        | "recover_session_snapshot"
        | "get_session_cursor"
        | "list_sessions"
        | "list_agents"
        | "list_acp_catalog"
        | "fetch_acp_registry_snapshot"
        | "get_acp_agent_overlay"
        | "search_mcp_catalog"
        | "describe_mcp_install"
        | "get_mcp_catalog_status"
        | "get_mcp_catalog_icon"
        | "list_claude_accounts"
        | "list_claude_usage"
        | "list_codex_accounts"
        | "list_codex_usage"
        | "list_antigravity_accounts"
        | "list_antigravity_usage"
        | "subagent_settings_get"
        | "store_read"
        | "mark_chat_read" => Requirement::View,
        "store_write" | "store_delete" => Requirement::DeviceStore,
        t if crate::web::ws::SSH_WS_REQUESTS.contains(&t)
            || crate::web::ws::LISTENERS_WS_REQUESTS.contains(&t) =>
        {
            Requirement::HostLocal
        }
        _ => Requirement::Write,
    }
}

/// [`ws_requirement`] refined by the payload: a `watch` of the `ssh` watch
/// needs [`Requirement::HostLocal`], because SSH events (host-key questions,
/// remote paths) go only to the connections that may run the sessions.
#[must_use]
pub fn ws_request_requirement(request_type: &str, payload: &serde_json::Value) -> Requirement {
    if request_type == "watch" && payload["watch"].as_str() == Some("ssh") {
        return Requirement::HostLocal;
    }
    ws_requirement(request_type)
}

/// Whether `caps` may run a `/ws` request of `request_type` whose payload is
/// `payload`. `identity` supplies the `deviceId` for the device-store rule.
#[must_use]
pub fn ws_request_permitted(
    request_type: &str,
    payload: &serde_json::Value,
    caps: Capabilities,
    identity: Option<&ClientIdentity>,
) -> bool {
    match ws_request_requirement(request_type, payload) {
        Requirement::View => true,
        Requirement::Write => caps.write,
        Requirement::HostLocal => caps.host_local,
        Requirement::DeviceStore => {
            caps.write
                || payload["key"]
                    .as_str()
                    .zip(identity.and_then(|id| device_store_prefix(&id.device_id)))
                    .is_some_and(|(key, prefix)| {
                        key.len() > prefix.len() && key.starts_with(&prefix)
                    })
        }
    }
}

/// `/terminal/ws` request types a view-only connection may still send.
/// Everything else mutates the host: spawn/write/resize/kill, claim rotation
/// and revocation — and
/// `list_preserved`, which re-issues (replaces) the claim of every terminal
/// without a live attachment, so a view-only peer could lock out its owner.
/// A view-only connection therefore cannot obtain a claim to `attach` (claims
/// come only from `spawn`, `list_preserved` and claim rotation): it sees
/// terminal events and reads for terminals it already holds, nothing more.
#[must_use]
pub fn terminal_request_is_view(request_type: &str) -> bool {
    matches!(
        request_type,
        "authenticate"
            | "attach"
            | "detach"
            | "get_cwd"
            | "get_git_branch"
            | "get_git_status"
            | "get_exit_code"
    )
}

#[cfg(test)]
mod tests;
