//! Interactive SSH sessions.
//!
//! `open` runs the connect pipeline, reports progress and host-key questions
//! to the UI as events, then adopts the remote shell into `PtyManager` as a
//! terminal. The UI answers host-key questions with `respond_host_key` and
//! can `cancel` at any stage. When the shell ends (exit, network loss,
//! close) an `ended` event says why. Each session's connection also carries
//! its SFTP browser and port forwards.
//!
//! Secrets: a password or passphrase that worked is kept in process memory
//! while at least one session for that host is alive, so a second tab to the
//! same host does not ask again. It is dropped with the last session and
//! never written anywhere by this module.

use super::connect::{
    self, BoxFuture, ConnectError, ConnectErrorKind, ConnectHooks, HostKeyDecision, Stage, Target,
};
use super::connection::SshConnection;
use super::known_hosts::{HostKeyInfo, HostKeyStatus};
use super::port_forward::PortForwardManager;
use super::shell::{self, ShellEnd};
use crate::pty::manager::{ExternalTerminal, SpawnedTerminal};
use crate::pty::PtyManager;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::mpsc;

pub const EVENT_PROGRESS: &str = "ssh-session-progress";
pub const EVENT_HOST_KEY: &str = "ssh-session-host-key";
pub const EVENT_ENDED: &str = "ssh-session-ended";

/// How long a host-key question waits for an answer before failing closed.
const HOST_KEY_PROMPT_TIMEOUT: Duration = Duration::from_secs(300);

pub type Emit = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

/// What `open` needs about the host. Built from a stored profile.
#[derive(Clone)]
pub struct SessionTarget {
    pub profile_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
}

impl SessionTarget {
    /// Everything that decides which host a secret goes to.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
            self.host,
            self.port,
            self.username,
            self.auth_method,
            self.key_path.as_deref().unwrap_or("")
        )
    }
}

/// An `open`'s hold on its session id. Dropping it (the open finished,
/// failed, or its future was dropped) releases the id, unless another open
/// has taken it since.
struct PendingReservation<'a> {
    manager: &'a SshSessionManager,
    session_id: &'a str,
    pending: Arc<Pending>,
}

