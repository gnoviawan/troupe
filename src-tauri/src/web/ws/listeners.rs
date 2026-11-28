//! Remote access `/ws` requests (`web::listeners_api`) for the host-local
//! desktop client: `listeners_status`, `listeners_set_lan {enabled, port?}`,
//! `listeners_set_tunnel {enabled}` and `listeners_rotate_token`. Each
//! answers the listeners' status, whose URLs carry the remote token, so
//! nothing here logs a reply.

use super::*;
use crate::web::listeners_api::ListenersStatus;

/// Every remote access request type.
pub const LISTENERS_WS_REQUESTS: &[&str] = &[
    "listeners_status",
    "listeners_set_lan",
    "listeners_set_tunnel",
    "listeners_rotate_token",
];

#[derive(Deserialize)]
struct Toggle {
    enabled: bool,
    #[serde(default)]
    port: u16,
}

fn status_reply(id: String, status: Result<ListenersStatus, String>) -> WsReply {
    match status.and_then(|status| serde_json::to_value(status).map_err(|e| e.to_string())) {
        Ok(value) => WsReply::ok(id, Some(value)),
        Err(message) => WsReply::err_with_code(id, "LISTENERS_FAILED", message),
    }
}

/// Run one remote access request.
pub(super) async fn handle_listeners_request(
    id: String,
    request_type: &str,
    payload: &Value,
    env: &HandlerEnv,
) -> WsReply {
    let Some(listeners) = env.relay.listeners() else {
        return WsReply::err(
            id,
            WsErrorCode::Unsupported,
            "this host has no remote access controls",
        );
    };
    let toggle = || {
        serde_json::from_value::<Toggle>(payload.clone())
            .map_err(|_| format!("malformed {request_type} payload (want enabled, port?)"))
    };
    let status = match request_type {
        "listeners_status" => Ok(listeners.status().await),
        "listeners_set_lan" => match toggle() {
            Ok(Toggle {
                enabled: true,
                port,
            }) => listeners.start_lan(port).await,
            Ok(Toggle { enabled: false, .. }) => Ok(listeners.stop_lan().await),
            Err(message) => Err(message),
        },
        "listeners_set_tunnel" => match toggle() {
            Ok(Toggle { enabled: true, .. }) => listeners.start_tunnel().await,
            Ok(Toggle { enabled: false, .. }) => Ok(listeners.stop_tunnel().await),
            Err(message) => Err(message),
        },
        "listeners_rotate_token" => listeners.rotate_token().await,
        other => Err(format!("unknown remote access request `{other}`")),
    };
    if let Err(message) = &status {
        warn!(
            target: "termul::web::ws",
            request = request_type,
            "remote access request failed: {message}"
        );
    }
    status_reply(id, status)
}
