//! The server's browser host: it owns the Chromium processes, their
//! profiles and the screencast streams behind `troupe-server --browser-host`.
//!
//! One persistent profile is one process. A user tab of project P runs in
//! P's process; agent tabs run in the agent process (ephemeral ones in a
//! disposable context of it). A process with no open tab for
//! [`IDLE_CLOSE`] is closed, and the next tab starts it again.
//!
//! A tab's screencast runs while a `/ws` connection watches it
//! (`browser_screencast`). A connection may watch a tab only after a
//! write-gated pane request (`browser_tab_open` or `browser_tab_attach`)
//! granted it that tab, so a read-only or view-only link never sees a
//! page. The stream stops at the first frame that finds no watcher.

pub mod input;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::browser_automation::engine::chromium::launch::LaunchSpec;
use crate::browser_automation::engine::chromium::perimeter::Perimeter;
use crate::browser_automation::engine::chromium::{
    Browser, BrowserSource, Frame, FrameSink, SourceFuture, Tabs,
};
use crate::browser_automation::engine::profile::DataStore;
use crate::web::watch::WatchKind;
use crate::web::ws::OutboundSender;
use crate::web::WsRelaySink;

/// How long a process with no open tab lives on.
pub const IDLE_CLOSE: Duration = Duration::from_secs(5 * 60);

/// Largest screencast frame, in device px.
pub const MAX_FRAME: (u32, u32) = (1920, 1920);

/// Which persistent profile, so which process, serves a tab.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProfileKey {
    Project(String),
    Agent,
}

impl ProfileKey {
    /// The profile's directory name. Project ids are host-minted, but the
    /// name is still reduced to a safe alphabet, with a hash of the id so
    /// two ids never share a folder.
    pub fn dir_name(&self) -> String {
        match self {
            Self::Agent => "agent".to_string(),
            Self::Project(id) => {
                let safe: String = id
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .take(64)
                    .collect();
                let digest = format!("{:x}", Sha256::digest(id.as_bytes()));
                format!("project-{safe}-{}", &digest[..16])
            }
        }
    }
}

/// Stable ownership of a browser pane tab, surviving WebSocket reconnects.
///
/// Mirrors the identity other per-chat `/ws` resources use: the owning
/// chat/session id, its project id, and the authenticated client principal
/// (`ClientIdentity::device_id`) when the opener presented one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabOwner {
    pub session_id: String,
    pub project_id: String,
    pub principal: Option<String>,
}

impl TabOwner {
    pub fn new(
        session_id: impl Into<String>,
        project_id: impl Into<String>,
        principal: Option<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            project_id: project_id.into(),
            principal: principal.filter(|p| !p.is_empty()),
        }
    }

    /// Whether a caller on a connection subscribed (`subscribed_to_owner`) to
    /// `self.session_id`, optionally claiming a session/project and presenting
    /// an authenticated `principal`, matches this tab's owner.
    pub fn allows(
        &self,
        subscribed_to_owner: bool,
        claimed_session: Option<&str>,
        claimed_project: Option<&str>,
        principal: Option<&str>,
    ) -> bool {
        if self.session_id.is_empty() || self.project_id.is_empty() || !subscribed_to_owner {
            return false;
        }
        if claimed_session.is_some_and(|session| session != self.session_id) {
            return false;
        }
        if claimed_project.is_some_and(|project| project != self.project_id) {
            return false;
        }
        if self
            .principal
            .as_deref()
            .is_some_and(|expected| principal != Some(expected))
        {
            return false;
        }
        true
    }
}

struct Running {
    browser: Arc<Browser>,
    open_tabs: usize,
    /// Bumped on every open and close, so a stale idle timer knows it lost.
    generation: u64,
}

pub struct BrowserHost {
    me: Weak<BrowserHost>,
    binary: PathBuf,
    state_dir: PathBuf,
    profiles: PathBuf,
    no_sandbox: bool,
    perimeter: Arc<Perimeter>,
    relay: Arc<WsRelaySink>,
    running: Mutex<HashMap<ProfileKey, Running>>,
    /// Serialises launches, so two tabs never start two processes for one
    /// profile.
    launching: tokio::sync::Mutex<()>,
    /// The profile of every open tab.
    tab_profile: Mutex<HashMap<String, ProfileKey>>,
    /// The project a pane asked a user tab for, until the tab opens.
    user_project: Mutex<HashMap<String, String>>,
    /// Stable owner of each pane-opened tab, surviving reconnects.
    tab_owners: Mutex<HashMap<String, TabOwner>>,
    /// Which live connections may currently watch which tab's screencast.
    grants: Mutex<Vec<(String, OutboundSender)>>,
    tabs: Mutex<Weak<Tabs>>,
    #[cfg(test)]
    test_live_tabs: Mutex<std::collections::HashSet<String>>,
}

