//! Browser session store (`src/renderer/stores/browser-session-store.ts`):
//! per-tab URL, title, loading, history capabilities, zoom, responsive viewport
//! preset/orientation/custom width, load errors, crash state, in-page find, and
//! page dialogs/file choosers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{display_host, same_origin_and_path, split_display_url};

pub const MIN_BROWSER_ZOOM: f64 = 0.5;
pub const MAX_BROWSER_ZOOM: f64 = 2.0;
pub const ZOOM_STEPS: [f64; 9] = [0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
const MAX_RECENT_URLS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewportPreset {
    #[default]
    Desktop,
    Tablet,
    Mobile,
}

impl ViewportPreset {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Desktop => Self::Tablet,
            Self::Tablet => Self::Mobile,
            Self::Mobile => Self::Desktop,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Desktop => "Desktop",
            Self::Tablet => "Tablet",
            Self::Mobile => "Mobile",
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Tablet => "tablet",
            Self::Mobile => "mobile",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewportOrientation {
    #[default]
    Portrait,
    Landscape,
}

impl ViewportOrientation {
    #[must_use]
    pub fn toggle(self) -> Self {
        match self {
            Self::Portrait => Self::Landscape,
            Self::Landscape => Self::Portrait,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportDimensions {
    pub target_width: Option<u32>,
    pub target_height: Option<u32>,
}

#[must_use]
pub fn resolve_viewport_dimensions(
    preset: ViewportPreset,
    orientation: ViewportOrientation,
    custom_width: Option<u32>,
) -> ViewportDimensions {
    let (base_w, base_h) = match preset {
        ViewportPreset::Desktop => {
            return ViewportDimensions {
                target_width: None,
                target_height: None,
            };
        }
        ViewportPreset::Tablet => (768, 1024),
        ViewportPreset::Mobile => (390, 844),
    };
    let (w, h) = match orientation {
        ViewportOrientation::Portrait => (base_w, base_h),
        ViewportOrientation::Landscape => (base_h, base_w),
    };
    ViewportDimensions {
        target_width: Some(custom_width.unwrap_or(w)),
        target_height: Some(h),
    }
}

#[must_use]
pub fn clamp_browser_zoom(factor: f64) -> f64 {
    if !factor.is_finite() {
        return 1.0;
    }
    (factor.clamp(MIN_BROWSER_ZOOM, MAX_BROWSER_ZOOM) * 100.0).round() / 100.0
}

#[must_use]
pub fn next_zoom_step(current: f64, zoom_in: bool) -> f64 {
    let c = clamp_browser_zoom(current);
    if zoom_in {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|&step| step > c + 0.005)
            .unwrap_or(MAX_BROWSER_ZOOM)
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|&step| step < c - 0.005)
            .unwrap_or(MIN_BROWSER_ZOOM)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLoadError {
    pub code: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentBrowserUrl {
    pub url: String,
    pub host: String,
    pub path: String,
    pub visited_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabView {
    pub tab_id: String,
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub agent_controlled: bool,
    #[serde(default)]
    pub epoch: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserDialogKind {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDialog {
    pub kind: BrowserDialogKind,
    pub message: String,
    #[serde(default)]
    pub default_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFileChooser {
    pub selector: String,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub accept: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_controlled: Option<bool>,
    pub zoom_factor: f64,
    pub viewport_preset: ViewportPreset,
    pub viewport_orientation: ViewportOrientation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_viewport_width: Option<u32>,
    pub load_error: Option<BrowserLoadError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
    /// True after the remote Chromium renderer exited or crashed while the tab remained open.
    #[serde(default, skip_serializing_if = "is_false")]
    pub crashed: bool,
    /// Pending JS dialog (alert/confirm/prompt/beforeunload) shown over the pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<BrowserDialog>,
    /// Pending file chooser shown over the pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_chooser: Option<BrowserFileChooser>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl BrowserTab {
    #[must_use]
    pub fn new(id: String, url: String) -> Self {
        let loading = !url.is_empty();
        Self {
            id,
            url,
            title: String::new(),
            loading,
            can_go_back: false,
            can_go_forward: false,
            agent_controlled: None,
            zoom_factor: 1.0,
            viewport_preset: ViewportPreset::Desktop,
            viewport_orientation: ViewportOrientation::Portrait,
            custom_viewport_width: None,
            load_error: None,
            previous_url: None,
            stop_generation: None,
            epoch: None,
            crashed: false,
            dialog: None,
            file_chooser: None,
        }
    }

    /// Label for the workspace tab bar: page title, else host, else "New Tab".
    #[must_use]
    pub fn display_title(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        if !self.url.is_empty() {
            return display_host(&self.url);
        }
        "New Tab".to_owned()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub tabs: BTreeMap<String, BrowserTab>,
    pub recent_urls: Vec<RecentBrowserUrl>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionEvent {
    CreateTab(String, #[serde(default)] String),
    RemoveTab(String),
    UpdateUrl(String, String),
    ApplyView(BrowserTabView),
    SetLoading(String, bool),
    SetNavCapabilities(String, bool, bool),
    SetAgentControlled(String, bool),
    SetZoomFactor(String, f64),
    StepZoom(String, String),
    ResetZoom(String),
    SetViewportPreset(String, ViewportPreset),
    CycleViewportPreset(String),
    ToggleViewportOrientation(String),
    SetCustomViewportWidth(String, Option<u32>),
    SetLoadError(String, Option<BrowserLoadError>),
    BumpStopGeneration(String),
    SetCrashed(String, bool),
    ClearRecentUrls,
}

fn record_recent(list: &mut Vec<RecentBrowserUrl>, url: &str, now_ms: u64) {
    let Some(parts) = split_display_url(url) else {
        return;
    };
    if parts.scheme != "http://" && parts.scheme != "https://" {
        return;
    }
    let path_without_hash = parts.path.split('#').next().unwrap_or("");
    let path = if path_without_hash == "/" {
        String::new()
    } else {
        path_without_hash.to_owned()
    };
    list.retain(|entry| entry.url != url);
    list.insert(
        0,
        RecentBrowserUrl {
            url: url.to_owned(),
            host: parts.host,
            path,
            visited_at: now_ms,
        },
    );
    list.truncate(MAX_RECENT_URLS);
}

/// Applies one browser session event and returns the action's return value (if any).
pub fn step_session(state: &mut SessionState, event: SessionEvent, now_ms: u64) -> Option<Value> {
    match event {
        SessionEvent::CreateTab(id, url) => {
            let tab = BrowserTab::new(id.clone(), url.clone());
            if !url.is_empty() {
                record_recent(&mut state.recent_urls, &url, now_ms);
            }
            state.tabs.insert(id, tab.clone());
            serde_json::to_value(tab).ok()
        }
        SessionEvent::RemoveTab(id) => {
            state.tabs.remove(&id);
            None
        }
        SessionEvent::UpdateUrl(id, url) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                if !tab.url.is_empty() && tab.url != url && tab.load_error.is_none() {
                    tab.previous_url = Some(tab.url.clone());
                }
                let _page_changed =
                    !tab.url.is_empty() && !url.is_empty() && !same_origin_and_path(&tab.url, &url);
                tab.url = url.clone();
                tab.load_error = None;
                tab.crashed = false;
                tab.loading = !url.is_empty();
                if !url.is_empty() {
                    record_recent(&mut state.recent_urls, &url, now_ms);
                }
            }
            None
        }
        SessionEvent::ApplyView(view) => {
            if let Some(tab) = state.tabs.get_mut(&view.tab_id) {
                tab.url = view.url.clone();
                tab.title = view.title;
                tab.loading = view.loading;
                tab.agent_controlled = Some(view.agent_controlled);
                tab.epoch = view.epoch;
                tab.crashed = false;
                if view.loading {
                    tab.load_error = None;
                }
                if !view.url.is_empty() {
                    record_recent(&mut state.recent_urls, &view.url, now_ms);
                }
            }
            None
        }
        SessionEvent::SetLoading(id, loading) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.loading = loading;
            }
            None
        }
        SessionEvent::SetNavCapabilities(id, can_go_back, can_go_forward) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.can_go_back = can_go_back;
                tab.can_go_forward = can_go_forward;
            }
            None
        }
        SessionEvent::SetAgentControlled(id, controlled) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.agent_controlled = Some(controlled);
            }
            None
        }
        SessionEvent::SetZoomFactor(id, factor) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.zoom_factor = clamp_browser_zoom(factor);
            }
            None
        }
        SessionEvent::StepZoom(id, direction) => {
            let current = state.tabs.get(&id).map_or(1.0, |t| t.zoom_factor);
            let next = next_zoom_step(current, direction == "in");
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.zoom_factor = next;
            }
            Some(Value::from(next))
        }
        SessionEvent::ResetZoom(id) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.zoom_factor = 1.0;
            }
            None
        }
        SessionEvent::SetViewportPreset(id, preset) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.viewport_preset = preset;
                tab.custom_viewport_width = None;
            }
            None
        }
        SessionEvent::CycleViewportPreset(id) => {
            let current = state
                .tabs
                .get(&id)
                .map_or(ViewportPreset::Desktop, |t| t.viewport_preset);
            let next = current.next();
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.viewport_preset = next;
                tab.custom_viewport_width = None;
            }
            Some(Value::from(next.as_str()))
        }
        SessionEvent::ToggleViewportOrientation(id) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.viewport_orientation = tab.viewport_orientation.toggle();
                tab.custom_viewport_width = None;
            }
            None
        }
        SessionEvent::SetCustomViewportWidth(id, width) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.custom_viewport_width = width;
            }
            None
        }
        SessionEvent::SetLoadError(id, error) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.load_error = error;
                tab.loading = false;
            }
            None
        }
        SessionEvent::BumpStopGeneration(id) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.loading = false;
                tab.stop_generation = Some(tab.stop_generation.unwrap_or(0) + 1);
            }
            None
        }
        SessionEvent::SetCrashed(id, crashed) => {
            if let Some(tab) = state.tabs.get_mut(&id) {
                tab.crashed = crashed;
                if crashed {
                    tab.loading = false;
                }
            }
            None
        }
        SessionEvent::ClearRecentUrls => {
            state.recent_urls.clear();
            None
        }
    }
}
