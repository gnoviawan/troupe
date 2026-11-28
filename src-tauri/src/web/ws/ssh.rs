//! SSH `/ws` requests: profiles, keys, interactive sessions and SFTP for the
//! host-local client of `troupe-server --desktop`, through `ssh::service`.
//! Each request type is the Tauri command's name and its payload is that
//! command's arguments (`{ request }`, `{ profileId }`, `{ sessionId, trust }`,
//! ...). A success answers the command's value; a failure answers the
//! service's stable code (`SSH_AUTH_FAILED`, `SFTP_ERROR`, ...) with its
//! message. Port forwarding is not served: no client calls it yet. Session
//! progress, host-key questions, session ends and transfer progress go to
//! the `ssh` watch (`web::desktop_mode::attach_ssh`).
//!
//! Trusting a host key names the fingerprint the client showed: the host
//! trusts only the key of the question open for that session, so a client
//! can never trust a key it was not shown, nor queue a trust ahead of a
//! question.
//!
//! The capability check admits these only for a `host_local` connection:
//! they use the host's SSH keys, keychain and known_hosts. Payloads can carry
//! a password or passphrase, so nothing here logs them.

use super::*;
use crate::ssh::service::{self, SshFailure, SshResult};
use crate::ssh::SSHManager;
use serde::de::DeserializeOwned;

/// Every SSH request type.
pub const SSH_WS_REQUESTS: &[&str] = &[
    "ssh_list_profiles",
    "ssh_save_profile",
    "ssh_delete_profile",
    "ssh_duplicate_profile",
    "ssh_import_config",
    "ssh_inspect_key",
    "ssh_list_keys",
    "ssh_session_open",
    "ssh_session_respond_host_key",
    "ssh_session_cancel",
    "ssh_session_close",
    "ssh_session_list",
    "sftp_list_dir",
    "sftp_realpath",
    "sftp_download",
    "sftp_upload",
    "sftp_delete",
    "sftp_mkdir",
    "sftp_rename",
    "sftp_read_file",
    "sftp_write_file",
    "sftp_create_file",
];

#[derive(Deserialize)]
struct RequestArg<T> {
    request: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileIdArg {
    profile_id: String,
}

#[derive(Deserialize)]
struct ProfileArg {
    profile: crate::ssh::profile_manager::SSHProfile,
}

#[derive(Deserialize)]
struct PathArg {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionIdArg {
    session_id: String,
}

/// Parse the payload. The error names a missing field, never a value.
fn args<T: DeserializeOwned>(request_type: &str, payload: &Value) -> SshResult<T> {
    serde_json::from_value(payload.clone()).map_err(|error| {
        SshFailure::new(
            format!(
                "malformed {request_type} payload: {}",
                payload_error(&error)
            ),
            "SSH_INVALID_REQUEST",
        )
    })
}

/// A payload parse error that never repeats a value the client sent: serde
/// quotes the offending value in a type or value error, and that field may be
/// a password or passphrase.
pub(super) fn payload_error(error: &serde_json::Error) -> String {
    let text = error.to_string();
    if text.starts_with("missing field") {
        text
    } else {
        "a field has the wrong type or value".to_string()
    }
}

fn value<T: serde::Serialize>(result: SshResult<T>) -> SshResult<Value> {
    result.and_then(|v| {
        serde_json::to_value(v).map_err(|e| SshFailure::new(e.to_string(), "SSH_INTERNAL"))
    })
}

async fn run(
    ssh: &SSHManager,
    pty: &PtyManager,
    request_type: &str,
    payload: &Value,
) -> SshResult<Value> {
    match request_type {
        "ssh_list_profiles" => value(service::list_profiles(ssh)),
        "ssh_save_profile" => {
            let a: ProfileArg = args(request_type, payload)?;
            value(service::save_profile(ssh, a.profile))
        }
        "ssh_delete_profile" => {
            let a: ProfileIdArg = args(request_type, payload)?;
            value(service::delete_profile(ssh, &a.profile_id))
        }
        "ssh_duplicate_profile" => {
            let a: ProfileIdArg = args(request_type, payload)?;
            value(service::duplicate_profile(ssh, &a.profile_id))
        }
        "ssh_import_config" => value(service::import_config(ssh)),
        "ssh_inspect_key" => {
            let a: PathArg = args(request_type, payload)?;
            value(Ok(service::inspect_key(&a.path)))
        }
        "ssh_list_keys" => value(Ok(service::list_keys())),
        "ssh_session_open" => {
            let a: RequestArg<service::SSHSessionOpenRequest> = args(request_type, payload)?;
            value(service::open_session(ssh, pty, a.request).await)
        }
        "ssh_session_respond_host_key" => {
            let a: troupe_protocol::ws::SshHostKeyAnswer = args(request_type, payload)?;
            if !a.trust {
                return value(Ok(service::respond_host_key(ssh, &a.session_id, false)));
            }
            // Trusting a changed key rewrites known_hosts, so the client must
            // name the key it showed.
            let Some(fingerprint) = a.fingerprint.as_deref() else {
                return Err(SshFailure::new(
                    "Trusting a host key names the fingerprint the prompt showed",
                    "SSH_HOST_KEY_FINGERPRINT_REQUIRED",
                ));
            };
            if !service::trust_host_key_shown(ssh, &a.session_id, fingerprint) {
                return Err(SshFailure::new(
                    "No open host-key question of this session shows that key",
                    "SSH_HOST_KEY_NOT_SHOWN",
                ));
            }
            value(Ok(true))
        }
        "ssh_session_cancel" => {
            let a: SessionIdArg = args(request_type, payload)?;
            value(Ok(service::cancel_session(ssh, &a.session_id)))
        }
        "ssh_session_close" => {
            let a: SessionIdArg = args(request_type, payload)?;
            value(service::close_session(ssh, pty, &a.session_id).await)
        }
        "ssh_session_list" => value(Ok(service::list_sessions(ssh, pty))),
        "sftp_list_dir" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_list_dir(ssh, a.request).await)
        }
        "sftp_realpath" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_realpath(ssh, a.request).await)
        }
        "sftp_download" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_download(ssh, a.request).await)
        }
        "sftp_upload" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_upload(ssh, a.request).await)
        }
        "sftp_delete" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_delete(ssh, a.request).await)
        }
        "sftp_mkdir" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_mkdir(ssh, a.request).await)
        }
        "sftp_rename" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_rename(ssh, a.request).await)
        }
        "sftp_read_file" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_read_file(ssh, a.request).await)
        }
        "sftp_write_file" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_write_file(ssh, a.request).await)
        }
        "sftp_create_file" => {
            let a: RequestArg<_> = args(request_type, payload)?;
            value(service::sftp_create_file(ssh, a.request).await)
        }
        other => Err(SshFailure::new(
            format!("unknown SSH request `{other}`"),
            "SSH_INVALID_REQUEST",
        )),
    }
}