impl BrowserHost {
    pub fn new(
        binary: PathBuf,
        state_dir: PathBuf,
        profiles: PathBuf,
        no_sandbox: bool,
        perimeter: Arc<Perimeter>,
        relay: Arc<WsRelaySink>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            binary,
            state_dir,
            profiles,
            no_sandbox,
            perimeter,
            relay,
            running: Mutex::new(HashMap::new()),
            launching: tokio::sync::Mutex::new(()),
            tab_profile: Mutex::new(HashMap::new()),
            user_project: Mutex::new(HashMap::new()),
            tab_owners: Mutex::new(HashMap::new()),
            grants: Mutex::new(Vec::new()),
            tabs: Mutex::new(Weak::new()),
            #[cfg(test)]
            test_live_tabs: Mutex::new(std::collections::HashSet::new()),
        })
    }

    /// Attach the engine's tabs: frames come back through this host.
    pub fn attach(self: &Arc<Self>, tabs: &Arc<Tabs>) {
        *self.tabs.lock() = Arc::downgrade(tabs);
        tabs.set_frame_sink(self.clone());
    }

    pub fn tabs(&self) -> Option<Arc<Tabs>> {
        self.tabs.lock().upgrade()
    }

    /// Record the stable owner and project of a user tab before it opens.
    pub fn claim_user_tab(&self, label: &str, owner: TabOwner) {
        self.user_project
            .lock()
            .insert(label.to_string(), owner.project_id.clone());
        self.tab_owners.lock().insert(label.to_string(), owner);
    }

    /// Drop a pre-registered tab claim after an open failure.
    pub fn release_tab_claim(&self, label: &str) {
        self.user_project.lock().remove(label);
        self.tab_owners.lock().remove(label);
        self.grants.lock().retain(|(l, _)| l != label);
    }

    /// Stable owner recorded for `label`, if any.
    pub fn tab_owner(&self, label: &str) -> Option<TabOwner> {
        self.tab_owners.lock().get(label).cloned()
    }

    /// Whether `label` is currently open in the engine.
    pub fn tab_is_live(&self, label: &str) -> bool {
        #[cfg(test)]
        if self.test_live_tabs.lock().contains(label) {
            return true;
        }
        self.tabs().is_some_and(|tabs| tabs.get(label).is_ok())
    }

    #[cfg(test)]
    pub fn mark_test_tab_live(&self, label: &str) {
        self.test_live_tabs.lock().insert(label.to_string());
    }

    /// Let `out` watch `label`'s screencast.
    pub fn grant(&self, label: &str, out: &OutboundSender) {
        let mut grants = self.grants.lock();
        grants.retain(|(_, o)| !o.closing().is_cancelled());
        if !grants
            .iter()
            .any(|(l, o)| l == label && o.same_channel(out))
        {
            grants.push((label.to_string(), out.clone()));
        }
    }

    pub fn may_watch(&self, label: &str, out: &OutboundSender) -> bool {
        !out.closing().is_cancelled()
            && self
                .grants
                .lock()
                .iter()
                .any(|(l, o)| l == label && o.same_channel(out) && !o.closing().is_cancelled())
    }

    fn watchers(&self, label: &str) -> usize {
        self.relay
            .watches()
            .subscriber_count(WatchKind::BrowserScreencast, label)
    }

    /// A watcher joined `label`: make sure its stream runs.
    pub async fn watcher_joined(&self, label: &str) -> Result<(), String> {
        let tabs = self.tabs().ok_or("the browser host is shutting down")?;
        tabs.start_screencast(label, MAX_FRAME)
            .await
            .map_err(|e| format!("the screencast did not start ({:?})", e.kind))
    }

    /// A watcher left `label`: stop the stream when it was the last one.
    pub async fn watcher_left(&self, label: &str) {
        if self.watchers(label) > 0 {
            return;
        }
        if let Some(tabs) = self.tabs() {
            let _ = tabs.stop_screencast(label).await;
        }
    }

    fn profile_for(&self, label: &str, store: &DataStore) -> ProfileKey {
        match store {
            DataStore::App => ProfileKey::Project(
                self.user_project
                    .lock()
                    .remove(label)
                    .unwrap_or_else(|| "default".to_string()),
            ),
            DataStore::Global | DataStore::Ephemeral { .. } => ProfileKey::Agent,
        }
    }

    fn running_browser(&self, key: &ProfileKey) -> Option<Arc<Browser>> {
        let mut running = self.running.lock();
        let entry = running.get_mut(key)?;
        if !entry.browser.is_alive() {
            running.remove(key);
            return None;
        }
        Some(entry.browser.clone())
    }

    /// Close the process for `key` once it has had no tab for
    /// [`IDLE_CLOSE`], unless a tab opened in between.
    fn schedule_idle_close(&self, key: ProfileKey, generation: u64) {
        let me = self.me.clone();
        tokio::spawn(async move {
            tokio::time::sleep(IDLE_CLOSE).await;
            let Some(host) = me.upgrade() else {
                return;
            };
            let idle = {
                let mut running = host.running.lock();
                let still_idle = running
                    .get(&key)
                    .is_some_and(|r| r.open_tabs == 0 && r.generation == generation);
                if still_idle {
                    running.remove(&key)
                } else {
                    None
                }
            };
            if let Some(idle) = idle {
                log::info!("[browser-host] closing an idle browser");
                idle.browser.close().await;
            }
        });
    }
}