impl Drop for PendingReservation<'_> {
    fn drop(&mut self) {
        let mut pending = lock(&self.manager.pending);
        if pending
            .get(self.session_id)
            .is_some_and(|held| Arc::ptr_eq(held, &self.pending))
        {
            pending.remove(self.session_id);
            lock(&self.manager.early_ends).remove(self.session_id);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub profile_id: String,
    pub terminal_id: Option<String>,
    pub connected_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedSession {
    pub session: SessionInfo,
    pub terminal: SpawnedTerminal,
}

struct LiveSession {
    info: SessionInfo,
    retains_secret: bool,
    connection: Arc<SshConnection>,
}

struct Pending {
    cancelled: AtomicBool,
    decision_tx: Mutex<Option<mpsc::UnboundedSender<HostKeyDecision>>>,
}

struct SecretEntry {
    secret: String,
    sessions: usize,
    /// The host the secret worked for ([`SessionTarget::fingerprint`]).
    target: String,
}

/// Most connects that may be in progress at once.
pub const MAX_PENDING_CONNECTS: usize = 8;

pub struct SshSessionManager {
    emit: Emit,
    // Lock order when more than one is held: live, pending, early_ends.
    pending: Mutex<HashMap<String, Arc<Pending>>>,
    live: Mutex<HashMap<String, LiveSession>>,
    /// Shells that ended before `open` registered their session.
    early_ends: Mutex<HashMap<String, ShellEnd>>,
    secrets: Mutex<HashMap<String, SecretEntry>>,
    pub port_forwards: PortForwardManager,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

struct SessionHooks {
    session_id: String,
    pending: Arc<Pending>,
    decisions: tokio::sync::Mutex<mpsc::UnboundedReceiver<HostKeyDecision>>,
    emit: Emit,
}

impl ConnectHooks for SessionHooks {
    fn stage(&self, stage: Stage) {
        (self.emit)(
            EVENT_PROGRESS,
            json!({ "sessionId": self.session_id, "stage": stage }),
        );
    }

    fn host_key(&self, info: HostKeyInfo, status: HostKeyStatus) -> BoxFuture<'_, HostKeyDecision> {
        Box::pin(async move {
            log::info!(
                "[SSH] Session {} asks to trust a host key ({:?})",
                crate::logging::redact_session_id(&self.session_id),
                status
            );
            (self.emit)(
                EVENT_HOST_KEY,
                json!({
                    "sessionId": self.session_id,
                    "status": status,
                    "host": info.host,
                    "port": info.port,
                    "keyType": info.key_type,
                    "fingerprint": info.fingerprint,
                }),
            );
            let mut decisions = self.decisions.lock().await;
            // A dropped sender (cancel) or the timeout both reject.
            match tokio::time::timeout(HOST_KEY_PROMPT_TIMEOUT, decisions.recv()).await {
                Ok(Some(decision)) if !self.cancelled() => decision,
                _ => HostKeyDecision::Reject,
            }
        })
    }

    fn cancelled(&self) -> bool {
        self.pending.cancelled.load(Ordering::Acquire)
    }

    fn asks_user(&self) -> bool {
        true
    }
}

/// Resolves once the session's connect was cancelled.
async fn wait_cancelled(hooks: &SessionHooks) {
    while !hooks.cancelled() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn end_payload(session_id: &str, terminal_id: Option<&str>, end: &ShellEnd) -> serde_json::Value {
    let (reason, exit_code, detail) = match end {
        ShellEnd::Exited(code) => ("exited", *code, None),
        ShellEnd::Lost(detail) => ("lost", None, Some(detail.clone())),
        ShellEnd::Closed => ("closed", None, None),
    };
    json!({
        "sessionId": session_id,
        "terminalId": terminal_id,
        "reason": reason,
        "exitCode": exit_code,
        "detail": detail,
    })
}

impl SshSessionManager {
    pub fn new(emit: Emit) -> Arc<Self> {
        Arc::new(Self {
            port_forwards: PortForwardManager::new(emit.clone()),
            emit,
            pending: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
            early_ends: Mutex::new(HashMap::new()),
            secrets: Mutex::new(HashMap::new()),
        })
    }

    /// The secret kept for a host with a live session, if any.
    pub fn cached_secret(&self, profile_id: &str) -> Option<String> {
        lock(&self.secrets)
            .get(profile_id)
            .map(|entry| entry.secret.clone())
    }

    /// [`Self::cached_secret`], only when it worked for this exact target:
    /// a profile edited to another host, port, user or key never gets the
    /// old host's secret.
    pub fn cached_secret_for(&self, target: &SessionTarget) -> Option<String> {
        lock(&self.secrets)
            .get(&target.profile_id)
            .filter(|entry| entry.target == target.fingerprint())
            .map(|entry| entry.secret.clone())
    }

    fn retain_secret(&self, target: &SessionTarget, secret: &str) {
        let mut secrets = lock(&self.secrets);
        let entry = secrets
            .entry(target.profile_id.clone())
            .or_insert_with(|| SecretEntry {
                secret: String::new(),
                sessions: 0,
                target: String::new(),
            });
        // The newest secret that worked wins (the user may have changed it).
        entry.secret = secret.to_string();
        entry.target = target.fingerprint();
        entry.sessions += 1;
    }

    fn release_secret(&self, profile_id: &str) {
        let mut secrets = lock(&self.secrets);
        if let Some(entry) = secrets.get_mut(profile_id) {
            entry.sessions = entry.sessions.saturating_sub(1);
            if entry.sessions == 0 {
                secrets.remove(profile_id);
            }
        }
    }

    /// Forget a host's cached secret now (the host was edited or deleted).
    pub fn forget_secret(&self, profile_id: &str) {
        lock(&self.secrets).remove(profile_id);
    }

    /// Connect, ask the UI about the host key when needed, open the shell and
    /// adopt it as a terminal. `secret` is the password or key passphrase to
    /// use; `None` means none is available.
    pub async fn open(
        self: &Arc<Self>,
        pty: &PtyManager,
        session_id: &str,
        target: SessionTarget,
        secret: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<OpenedSession, ConnectError> {
        let (decision_tx, decision_rx) = mpsc::unbounded_channel();
        let pending = Arc::new(Pending {
            cancelled: AtomicBool::new(false),
            decision_tx: Mutex::new(Some(decision_tx)),
        });
        {
            // Lock order: live before pending (see SshSessionManager).
            let live = lock(&self.live);
            let mut pending_map = lock(&self.pending);
            if pending_map.contains_key(session_id) || live.contains_key(session_id) {
                return Err(ConnectError::new(
                    ConnectErrorKind::Internal,
                    "Session id already in use",
                ));
            }
            if pending_map.len() >= MAX_PENDING_CONNECTS {
                return Err(ConnectError::new(
                    ConnectErrorKind::Internal,
                    "Too many connections are being opened; wait for one to finish",
                ));
            }
            pending_map.insert(session_id.to_string(), pending.clone());
        }
        // Dropped with this future (a client that disconnected mid-open
        // aborts its request): the reservation is released, not left until
        // the host-key timeout.
        let _reservation = PendingReservation {
            manager: self,
            session_id,
            pending: Arc::clone(&pending),
        };
        log::info!(
            "[SSH] Opening session {} for profile {} ({})",
            crate::logging::redact_session_id(session_id),
            target.profile_id,
            target.auth_method
        );

        let hooks = Arc::new(SessionHooks {
            session_id: session_id.to_string(),
            pending: pending.clone(),
            decisions: tokio::sync::Mutex::new(decision_rx),
            emit: self.emit.clone(),
        });
        let started = self
            .connect_and_start(session_id, &target, secret.as_deref(), hooks, cols, rows)
            .await;
        let (connection, handles) = match started {
            Ok(started) => started,
            Err(error) => {
                lock(&self.pending).remove(session_id);
                lock(&self.early_ends).remove(session_id);
                log::warn!(
                    "[SSH] Session {} failed: {}",
                    crate::logging::redact_session_id(session_id),
                    error.kind.code()
                );
                return Err(error);
            }
        };

        // Register before adopting: the shell may end at any moment, and
        // `finish` must find the entry to release the secret. The id stays
        // reserved (pending) until this swap, so no second `open` can take it.
        let retains_secret = secret.is_some();
        // Decide under the locks; await only after they are released.
        let (cancelled, early_end) = {
            let mut live = lock(&self.live);
            lock(&self.pending).remove(session_id);
            let early_end = lock(&self.early_ends).remove(session_id);
            let cancelled = pending.cancelled.load(Ordering::Acquire);
            if !cancelled && early_end.is_none() {
                if let Some(secret) = secret.as_deref() {
                    self.retain_secret(&target, secret);
                }
                live.insert(
                    session_id.to_string(),
                    LiveSession {
                        info: SessionInfo {
                            session_id: session_id.to_string(),
                            profile_id: target.profile_id.clone(),
                            terminal_id: None,
                            connected_at: chrono::Utc::now().to_rfc3339(),
                        },
                        retains_secret,
                        connection: connection.clone(),
                    },
                );
            }
            (cancelled, early_end)
        };
        if cancelled {
            drop(handles);
            connection.close().await;
            log::info!(
                "[SSH] Session {} cancelled while its shell started",
                crate::logging::redact_session_id(session_id)
            );
            return Err(ConnectError::new(ConnectErrorKind::Cancelled, "Cancelled"));
        }
        if let Some(end) = early_end {
            // The shell exited before registration: report it now, and still
            // adopt the terminal below so its last output stays visible.
            self.announce_end(session_id, None, &end);
            connection.close().await;
        }

        let terminal = pty
            .adopt_external(ExternalTerminal {
                reader: handles.reader,
                writer: handles.writer,
                child: handles.child,
                resizer: handles.resizer,
                shell: format!("ssh {}@{}", target.username, target.host),
                cwd: format!("{}:{}", target.host, target.port),
                cols,
                rows,
            })
            .await
            // The handles were dropped with the failed adopt, which closes
            // the shell and runs `finish`.
            .map_err(|e| ConnectError::new(ConnectErrorKind::Internal, e))?;

        let info = {
            let mut live = lock(&self.live);
            match live.get_mut(session_id) {
                Some(entry) => {
                    entry.info.terminal_id = Some(terminal.info.id.clone());
                    entry.info.clone()
                }
                // Ended already (the shell exited immediately); the ended
                // event is out and the terminal still shows the output.
                None => SessionInfo {
                    session_id: session_id.to_string(),
                    profile_id: target.profile_id.clone(),
                    terminal_id: Some(terminal.info.id.clone()),
                    connected_at: chrono::Utc::now().to_rfc3339(),
                },
            }
        };
        log::info!(
            "[SSH] Session {} connected (terminal {})",
            crate::logging::redact_session_id(session_id),
            terminal.info.id
        );
        Ok(OpenedSession {
            session: info,
            terminal,
        })
    }

    async fn connect_and_start(
        self: &Arc<Self>,
        session_id: &str,
        target: &SessionTarget,
        secret: Option<&str>,
        hooks: Arc<SessionHooks>,
        cols: u16,
        rows: u16,
    ) -> Result<(Arc<SshConnection>, shell::ShellHandles), ConnectError> {
        let connected = connect::connect(
            &Target {
                host: &target.host,
                port: target.port,
                username: &target.username,
                auth_method: &target.auth_method,
                key_path: target.key_path.as_deref(),
            },
            secret,
            hooks.clone(),
        )
        .await?;
        let connection = SshConnection::new(connected);
        if hooks.cancelled() {
            connection.close().await;
            return Err(ConnectError::new(ConnectErrorKind::Cancelled, "Cancelled"));
        }
        hooks.stage(Stage::StartingShell);
        let weak: Weak<Self> = Arc::downgrade(self);
        let ended_session_id = session_id.to_string();
        let start = shell::start(
            connection.clone(),
            cols,
            rows,
            Box::new(move |end| {
                if let Some(manager) = weak.upgrade() {
                    manager.finish(&ended_session_id, end);
                }
            }),
        );
        // Cancel must also stop a shell that is still being requested.
        let started = tokio::select! {
            started = start => started,
            () = wait_cancelled(&hooks) => {
                Err(ConnectError::new(ConnectErrorKind::Cancelled, "Cancelled"))
            }
        };
        match started {
            Ok(handles) => Ok((connection, handles)),
            Err(error) => {
                connection.close().await;
                Err(error)
            }
        }
    }

    /// Called once from the shell reader task when the shell ended.
    fn finish(&self, session_id: &str, end: ShellEnd) {
        let removed = {
            let mut live = lock(&self.live);
            let removed = live.remove(session_id);
            if removed.is_none() && lock(&self.pending).contains_key(session_id) {
                // `open` has not registered it yet; it reports this end.
                lock(&self.early_ends).insert(session_id.to_string(), end);
                return;
            }
            removed
        };
        let Some(session) = removed else {
            // A shell `open` dropped (cancelled at registration): nothing to report.
            self.port_forwards.stop_all_for_session(session_id);
            return;
        };
        if session.retains_secret {
            self.release_secret(&session.info.profile_id);
        }
        self.announce_end(session_id, session.info.terminal_id.as_deref(), &end);
    }

    fn announce_end(&self, session_id: &str, terminal_id: Option<&str>, end: &ShellEnd) {
        self.port_forwards.stop_all_for_session(session_id);
        log::info!(
            "[SSH] Session {} ended",
            crate::logging::redact_session_id(session_id)
        );
        (self.emit)(EVENT_ENDED, end_payload(session_id, terminal_id, end));
    }

    /// Close a session whose shell is still being adopted (no terminal yet).
    /// Returns false when there is no such session.
    pub async fn close_unadopted(&self, session_id: &str) -> bool {
        let connection = lock(&self.live)
            .get(session_id)
            .filter(|s| s.info.terminal_id.is_none())
            .map(|s| s.connection.clone());
        match connection {
            Some(connection) => {
                connection.close().await;
                true
            }
            None => false,
        }
    }

    /// Answer a pending host-key question.
    pub fn respond_host_key(&self, session_id: &str, trust: bool) -> bool {
        let Some(pending) = lock(&self.pending).get(session_id).cloned() else {
            return false;
        };
        let decision = if trust {
            HostKeyDecision::Trust
        } else {
            HostKeyDecision::Reject
        };
        let sent = lock(&pending.decision_tx)
            .as_ref()
            .is_some_and(|tx| tx.send(decision).is_ok());
        log::info!(
            "[SSH] Session {} host key {}",
            crate::logging::redact_session_id(session_id),
            if trust { "trusted" } else { "rejected" }
        );
        sent
    }

    /// Abort a connect in progress. Returns false when nothing was pending.
    pub fn cancel(&self, session_id: &str) -> bool {
        let Some(pending) = lock(&self.pending).get(session_id).cloned() else {
            return false;
        };
        pending.cancelled.store(true, Ordering::Release);
        // Unblock a host-key question.
        lock(&pending.decision_tx).take();
        log::info!(
            "[SSH] Session {} cancelled",
            crate::logging::redact_session_id(session_id)
        );
        true
    }

    /// Terminal id of a live session.
    pub fn terminal_id(&self, session_id: &str) -> Option<String> {
        lock(&self.live)
            .get(session_id)
            .and_then(|s| s.info.terminal_id.clone())
    }

    /// The connection of a live session (SFTP, port forwards).
    pub fn connection(&self, session_id: &str) -> Option<Arc<SshConnection>> {
        lock(&self.live)
            .get(session_id)
            .map(|s| s.connection.clone())
    }

    /// Live sessions (for a renderer that reloaded and lost its state).
    pub fn list(&self) -> Vec<SessionInfo> {
        lock(&self.live).values().map(|s| s.info.clone()).collect()
    }

    /// Abort every pending connect and stop forwards (app exit). Live shells
    /// are closed by `PtyManager::kill_all`.
    pub fn shutdown(&self) {
        let ids: Vec<String> = lock(&self.pending).keys().cloned().collect();
        for id in ids {
            self.cancel(&id);
        }
        self.port_forwards.stop_all();
    }
}

#[cfg(test)]
mod tests;
