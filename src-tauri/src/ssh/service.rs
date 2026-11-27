//! The SSH operations behind the `/ws` SSH requests (`web/ws/ssh.rs`), which
//! a host-local client of `troupe-server --desktop` sends. They match the
//! Tauri commands in `commands/ssh.rs` one for one; each returns its value or
//! an [`SshFailure`] carrying the stable code the client branches on.
//!
//! Secrets (passwords, passphrases) pass through `open_session` and
//! `save_profile`; nothing here logs or returns them.

use std::future::Future;
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};

use super::config_parser;
use super::keys::{self, KeyFileInfo};
use super::profile_manager::SSHProfile;
use super::session::{OpenedSession, SessionInfo, SessionTarget};
use super::sftp::{self as sftp_ops, SFTPEntry};
use super::SSHManager;
use crate::pty::PtyManager;

/// A refused or failed operation: user-facing text plus a stable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshFailure {
    pub message: String,
    pub code: &'static str,
}

impl SshFailure {
    pub fn new(message: impl Into<String>, code: &'static str) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

pub type SshResult<T> = Result<T, SshFailure>;

/// SFTP requests address a live session (`connectionId` = session id).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SFTPPathRequest {
    pub connection_id: String,
    pub remote_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SFTPTransferRequest {
    pub connection_id: String,
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SFTPRenameRequest {
    pub connection_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SFTPFileRequest {
    pub connection_id: String,
    pub remote_path: String,
    pub content: Option<String>,
}

/// No `Debug`: it carries the prompt secret.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHSessionOpenRequest {
    /// Chosen by the client so progress events match before the open returns.
    pub session_id: String,
    pub profile_id: String,
    pub cols: u16,
    pub rows: u16,
    /// Password or key passphrase typed at a prompt.
    pub secret: Option<String>,
    /// Save `secret` to the OS keychain once it worked.
    #[serde(default)]
    pub save_secret: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHSessionOpenResponse {
    #[serde(flatten)]
    pub opened: OpenedSession,
    /// The prompt secret was saved to the keychain.
    pub secret_saved: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHLiveSession {
    #[serde(flatten)]
    pub session: SessionInfo,
    /// Fresh attach credential for the session's terminal.
    pub claim: Option<String>,
}

fn profile_error(e: String) -> SshFailure {
    SshFailure::new(e, "SSH_PROFILE_ERROR")
}

pub fn list_profiles(ssh: &SSHManager) -> SshResult<Vec<SSHProfile>> {
    ssh.profiles.list().map_err(profile_error)
}

/// The host a profile's secret goes to.
fn target_of(profile: &SSHProfile) -> SessionTarget {
    SessionTarget {
        profile_id: profile.id.clone(),
        host: profile.host.clone(),
        port: profile.port,
        username: profile.username.clone(),
        auth_method: profile.auth_method.clone(),
        key_path: profile.private_key_path.clone(),
    }
}

pub fn save_profile(ssh: &SSHManager, mut profile: SSHProfile) -> SshResult<()> {
    // A new secret, or a different host/user/auth, makes the in-memory copy
    // kept for live sessions stale (a later tab would try the old one first).
    let profile_id = profile.id.clone();
    let secret_changed = profile.password.is_some() || profile.passphrase.is_some();
    let previous = ssh.profiles.get(&profile_id).map_err(profile_error)?;
    let target_changed = previous.as_ref().is_some_and(|previous| {
        target_of(previous).fingerprint() != target_of(&profile).fingerprint()
    });
    // Whether a secret is stored is the host's record, never the client's
    // claim: `ProfileManager::save` sets the flag again only for a secret
    // sent with this save. A stored secret belongs to the target it was
    // typed for, so a profile moved to another host, port, user, auth method
    // or key loses it, from the keychain too.
    let (stored_password, stored_passphrase) = match &previous {
        Some(previous) if !target_changed => {
            (previous.has_stored_password, previous.has_stored_passphrase)
        }
        _ => (false, false),
    };
    profile.has_stored_password = stored_password;
    profile.has_stored_passphrase = stored_passphrase;
    if target_changed {
        if previous
            .as_ref()
            .is_some_and(|previous| previous.has_stored_password || previous.has_stored_passphrase)
        {
            if let Err(e) = super::credential_store::delete_credentials(&profile_id) {
                log::warn!(
                    "[SSH] Could not delete the moved profile's stored secrets: {}",
                    e
                );
            }
        }
        log::info!("[SSH] Profile target changed; stored secrets need entering again");
    }
    ssh.profiles.save(profile).map_err(profile_error)?;
    if secret_changed || target_changed {
        ssh.sessions.forget_secret(&profile_id);
    }
    Ok(())
}

pub fn delete_profile(ssh: &SSHManager, profile_id: &str) -> SshResult<()> {
    ssh.profiles.delete(profile_id).map_err(profile_error)?;
    ssh.sessions.forget_secret(profile_id);
    Ok(())
}

pub fn duplicate_profile(ssh: &SSHManager, profile_id: &str) -> SshResult<SSHProfile> {
    ssh.profiles.duplicate(profile_id).map_err(profile_error)
}

pub fn import_config(ssh: &SSHManager) -> SshResult<Vec<SSHProfile>> {
    ssh.profiles
        .import_from_config(config_parser::parse_ssh_config())
        .map_err(|e| SshFailure::new(e, "SSH_IMPORT_ERROR"))
}

#[must_use]
pub fn inspect_key(path: &str) -> KeyFileInfo {
    keys::inspect(path)
}

#[must_use]
pub fn list_keys() -> Vec<KeyFileInfo> {
    keys::discover()
}

/// The keychain secret matching the profile's auth method.
fn stored_secret(profile: &SSHProfile) -> Option<String> {
    match profile.auth_method.as_str() {
        "password" => profile.password.clone(),
        "key" => profile.passphrase.clone(),
        _ => None,
    }
}

pub async fn open_session(
    ssh: &SSHManager,
    pty: &PtyManager,
    request: SSHSessionOpenRequest,
) -> SshResult<SSHSessionOpenResponse> {
    let profile = ssh
        .profiles
        .get_with_credentials(&request.profile_id)
        .map_err(profile_error)?
        .ok_or_else(|| SshFailure::new("Profile not found", "SSH_PROFILE_NOT_FOUND"))?;
    // Prompt answer, then the secret a live session to this exact target
    // used, then the keychain. All three belong to this one snapshot of the
    // profile, so an edit made meanwhile never mixes hosts and secrets.
    let target = target_of(&profile);
    let fingerprint = target.fingerprint();
    let prompted = request.secret;
    let secret = prompted
        .clone()
        .or_else(|| ssh.sessions.cached_secret_for(&target))
        .or_else(|| stored_secret(&profile));
    let opened = ssh
        .sessions
        .open(
            pty,
            &request.session_id,
            target,
            secret,
            request.cols.max(1),
            request.rows.max(1),
        )
        .await
        .map_err(|e| SshFailure::new(e.detail, e.kind.code()))?;

    // The profile may have been edited while the session opened; the
    // prompted secret is saved only if it still points at the host it
    // worked for.
    let unchanged = matches!(
        ssh.profiles.get(&profile.id),
        Ok(Some(now)) if target_of(&now).fingerprint() == fingerprint
    );
    let mut secret_saved = false;
    if request.save_secret && !unchanged {
        log::warn!("[SSH] Profile changed during connect; the typed secret was not saved");
    }
    if request.save_secret && unchanged {
        if let Some(secret) = prompted.as_deref() {
            match ssh.profiles.store_secret(&profile.id, secret) {
                Ok(()) => secret_saved = true,
                Err(e) => log::warn!("[SSH] Could not save secret to keychain: {}", e),
            }
        }
    }
    if let Err(e) = ssh.profiles.update_last_connected(&profile.id) {
        log::warn!("[SSH] Could not record last connection: {}", e);
    }
    Ok(SSHSessionOpenResponse {
        opened,
        secret_saved,
    })
}

#[must_use]
pub fn respond_host_key(ssh: &SSHManager, session_id: &str, trust: bool) -> bool {
    ssh.sessions.respond_host_key(session_id, trust)
}

/// Trust the open host-key question of `session_id` when `fingerprint` is
/// the key it shows; false otherwise (see
/// [`super::session::SshSessionManager::trust_host_key_shown`]).
#[must_use]
pub fn trust_host_key_shown(ssh: &SSHManager, session_id: &str, fingerprint: &str) -> bool {
    ssh.sessions.trust_host_key_shown(session_id, fingerprint)
}

#[must_use]
pub fn cancel_session(ssh: &SSHManager, session_id: &str) -> bool {
    ssh.sessions.cancel(session_id)
}

/// Close a session: kill its terminal (which closes the channel and the
/// connection), or cancel the connect while it is still in progress.
pub async fn close_session(ssh: &SSHManager, pty: &PtyManager, session_id: &str) -> SshResult<()> {
    match ssh.sessions.terminal_id(session_id) {
        Some(terminal_id) => pty
            .force_kill(&terminal_id)
            .await
            .map_err(|e| SshFailure::new(e, "SSH_SESSION_CLOSE_ERROR")),
        None => {
            if !ssh.sessions.cancel(session_id) {
                ssh.sessions.close_unadopted(session_id).await;
            }
            Ok(())
        }
    }
}

/// Live sessions with fresh attach claims, for a client that reloaded.
#[must_use]
pub fn list_sessions(ssh: &SSHManager, pty: &PtyManager) -> Vec<SSHLiveSession> {
    ssh.sessions
        .list()
        .into_iter()
        .map(|session| {
            let claim = session
                .terminal_id
                .as_deref()
                .and_then(|id| pty.reissue_unbound_claim(id));
            SSHLiveSession { session, claim }
        })
        .collect()
}

/// Run `op` on the file browser channel of a live session.
async fn with_sftp<T, F, Fut>(
    ssh: &SSHManager,
    session_id: &str,
    code: &'static str,
    op: F,
) -> SshResult<T>
where
    F: FnOnce(Arc<SftpSession>) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let connection = ssh.sessions.connection(session_id).ok_or_else(|| {
        SshFailure::new("The session is not connected", "SSH_CONNECTION_NOT_FOUND")
    })?;
    let sftp = connection
        .sftp()
        .await
        .map_err(|e| SshFailure::new(e, "SFTP_UNAVAILABLE"))?;
    op(sftp).await.map_err(|e| SshFailure::new(e, code))
}

pub async fn sftp_list_dir(ssh: &SSHManager, r: SFTPPathRequest) -> SshResult<Vec<SFTPEntry>> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_ERROR", |sftp| async move {
        sftp_ops::list_dir(&sftp, &r.remote_path).await
    })
    .await
}

pub async fn sftp_realpath(ssh: &SSHManager, r: SFTPPathRequest) -> SshResult<String> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_ERROR", |sftp| async move {
        sftp_ops::realpath(&sftp, &r.remote_path).await
    })
    .await
}

