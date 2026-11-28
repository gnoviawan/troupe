//! The plain [`BrowserSource`]: one browser per profile, started on first
//! use and kept until the source is dropped. Tests and the conformance
//! runner use it; the server uses `browser_host`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::launch::LaunchSpec;
use super::perimeter::Perimeter;
use super::{Browser, BrowserSource, SourceFuture};
use crate::browser_automation::engine::profile::DataStore;

/// Which persistent profile a store maps to. Ephemeral tabs run in a
/// disposable context of the agent profile's browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ProfileKey {
    User,
    Agent,
}

pub struct LocalSource {
    binary: PathBuf,
    state_dir: PathBuf,
    root: PathBuf,
    no_sandbox: bool,
    perimeter: Arc<Perimeter>,
    running: tokio::sync::Mutex<HashMap<ProfileKey, Arc<Browser>>>,
}

impl LocalSource {
    pub fn new(
        binary: PathBuf,
        state_dir: PathBuf,
        root: PathBuf,
        no_sandbox: bool,
        perimeter: Arc<Perimeter>,
    ) -> Self {
        Self {
            binary,
            state_dir,
            root,
            no_sandbox,
            perimeter,
            running: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl BrowserSource for LocalSource {
    fn browser_for<'a>(
        &'a self,
        _label: &'a str,
        store: &'a DataStore,
    ) -> SourceFuture<'a, Result<Arc<Browser>, String>> {
        Box::pin(async move {
            let key = match store {
                DataStore::App => ProfileKey::User,
                DataStore::Global | DataStore::Ephemeral { .. } => ProfileKey::Agent,
            };
            let mut running = self.running.lock().await;
            if let Some(browser) = running.get(&key).filter(|b| b.is_alive()) {
                return Ok(browser.clone());
            }
            let dir = match key {
                ProfileKey::User => "user",
                ProfileKey::Agent => "agent",
            };
            let spec = LaunchSpec {
                binary: self.binary.clone(),
                state_dir: self.state_dir.clone(),
                profile_dir: self.root.join(dir),
                no_sandbox: self.no_sandbox,
                proxy_port: self.perimeter.port(),
            };
            let browser = Browser::launch(&spec).await?;
            running.insert(key, browser.clone());
            Ok(browser)
        })
    }
}