impl BrowserSource for BrowserHost {
    fn browser_for<'a>(
        &'a self,
        label: &'a str,
        store: &'a DataStore,
    ) -> SourceFuture<'a, Result<Arc<Browser>, String>> {
        Box::pin(async move {
            let key = self.profile_for(label, store);
            self.tab_profile
                .lock()
                .insert(label.to_string(), key.clone());
            let _launching = self.launching.lock().await;
            if let Some(browser) = self.running_browser(&key) {
                return Ok(browser);
            }
            let spec = LaunchSpec {
                binary: self.binary.clone(),
                state_dir: self.state_dir.clone(),
                profile_dir: self.profiles.join(key.dir_name()),
                no_sandbox: self.no_sandbox,
                proxy_port: self.perimeter.port(),
            };
            let browser = Browser::launch(&spec).await?;
            self.running.lock().insert(
                key,
                Running {
                    browser: browser.clone(),
                    open_tabs: 0,
                    generation: 0,
                },
            );
            Ok(browser)
        })
    }

    fn tab_opened(&self, label: &str) {
        let Some(key) = self.tab_profile.lock().get(label).cloned() else {
            return;
        };
        if let Some(entry) = self.running.lock().get_mut(&key) {
            entry.open_tabs += 1;
            entry.generation += 1;
        }
    }

    fn tab_closed(&self, label: &str) {
        self.grants.lock().retain(|(l, _)| l != label);
        self.tab_owners.lock().remove(label);
        self.user_project.lock().remove(label);
        #[cfg(test)]
        self.test_live_tabs.lock().remove(label);
        let Some(key) = self.tab_profile.lock().remove(label) else {
            return;
        };
        let idle = {
            let mut running = self.running.lock();
            running.get_mut(&key).and_then(|entry| {
                entry.open_tabs = entry.open_tabs.saturating_sub(1);
                entry.generation += 1;
                (entry.open_tabs == 0).then_some(entry.generation)
            })
        };
        if let Some(generation) = idle {
            self.schedule_idle_close(key, generation);
        }
    }
}

impl FrameSink for BrowserHost {
    fn frame(&self, label: &str, frame: Frame) -> bool {
        if self.watchers(label) == 0 {
            return false;
        }
        self.relay
            .watches()
            .publish(WatchKind::BrowserScreencast, label, || {
                json!({
                    "format": "jpeg",
                    "data": frame.data,
                    "width": frame.width,
                    "height": frame.height,
                })
            });
        true
    }
}

static INSTALLED: std::sync::OnceLock<Arc<BrowserHost>> = std::sync::OnceLock::new();

/// Make `host` the one the `/ws` pane requests reach.
pub fn install(host: Arc<BrowserHost>) {
    if INSTALLED.set(host).is_err() {
        log::warn!("[browser-host] a browser host was already installed");
    }
}

pub fn installed() -> Option<Arc<BrowserHost>> {
    INSTALLED.get().cloned()
}

#[cfg(test)]
mod tests;