pub(super) fn request_attempt<'a>(request_type: &str, payload: &'a Value) -> Option<&'a str> {
    let (request, field) = match request_type {
        "ssh_session_open" => (payload.get("request")?, "sessionId"),
        "ssh_session_respond_host_key" | "ssh_session_cancel" | "ssh_session_close" => {
            (payload, "sessionId")
        }
        route if SSH_WS_REQUESTS.contains(&route) && route.starts_with("sftp_") => {
            (payload.get("request")?, "connectionId")
        }
        _ => return None,
    };
    request.get(field).and_then(Value::as_str)
}

/// Reject alternate target identifiers before either authorization or dispatch.
pub(super) fn valid_target_fields(request_type: &str, payload: &Value) -> bool {
    let nested = request_type == "ssh_session_open" || request_type.starts_with("sftp_");
    let field = if request_type.starts_with("sftp_") {
        "connectionId"
    } else {
        "sessionId"
    };
    let other = if field == "sessionId" {
        "connectionId"
    } else {
        "sessionId"
    };
    if nested {
        !payload.get("sessionId").is_some()
            && !payload.get("connectionId").is_some()
            && payload
                .get("request")
                .is_some_and(|r| r.get(other).is_none())
    } else {
        payload.get("connectionId").is_none() && payload.get("request").is_none()
    }
}

/// Stable per-install/window ownership also survives transport reconnects.
pub(super) fn ssh_owner(env: &HandlerEnv) -> Option<String> {
    env.identity
        .get()
        .map(|i| serde_json::json!([i.device_id, i.client_instance_id]).to_string())
}

/// Run one SSH request on the host's SSH manager.
pub(super) async fn handle_ssh_request(
    id: String,
    request_type: &str,
    payload: &Value,
    env: &HandlerEnv,
) -> WsReply {
    let Some(ssh) = env.relay.ssh() else {
        return WsReply::err(id, WsErrorCode::Unsupported, "this host does not serve SSH");
    };
    let Some(owner) = ssh_owner(env) else {
        return WsReply::err(
            id,
            WsErrorCode::Unauthorized,
            "SSH requires an authenticated principal",
        );
    };
    if !valid_target_fields(request_type, payload) {
        return WsReply::err(
            id,
            WsErrorCode::Unauthorized,
            "Conflicting SSH target fields",
        );
    }
    let attempt = request_attempt(request_type, payload);
    if let Some(attempt) = attempt {
        let mut owners = ssh.owners.lock();
        if request_type == "ssh_session_open" {
            match owners.get(attempt) {
                Some(existing) if existing != &owner => {
                    return WsReply::err(
                        id,
                        WsErrorCode::Unauthorized,
                        "SSH attempt belongs to another principal",
                    )
                }
                _ => {
                    owners.insert(attempt.to_owned(), owner.clone());
                }
            }
        } else if owners.get(attempt) != Some(&owner) {
            return WsReply::err(
                id,
                WsErrorCode::Unauthorized,
                "SSH attempt belongs to another principal",
            );
        }
    }
    if request_type == "ssh_session_list" {
        let sessions: Vec<_> = ssh
            .sessions
            .list()
            .into_iter()
            .filter(|s| ssh.owners.lock().get(&s.session_id) == Some(&owner))
            .map(|session| {
                let claim = session
                    .terminal_id
                    .as_deref()
                    .and_then(|id| env.pty.reissue_unbound_claim(id));
                service::SSHLiveSession { session, claim }
            })
            .collect();
        return match serde_json::to_value(sessions) {
            Ok(value) => WsReply::ok(id, Some(value)),
            Err(_) => WsReply::err(
                id,
                WsErrorCode::Unsupported,
                "SSH session serialization failed",
            ),
        };
    }
    match run(&ssh, &env.pty, request_type, payload).await {
        Ok(value) => WsReply::ok(id, Some(value)),
        Err(failure) => {
            // The code and the request type only: messages can name remote
            // paths or hosts, and payloads can carry secrets.
            warn!(
                target: "termul::web::ws",
                request = request_type,
                code = failure.code,
                "SSH request over /ws failed"
            );
            WsReply::err_with_code(id, failure.code, failure.message)
        }
    }
}
