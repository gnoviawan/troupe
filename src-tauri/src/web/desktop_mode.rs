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

use serde_json::Value;
use tracing::{debug, error, info};

use crate::acp::{import_chat_history, ChatHistoryStore, SessionPersistence};
use crate::web::sink::WsRelaySink;
use crate::web::store::{desktop_legacy_file, WebStore};
use crate::web::watch::WatchKind;

/// The legacy renderer-authored chat history, under the Tauri app data dir.
const LEGACY_CHAT_HISTORY_DIR: &str = "acp-chat-history";

/// The launch flags `server_main` takes off argv before the shared parser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaunchFlags {
    pub desktop: bool,
    pub parent_pid: Option<u32>,
    /// `--browser-host`: serve agent browser tabs from a Chromium on this
    /// host (`web::hosted_browser`). Off by default.
    pub browser_host: Option<BrowserHostOptions>,
}

/// The `--browser-host-*` switches. They exist only with `--browser-host`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BrowserHostOptions {
    /// `--browser-host-no-sandbox`: let Chromium run without its sandbox,
    /// which it needs as root.
    pub no_sandbox: bool,
    /// `--browser-host-allow-internal`: let tabs reach loopback and private
    /// networks.
    pub allow_internal: bool,
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
        let (mut browser_host, mut browser) = (false, BrowserHostOptions::default());
        while let Some(arg) = iter.next() {
            if arg == "--desktop" {
                flags.desktop = true;
            } else if arg == "--browser-host" {
                browser_host = true;
            } else if arg == "--browser-host-no-sandbox" {
                browser.no_sandbox = true;
            } else if arg == "--browser-host-allow-internal" {
                browser.allow_internal = true;
            } else if arg == "--parent-pid" {
                let value = iter.next().ok_or("missing value for --parent-pid")?;
                flags.parent_pid = Some(parse_pid(&value)?);
            } else if let Some(value) = arg.strip_prefix("--parent-pid=") {
                flags.parent_pid = Some(parse_pid(value)?);
            } else {
                rest.push(arg);
            }
        }
        if browser_host {
            flags.browser_host = Some(browser);
        } else if browser != BrowserHostOptions::default() {
            return Err("--browser-host-no-sandbox and --browser-host-allow-internal need --browser-host".into());
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

/// Serve the host-local client's SSH requests, with profiles in the state dir
/// (the Tauri app's `ssh-profiles.json`). Session progress, host-key
/// questions, session ends and transfer progress go to the `ssh` watch as
/// [`troupe_protocol::ws::SshWatchDelta`]s, which only a host-local
/// connection may hold.
///
/// The SSH code emits from inside its connect and transfer loops, so a
/// dedicated thread publishes: an emit never waits on the watch hub, and the
/// hub's state read (the open host-key questions) never waits on a caller
/// that holds an SSH lock.
pub fn attach_ssh(relay: &WsRelaySink, state_dir: &Path) {
    let hub = Arc::clone(relay.watches());
    let owners: Arc<parking_lot::Mutex<std::collections::HashMap<String, String>>> = Arc::default();
    let publishing_owners = Arc::clone(&owners);
    let (deltas, inbox) = std::sync::mpsc::channel::<Value>();
    let publisher = std::thread::Builder::new()
        .name("ssh-watch".into())
        .spawn(move || {
            while let Ok(delta) = inbox.recv() {
                let attempt = delta
                    .get("sessionId")
                    .or_else(|| delta.get("connectionId"))
                    .and_then(Value::as_str);
                let owner = attempt.and_then(|id| publishing_owners.lock().get(id).cloned());
                if let Some(owner) = owner {
                    hub.publish(WatchKind::Ssh, &owner, || delta);
                }
            }
        });
    if let Err(error) = publisher {
        error!(
            target: "termul::desktop_mode",
            "the ssh watch publisher did not start; SSH events are not delivered: {error}"
        );
    }
    let emit: crate::ssh::Emit = Arc::new(move |event, payload| match ssh_delta(event, payload) {
        Some(delta) => {
            if deltas.send(delta).is_err() {
                debug!(target: "termul::desktop_mode", event, "SSH event not delivered");
            }
        }
        None => debug!(target: "termul::desktop_mode", event, "SSH event has no watch delta"),
    });
    let mut ssh = crate::ssh::SSHManager::for_server(state_dir, emit);
    ssh.owners = owners;
    relay.set_ssh(Arc::new(ssh));
    info!(
        target: "termul::desktop_mode",
        "SSH manager attached; profiles in '{}'",
        state_dir.display()
    );
}

/// The `ssh` watch delta of one SSH event: its payload tagged with `event`.
/// `None` for an event the watch does not carry (port forward status).
pub(crate) fn ssh_delta(event: &str, payload: Value) -> Option<Value> {
    use crate::ssh::{session, sftp};
    let tag = match event {
        session::EVENT_PROGRESS => "progress",
        session::EVENT_HOST_KEY => "host_key",
        session::EVENT_HOST_KEY_RESOLVED => "host_key_resolved",
        session::EVENT_ENDED => "ended",
        sftp::EVENT_TRANSFER_PROGRESS => "transfer",
        _ => return None,
    };
    let Value::Object(mut fields) = payload else {
        return None;
    };
    fields.insert("event".into(), Value::String(tag.into()));
    Some(Value::Object(fields))
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

/// Snapshot restricted to attempts opened by this authenticated principal.
pub fn ssh_state_for(ssh: &crate::ssh::SSHManager, owner: &str) -> Value {
    let prompts = ssh.sessions.host_key_prompts();
    let owners = ssh.owners.lock();
    let prompts: Vec<_> = prompts
        .into_iter()
        .filter(|p| {
            p.get("sessionId")
                .and_then(Value::as_str)
                .and_then(|id| owners.get(id))
                .is_some_and(|o| o == owner)
        })
        .collect();
    serde_json::json!({ "hostKeys": prompts })
}
