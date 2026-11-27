//! The tab surface of a host with no window: tabs are Chromium page
//! targets the server starts itself (`engine::chromium`), and tab changes
//! go to every `/ws` client instead of a desktop renderer.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Url;

use super::events::TabProjection;
use super::surface::{HistoryStep, SurfaceFuture, TabSurface};
use super::{spawn, BrowserEngine, DesktopBrowserHost, HostServices};
use crate::acp::events as acp_events;
use crate::browser_automation::engine::chromium::Tabs;
use crate::browser_automation::engine::events::TabEvents;
use crate::browser_automation::engine::profile::DataStore;
use crate::browser_automation::engine::Support;
use crate::browser_tab_manager::BrowserBounds;
use crate::web::EventSink;

pub struct HostedSurface {
    pub tabs: Arc<Tabs>,
    pub sinks: Vec<Arc<dyn EventSink>>,
    /// Where screenshots are written.
    pub artifacts: PathBuf,
    /// Root of the per-session directories.
    pub sessions: PathBuf,
}

impl TabSurface for HostedSurface {
    fn create<'a>(
        &'a self,
        label: &'a str,
        url: &'a str,
        bounds: BrowserBounds,
        events: TabEvents,
        store: DataStore,
    ) -> SurfaceFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.tabs
                .open(label, url, (bounds.width, bounds.height), events, &store)
                .await
        })
    }

    fn navigate(&self, tab: &str, url: String) -> Result<(), String> {
        self.tabs
            .get(tab)
            .map_err(|_| "tab not found".to_string())?;
        let (tabs, label) = (self.tabs.clone(), tab.to_string());
        spawn(async move {
            if let Err(e) = tabs.navigate(&label, &url).await {
                log::info!("[browser-tab] hosted navigate did not start: {e}");
            }
        });
        Ok(())
    }

    fn history(&self, tab: &str, step: HistoryStep) -> Result<(), String> {
        self.tabs
            .get(tab)
            .map_err(|_| "tab not found".to_string())?;
        let (tabs, label) = (self.tabs.clone(), tab.to_string());
        spawn(async move {
            let done = match step {
                HistoryStep::Back => tabs.go(&label, -1).await,
                HistoryStep::Forward => tabs.go(&label, 1).await,
                HistoryStep::Reload => tabs.reload(&label).await,
            };
            if let Err(e) = done {
                log::info!("[browser-tab] hosted history step did not start: {e}");
            }
        });
        Ok(())
    }

    fn allows_navigation(&self, _tab: &str, url: &Url) -> bool {
        self.tabs.policy().allows_url(url)
    }

    fn close<'a>(&'a self, tab: &'a str) -> SurfaceFuture<'a, ()> {
        Box::pin(self.tabs.close(tab))
    }

    fn publish(&self, projection: &TabProjection) {
        let name = projection.event_name();
        match projection {
            TabProjection::Updated(view) => acp_events::fan_out(&self.sinks, None, name, view),
            TabProjection::Closed { tab_id } => {
                acp_events::fan_out(
                    &self.sinks,
                    None,
                    name,
                    &serde_json::json!({ "tabId": tab_id }),
                );
            }
        }
    }

    fn user_page_loaded(&self, _tab: &str) {}

    fn artifacts_root(&self) -> Result<PathBuf, String> {
        Ok(self.artifacts.clone())
    }

    fn session_data_root(&self) -> Result<PathBuf, String> {
        Ok(self.sessions.clone())
    }

    fn persistent_profile(&self) -> Support {
        Support::Trusted
    }

    fn wipe_global_profile(&self) -> SurfaceFuture<'_, Result<(), String>> {
        Box::pin(async {
            Err("clearing the agent profile is not available on the server yet".to_string())
        })
    }
}

impl DesktopBrowserHost {
    /// A host over a windowless surface and its engine, for the server.
    pub(crate) fn hosted(
        surface: HostedSurface,
        engine: Arc<dyn BrowserEngine>,
        sinks: Vec<Arc<dyn EventSink>>,
        services: HostServices,
    ) -> Arc<Self> {
        let surface: Arc<dyn TabSurface> = Arc::new(surface);
        super::teardown::spawn_startup_sweep(surface.clone());
        Self::with_parts(surface, engine, sinks, services)
    }
}
