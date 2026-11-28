//! SSH & Remote Connection Manager module
//!
//! Interactive sessions (native russh shell channels adopted as terminals),
//! host profiles, host key trust, and SFTP / port forwarding over each
//! session's connection. See `docs/ssh-native.md`.

pub mod config_parser;
pub mod connect;
pub mod connection;
pub mod credential_store;
pub mod keys;
pub mod known_hosts;
pub mod port_forward;
pub mod profile_manager;
pub mod service;
pub mod session;
pub mod sftp;
pub mod shell;

use profile_manager::ProfileManager;
pub use session::Emit;
use session::SshSessionManager;
use std::path::Path;
use std::sync::Arc;
use tauri::Emitter;

/// Top-level SSH manager that coordinates all SSH subsystems
pub struct SSHManager {
    /// Server-side attempt ownership, registered before connect emits.
    pub owners: Arc<parking_lot::Mutex<std::collections::HashMap<String, String>>>,
    pub profiles: Arc<ProfileManager>,
    pub sessions: Arc<SshSessionManager>,
    /// Where session progress, host-key questions, session ends, transfer
    /// progress and port-forward status go: Tauri events in the Tauri app,
    /// a log line on `troupe-server --desktop` (`web::desktop_mode`).
    pub emit: Emit,
}

impl SSHManager {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        let profiles = Arc::new(ProfileManager::new(app_handle.clone()));
        let emitter = app_handle;
        let emit: Emit = Arc::new(move |event, payload| {
            if let Err(e) = emitter.emit(event, payload) {
                log::error!("[SSH] Failed to emit {}: {}", event, e);
            }
        });
        Self {
            owners: Arc::default(),
            profiles,
            sessions: SshSessionManager::new(emit.clone()),
            emit,
        }
    }

    /// The server's manager: profiles in `data_dir`, events to `emit`.
    pub fn for_server(data_dir: &Path, emit: Emit) -> Self {
        Self {
            owners: Arc::default(),
            profiles: Arc::new(ProfileManager::in_dir(data_dir)),
            sessions: SshSessionManager::new(emit.clone()),
            emit,
        }
    }

    /// App exit: abort pending connects and stop port forwards. Live shells
    /// (and with them their connections) end via `PtyManager::kill_all`.
    pub async fn shutdown(&self) {
        self.sessions.shutdown();
    }
}
