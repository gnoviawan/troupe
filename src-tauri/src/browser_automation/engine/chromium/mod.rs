//! Chromium adapter: a headless Chromium the host starts itself and drives
//! over CDP on a pipe (`--remote-debugging-pipe`), never a port.
//!
//! The engine-neutral CDP helpers in [`super::cdp`] run unchanged over the
//! tab sessions here. A [`BrowserSource`] decides which running Chromium
//! serves a tab, so the process policy (profiles per project, the idle
//! timer) lives with whoever owns the processes, not in the adapter.
//! Every request a tab makes goes through the [`perimeter`] proxy.

mod browser;
pub mod launch;
pub mod perimeter;
mod pipe;
mod tab;

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

pub use browser::Browser;
pub use tab::Tabs;

use super::assets::Script;
use super::cdp::{self, input as cdp_input, CdpMethod, CdpTransport};
use super::events::DialogAnswer;
use super::profile::DataStore;
use super::{
    BrowserEngine, CapabilityId, CapabilityTable, EngineError, EngineErrorKind, EngineFacts,
    EngineFuture, EngineTraits, InputMode, KeyInput, Point, Support,
};
use crate::browser_automation::budgets::EVAL_STEP;
use launch::LaunchSpec;
use perimeter::Perimeter;

pub type SourceFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Supplies the running Chromium that serves a tab.
pub trait BrowserSource: Send + Sync {
    /// The browser for `label` on `store`, launched if none runs.
    fn browser_for<'a>(
        &'a self,
        label: &'a str,
        store: &'a DataStore,
    ) -> SourceFuture<'a, Result<Arc<Browser>, String>>;
    /// A tab of that browser opened.
    fn tab_opened(&self, _label: &str) {}
    /// A tab of that browser closed.
    fn tab_closed(&self, _label: &str) {}
}

/// Which persistent profile a store maps to. Ephemeral tabs run in a
/// disposable context of the agent profile's browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ProfileKey {
    User,
    Agent,
}

/// The plain source: one browser per profile under `root`, started on
/// first use and kept until the source is dropped.
pub struct LocalSource {
    binary: PathBuf,
    root: PathBuf,
    no_sandbox: bool,
    perimeter: Arc<Perimeter>,
    running: tokio::sync::Mutex<HashMap<ProfileKey, Arc<Browser>>>,
}

impl LocalSource {
    pub fn new(
        binary: PathBuf,
        root: PathBuf,
        no_sandbox: bool,
        perimeter: Arc<Perimeter>,
    ) -> Self {
        Self {
            binary,
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

impl CdpTransport for Tabs {
    fn call<'a>(
        &'a self,
        tab: &'a str,
        method: CdpMethod,
        params: Value,
    ) -> EngineFuture<'a, Value> {
        Box::pin(async move { self.get(tab)?.call(method, params).await })
    }

    /// Child frames in the page's process share its session. Out-of-process
    /// frames are not attached, so the page's session refuses them.
    fn call_in<'a>(
        &'a self,
        tab: &'a str,
        _frame: Option<&'a str>,
        method: CdpMethod,
        params: Value,
    ) -> EngineFuture<'a, Value> {
        self.call(tab, method, params)
    }
}

/// The Chromium engine over the tabs of [`Tabs`].
pub struct ChromiumEngine {
    tabs: Arc<Tabs>,
    facts: EngineFacts,
}

impl ChromiumEngine {
    pub fn new(tabs: Arc<Tabs>) -> Self {
        Self {
            tabs,
            facts: EngineFacts {
                capabilities: capability_table(),
                traits: EngineTraits {
                    isolated_world: true,
                    background_render: true,
                    network_observe: false,
                },
            },
        }
    }
}