pub async fn sftp_download(ssh: &SSHManager, r: SFTPTransferRequest) -> SshResult<()> {
    let sink = Arc::clone(&ssh.emit);
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_DOWNLOAD_ERROR", |sftp| async move {
        sftp_ops::download_file_to(
            &sftp,
            &r.remote_path,
            &r.local_path,
            &*sink,
            &r.connection_id,
        )
        .await
    })
    .await
}

pub async fn sftp_upload(ssh: &SSHManager, r: SFTPTransferRequest) -> SshResult<()> {
    let sink = Arc::clone(&ssh.emit);
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_UPLOAD_ERROR", |sftp| async move {
        sftp_ops::upload_file_from(
            &sftp,
            &r.local_path,
            &r.remote_path,
            &*sink,
            &r.connection_id,
        )
        .await
    })
    .await
}

pub async fn sftp_delete(ssh: &SSHManager, r: SFTPPathRequest) -> SshResult<()> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_DELETE_ERROR", |sftp| async move {
        sftp_ops::delete_path(&sftp, &r.remote_path).await
    })
    .await
}

pub async fn sftp_mkdir(ssh: &SSHManager, r: SFTPPathRequest) -> SshResult<()> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_MKDIR_ERROR", |sftp| async move {
        sftp_ops::mkdir(&sftp, &r.remote_path).await
    })
    .await
}

pub async fn sftp_rename(ssh: &SSHManager, r: SFTPRenameRequest) -> SshResult<()> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_RENAME_ERROR", |sftp| async move {
        sftp_ops::rename(&sftp, &r.old_path, &r.new_path).await
    })
    .await
}

pub async fn sftp_read_file(ssh: &SSHManager, r: SFTPFileRequest) -> SshResult<String> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_READ_ERROR", |sftp| async move {
        sftp_ops::read_file_to_string(&sftp, &r.remote_path).await
    })
    .await
}

pub async fn sftp_write_file(ssh: &SSHManager, r: SFTPFileRequest) -> SshResult<()> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_WRITE_ERROR", |sftp| async move {
        let content = r.content.unwrap_or_default();
        sftp_ops::write_file_from_string(&sftp, &r.remote_path, &content).await
    })
    .await
}

pub async fn sftp_create_file(ssh: &SSHManager, r: SFTPPathRequest) -> SshResult<()> {
    let session = r.connection_id.clone();
    with_sftp(ssh, &session, "SFTP_CREATE_ERROR", |sftp| async move {
        sftp_ops::create_file(&sftp, &r.remote_path).await
    })
    .await
}
