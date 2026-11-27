//! `troupe-server --desktop [--parent-pid <pid>]`: the server the Rune desktop
//! app starts on the Tauri app's data directory.
//!
//! The desktop mode runs the startup work only the Tauri process did before:
//! the import of the retired plugin-store file `termul-data.json`, the import
//! of the legacy renderer-authored chat history, the SSH manager (served to
//! the host-local client over `/ws`) and the keychain self-test.
//! It binds loopback only and never self-updates (the app bundle owns the
//! binary). `--parent-pid` stops the server cleanly when the app exits; it
//! works with or without `--desktop`. Both are off by default.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{error, info};

use crate::acp::{import_chat_history, ChatHistoryStore, SessionPersistence};
use crate::web::sink::WsRelaySink;
use crate::web::store::{desktop_legacy_file, WebStore};

/// The legacy renderer-authored chat history, under the Tauri app data dir.
const LEGACY_CHAT_HISTORY_DIR: &str = "acp-chat-history";

/// The launch flags `server_main` takes off argv before the shared parser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaunchFlags {
    pub desktop: bool,
    pub parent_pid: Option<u32>,
}

impl LaunchFlags {
    /// Split `--desktop` and `--parent-pid <pid>` (or `--parent-pid=<pid>`)
    /// out of `args`, returning the flags and the remaining arguments.
    ///
    /// # Errors
    /// A missing or invalid pid.
    pub fn take(args: Vec<String>) -> Result<(Self, Vec<String>), String> {
        let mut flags = Self::default();
        let mut rest = Vec::with_capacity(args.len());
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            if arg == "--desktop" {
                flags.desktop = true;
            } else if arg == "--parent-pid" {
                let value = iter.next().ok_or("missing value for --parent-pid")?;
                flags.parent_pid = Some(parse_pid(&value)?);
            } else if let Some(value) = arg.strip_prefix("--parent-pid=") {
                flags.parent_pid = Some(parse_pid(value)?);
            } else {
                rest.push(arg);
            }
        }
        Ok((flags, rest))
    }
}

fn parse_pid(value: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(pid) if pid > 0 => Ok(pid),
        _ => Err(format!(
            "invalid --parent-pid '{value}': expected a positive process id"
        )),
    }
}

/// The store with the retired plugin-store file `<state dir>/termul-data.json`
/// queued for import into the first device that claims it.
#[must_use]
pub fn with_desktop_legacy(store: WebStore, state_dir: &Path) -> WebStore {
    let legacy = desktop_legacy_file(state_dir);
    if legacy.is_file() {
        info!(
            target: "termul::desktop_mode",
            "retired plugin-store file '{}' found; it is imported into the first desktop device",
            legacy.display()
        );
    }
    store.with_desktop_legacy(legacy)
}

/// Where the legacy chat history lives, when the state dir has one.
#[must_use]
pub fn legacy_chat_history_dir(state_dir: &Path) -> Option<PathBuf> {
    let dir = state_dir.join(LEGACY_CHAT_HISTORY_DIR);
    dir.is_dir().then_some(dir)
}

/// Import the legacy renderer-authored chat history into the host sessions
/// store in the background. Idempotent: the import keeps a ledger, so a
/// session imported once (even if deleted since) is never imported again.
pub fn spawn_legacy_history_import(persistence: Arc<SessionPersistence>, state_dir: &Path) {
    let Some(root) = legacy_chat_history_dir(state_dir) else {
        return;
    };
    let chat_history = match ChatHistoryStore::open(root.clone()) {
        Ok(store) => store,
        Err(error) => {
            error!(
                target: "termul::desktop_mode",
                "legacy chat history at '{}' cannot be opened, not imported: {error}",
                root.display()
            );
            return;
        }
    };
    tokio::spawn(async move {
        let imported = import_chat_history(&persistence, &chat_history).await;
        info!(
            target: "termul::desktop_mode",
            "legacy chat history import finished: {imported} new session(s)"
        );
    });
}

/// Serve the host-local client's SSH requests: profiles in the state dir (the
/// Tauri app's `ssh-profiles.json`), events to the `ssh` watch. The emitter
/// holds the relay weakly, because the relay owns the manager.
pub fn attach_ssh(relay: &Arc<WsRelaySink>, state_dir: &Path) {
    let events = Arc::downgrade(relay);
    let emit: crate::ssh::Emit = Arc::new(move |event, payload| {
        if let Some(relay) = events.upgrade() {
            crate::web::watch::publish_ssh_event(&relay, event, payload);
        }
    });
    relay.set_ssh(Arc::new(crate::ssh::SSHManager::for_server(
        state_dir, emit,
    )));
    info!(
        target: "termul::desktop_mode",
        "SSH manager attached; profiles in '{}'",
        state_dir.display()
    );
}

/// Check that the OS keychain persists secrets. When it does not, stored SSH
/// passwords and passphrases are lost silently, so the failure is logged
/// loudly.
pub fn keychain_self_test() {
    match crate::ssh::credential_store::self_test() {
        Ok(()) => info!(
            target: "termul::desktop_mode",
            "credential keychain self-test passed"
        ),
        Err(error) => error!(
            target: "termul::desktop_mode",
            "credential keychain self-test FAILED: {error}; stored SSH credentials will not persist"
        ),
    }
}