/// Every input goes through CDP `Input.*`, so it is trusted. The agent tab
/// floor is native too:
/// - navigation: `Fetch` pauses each document request for the core;
/// - new windows: every new target starts paused, and a popup is closed
///   before it runs while its opener reports the request;
/// - downloads: Chromium names each one to the host, which asks the core;
/// - dialogs: `Page.javascriptDialogOpening` holds them for the core;
/// - file choosers: intercepted and cancelled, inputs set by element;
/// - permissions: `--deny-permission-prompts` refuses every request;
/// - external apps: headless Chromium has no prompt to launch one.
/// The console and the network are not observed yet.
pub fn capability_table() -> CapabilityTable {
    CapabilityTable::unavailable("not implemented on the Chromium engine yet")
        .with(CapabilityId::IsolatedEval, Support::Trusted)
        .with(CapabilityId::PersistentProfile, Support::Trusted)
        .with(CapabilityId::NavigationIntercept, Support::Trusted)
        .with(CapabilityId::NewWindowIntercept, Support::Trusted)
        .with(CapabilityId::DownloadIntercept, Support::Trusted)
        .with(CapabilityId::DialogIntercept, Support::Trusted)
        .with(CapabilityId::FileChooser, Support::Trusted)
        .with(CapabilityId::PermissionIntercept, Support::Trusted)
        .with(CapabilityId::ExternalProtocolIntercept, Support::Trusted)
        .with(CapabilityId::Screenshot, Support::Trusted)
        .with(CapabilityId::PointerClick, Support::Trusted)
        .with(CapabilityId::TrustedKey, Support::Trusted)
        .with(CapabilityId::PointerMove, Support::Trusted)
        .with(CapabilityId::Wheel, Support::Trusted)
        .with(CapabilityId::Drag, Support::Trusted)
}

fn trusted_only(mode: InputMode) -> Result<(), EngineError> {
    match mode {
        InputMode::Trusted => Ok(()),
        InputMode::Synthetic => Err(EngineError::new(EngineErrorKind::Unsupported)),
    }
}

async fn within_step<T>(
    step: impl Future<Output = Result<T, EngineError>>,
) -> Result<T, EngineError> {
    tokio::time::timeout(EVAL_STEP, step)
        .await
        .unwrap_or(Err(EngineError::new(EngineErrorKind::Timeout)))
}

impl BrowserEngine for ChromiumEngine {
    fn facts(&self) -> &EngineFacts {
        &self.facts
    }

    fn evaluate<'a>(&'a self, tab: &'a str, script: Script) -> EngineFuture<'a, Value> {
        Box::pin(async move { within_step(cdp::evaluate(&*self.tabs, tab, &script)).await })
    }

    fn evaluate_in_frame<'a>(
        &'a self,
        tab: &'a str,
        frame: &'a str,
        script: Script,
    ) -> EngineFuture<'a, Value> {
        Box::pin(async move {
            within_step(cdp::evaluate_in(&*self.tabs, tab, Some(frame), &script)).await
        })
    }

    fn pointer_click<'a>(
        &'a self,
        tab: &'a str,
        at: Point,
        mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            trusted_only(mode)?;
            cdp::click_at(&*self.tabs, tab, at).await
        })
    }

    fn pointer_move<'a>(
        &'a self,
        tab: &'a str,
        at: Point,
        mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            trusted_only(mode)?;
            cdp_input::move_to(&*self.tabs, tab, at).await
        })
    }

    fn wheel<'a>(
        &'a self,
        tab: &'a str,
        at: Point,
        (dx, dy): (f64, f64),
        mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            trusted_only(mode)?;
            cdp_input::wheel(&*self.tabs, tab, at, dx, dy).await
        })
    }

    fn drag<'a>(
        &'a self,
        tab: &'a str,
        from: Point,
        to: Point,
        mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            trusted_only(mode)?;
            cdp_input::drag(&*self.tabs, tab, from, to).await
        })
    }

    fn key<'a>(
        &'a self,
        tab: &'a str,
        input: KeyInput<'a>,
        mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            trusted_only(mode)?;
            match input {
                KeyInput::Text(text) => cdp_input::insert_text(&*self.tabs, tab, text).await,
                KeyInput::Press(key) => cdp_input::press(&*self.tabs, tab, key).await,
            }
        })
    }

    fn screenshot<'a>(&'a self, tab: &'a str) -> EngineFuture<'a, Vec<u8>> {
        Box::pin(async move { within_step(cdp::capture_screenshot(&*self.tabs, tab)).await })
    }

    fn answer_dialog<'a>(
        &'a self,
        tab: &'a str,
        id: u64,
        answer: DialogAnswer,
    ) -> EngineFuture<'a, bool> {
        Box::pin(self.tabs.answer_dialog(tab, id, answer))
    }

    fn set_input_files<'a>(
        &'a self,
        tab: &'a str,
        frame: Option<&'a str>,
        path: &'a str,
        files: &'a [std::path::PathBuf],
    ) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            within_step(cdp::set_input_files(&*self.tabs, tab, frame, path, files)).await
        })
    }
}

#[cfg(test)]
mod tests;
