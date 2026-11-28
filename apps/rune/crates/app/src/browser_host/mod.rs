//! Connects the Rune browser pane (`ui::browser` + `model::browser`) to the server's
//! browser host (`browser_tab_*` `/ws` requests and the `browser_screencast` watch).

mod view;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use client::ws::Command;
use model::browser::{
    BrowserLoadError, BrowserTabView, ControlEvent, ControlState, SessionEvent, SessionState,
    ViewportPreset, normalize_browser_input_url, resolve_viewport_dimensions, step_control,
    step_session,
};
use model::workspace::Event as WorkspaceEvent;
use rune::{Pixels, PointerPress, TextEdit};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsWatchName};
use ui::browser::decode_base64_jpeg;

use crate::State;
use crate::editor_host::epoch_ms;
use crate::sync::Pending;

/// Pending `/ws` calls made by the browser pane host.
#[derive(Clone, Debug, PartialEq)]
pub enum BrowserWsPending {
    Open { tab_id: String },
    Attach { tab_id: String, host_tab_id: String },
    Navigate { tab_id: String },
    History { tab_id: String },
    Clipboard { tab_id: String, action: String },
    Close { host_tab_id: String },
    Resize { tab_id: String },
    Input,
}

/// Live runtime state of one browser pane tab.
pub struct PaneRuntime {
    pub pixels: Pixels,
    pub has_frame: bool,
    pub project_id: String,
    pub session_id: Option<String>,
    pub host_tab_id: Option<String>,
    pub watching: bool,
    pub opening: bool,
    pub last_size: Option<(u32, u32, bool)>,
    pub measured: (f32, f32),
    pub status_note: Option<String>,
    pub url_edit: TextEdit,
    pub url_dirty: bool,
}

impl PaneRuntime {
    pub(super) fn new(project_id: String, url: &str) -> Self {
        Self {
            pixels: Pixels::new(),
            has_frame: false,
            project_id,
            session_id: None,
            host_tab_id: None,
            watching: false,
            opening: false,
            last_size: None,
            measured: (0.0, 0.0),
            status_note: None,
            url_edit: TextEdit::new(url),
            url_dirty: false,
        }
    }
}

#[derive(Default)]
pub struct BrowserUi {
    pub session: SessionState,
    pub control: ControlState,
    pub panes: BTreeMap<String, PaneRuntime>,
    /// Currently focused browser tab id (for IME, keyboard shortcuts, and clipboard).
    pub focused: Option<String>,
    /// Open right-click context menu `(tab_id, pointer_press)`.
    pub menu: Option<(String, PointerPress)>,
    /// Visible browser tab ids captured during the latest `view` pass.
    pub(crate) visible_now: RefCell<BTreeSet<String>>,
    /// Visible browser tab ids as of the last `pump_browsers` sync.
    pub(crate) reported_visible: BTreeSet<String>,
    /// Whether the app was rendered in phone layout on the latest `view` pass.
    pub(crate) phone_now: RefCell<bool>,
    /// Strings queued to write to the system clipboard on the next `pump_browsers(tx)`.
    pub(crate) pending_copy: Vec<String>,
    /// External URLs queued to open in the system browser on the next `pump_browsers(tx)`.
    pub(crate) pending_open_url: Vec<String>,
}

impl BrowserUi {
    /// Finds the local tab id backing a server `host_tab_id`.
    #[must_use]
    pub fn tab_for_host(&self, host_tab_id: &str) -> Option<String> {
        if self.panes.contains_key(host_tab_id) {
            return Some(host_tab_id.to_owned());
        }
        self.panes
            .iter()
            .find(|(_, rt)| rt.host_tab_id.as_deref() == Some(host_tab_id))
            .map(|(id, _)| id.clone())
    }
}

pub(super) fn now_u64() -> u64 {
    epoch_ms().max(0.0) as u64
}

impl State {
    /// Opens a new browser tab in `pane` (or the active pane), optionally navigating to `url`.
    pub fn open_browser_tab(&mut self, pane: Option<String>, url: Option<String>) {
        let Some(project_id) = self.active_project().map(|p| p.id.clone()) else {
            self.toast_error("Browser unavailable", "Select a project first.");
            return;
        };
        let tab_id = format!("browser-{}", self.workspace.ids().next_id());
        let initial_url = url
            .as_deref()
            .and_then(normalize_browser_input_url)
            .unwrap_or_default();
        step_session(
            &mut self.browser.session,
            SessionEvent::CreateTab(tab_id.clone(), initial_url.clone()),
            now_u64(),
        );
        self.browser
            .panes
            .insert(tab_id.clone(), PaneRuntime::new(project_id, &initial_url));
        self.browser.focused = Some(tab_id.clone());
        self.workspace
            .step(WorkspaceEvent::AddBrowserTab(tab_id.clone(), pane));
        log::info!("[browser] opened pane tab {tab_id}");
        if !initial_url.is_empty() {
            self.request_browser_open(&tab_id, &initial_url);
        }
    }

    /// Ensures a restored workspace `Tab::Browser` has a runtime entry in `self.browser`.
    pub(super) fn ensure_browser_tab(&mut self, tab_id: &str) {
        let project_id = self
            .active_project()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        if !self.browser.session.tabs.contains_key(tab_id) {
            step_session(
                &mut self.browser.session,
                SessionEvent::CreateTab(tab_id.to_owned(), String::new()),
                now_u64(),
            );
        }
        self.browser
            .panes
            .entry(tab_id.to_owned())
            .or_insert_with(|| PaneRuntime::new(project_id, ""));
    }

    /// Navigates `tab_id` to `raw_url` (normalizing shorthand inputs like `:3000`).
    pub fn browser_navigate(&mut self, tab_id: &str, raw_url: &str) {
        let Some(url) = normalize_browser_input_url(raw_url) else {
            return;
        };
        self.ensure_browser_tab(tab_id);
        step_session(
            &mut self.browser.session,
            SessionEvent::UpdateUrl(tab_id.to_owned(), url.clone()),
            now_u64(),
        );
        if let Some(rt) = self.browser.panes.get_mut(tab_id) {
            rt.url_edit = TextEdit::new(&url);
            rt.url_dirty = false;
            rt.status_note = None;
        }
        let host_tab = self
            .browser
            .panes
            .get(tab_id)
            .and_then(|rt| rt.host_tab_id.clone());
        match host_tab {
            Some(host_tab_id) => {
                let payload = self.browser_tab_payload(tab_id, &host_tab_id, json!({ "url": url }));
                self.raw_call(
                    "browser_tab_navigate",
                    payload,
                    Pending::Browser(BrowserWsPending::Navigate {
                        tab_id: tab_id.to_owned(),
                    }),
                );
            }
            None => self.request_browser_open(tab_id, &url),
        }
    }

    pub(super) fn request_browser_open(&mut self, tab_id: &str, url: &str) {
        let Some(rt) = self.browser.panes.get_mut(tab_id) else {
            return;
        };
        if rt.opening {
            return;
        }
        rt.opening = true;
        rt.status_note = None;
        let project_id = rt.project_id.clone();
        let session_id = rt.session_id.clone();
        let (w, h) = rt.measured;
        let mut payload = json!({
            "projectId": project_id,
            "url": url,
        });
        if let Some(sid) = session_id {
            payload["sessionId"] = json!(sid);
        }
        if w >= 64.0 && h >= 64.0 {
            payload["width"] = json!(w.round() as u32);
            payload["height"] = json!(h.round() as u32);
        }
        self.raw_call(
            "browser_tab_open",
            payload,
            Pending::Browser(BrowserWsPending::Open {
                tab_id: tab_id.to_owned(),
            }),
        );
    }

    /// Sends a history navigation or reload command (`back` | `forward` | `reload` | `hardReload`).
    pub fn browser_history(&mut self, tab_id: &str, action: &str) {
        let Some(host_tab_id) = self
            .browser
            .panes
            .get(tab_id)
            .and_then(|rt| rt.host_tab_id.clone())
        else {
            if action == "reload" || action == "hardReload" {
                if let Some(url) = self
                    .browser
                    .session
                    .tabs
                    .get(tab_id)
                    .map(|t| t.url.clone())
                    .filter(|u| !u.is_empty())
                {
                    self.request_browser_open(tab_id, &url);
                }
            }
            return;
        };
        let payload = self.browser_tab_payload(tab_id, &host_tab_id, json!({ "action": action }));
        self.raw_call(
            "browser_tab_history",
            payload,
            Pending::Browser(BrowserWsPending::History {
                tab_id: tab_id.to_owned(),
            }),
        );
    }

    /// Runs a page clipboard operation (`copy` | `cut` | `paste` | `select_all`).
    pub fn browser_clipboard(&mut self, tab_id: &str, action: &str, text: Option<String>) {
        let Some(host_tab_id) = self
            .browser
            .panes
            .get(tab_id)
            .and_then(|rt| rt.host_tab_id.clone())
        else {
            return;
        };
        let mut extra = json!({ "action": action });
        if let Some(t) = text {
            extra["text"] = json!(t);
        }
        let payload = self.browser_tab_payload(tab_id, &host_tab_id, extra);
        self.raw_call(
            "browser_tab_clipboard",
            payload,
            Pending::Browser(BrowserWsPending::Clipboard {
                tab_id: tab_id.to_owned(),
                action: action.to_owned(),
            }),
        );
    }

    /// Sends one `PaneInput` JSON payload to `browser_tab_input`.
    pub fn browser_send_input(&mut self, tab_id: &str, event: Value) {
        let Some(host_tab_id) = self
            .browser
            .panes
            .get(tab_id)
            .and_then(|rt| rt.host_tab_id.clone())
        else {
            return;
        };
        let payload = self.browser_tab_payload(tab_id, &host_tab_id, json!({ "event": event }));
        self.raw_call(
            "browser_tab_input",
            payload,
            Pending::Browser(BrowserWsPending::Input),
        );
    }

    /// Builds a pane request payload carrying `tabId`, optional `projectId`, and optional `sessionId`.
    pub(crate) fn browser_tab_payload(
        &self,
        tab_id: &str,
        host_tab_id: &str,
        mut extra: Value,
    ) -> Value {
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("tabId".into(), json!(host_tab_id));
            if let Some(rt) = self.browser.panes.get(tab_id) {
                if !rt.project_id.is_empty() {
                    obj.insert("projectId".into(), json!(rt.project_id));
                }
                if let Some(sid) = &rt.session_id {
                    obj.insert("sessionId".into(), json!(sid));
                }
            }
        }
        extra
    }

    /// Handles a `WatchState` / `WatchDelta` frame on `WsWatchName::BrowserScreencast`.
    pub(crate) fn browser_screencast_frame(&mut self, scope: &str, payload: &Value) {
        let Some(tab_id) = self.browser.tab_for_host(scope) else {
            return;
        };
        if let Some(event) = payload.get("event").and_then(Value::as_str) {
            match event {
                "tab_updated" => {
                    if let Some(tab_val) = payload.get("tab")
                        && let Ok(mut view) = serde_json::from_value::<BrowserTabView>(tab_val.clone())
                    {
                        view.tab_id = tab_id.clone();
                        let updated_url = view.url.clone();
                        step_session(&mut self.browser.session, SessionEvent::ApplyView(view), now_u64());
                        if let Some(rt) = self.browser.panes.get_mut(&tab_id)
                            && !rt.url_dirty
                            && !updated_url.is_empty()
                        {
                            rt.url_edit = TextEdit::new(&updated_url);
                        }
                    }
                }
                "tab_closed" => {
                    log::warn!("[browser] host tab closed or renderer exited for {tab_id}");
                    if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
                        rt.host_tab_id = None;
                        rt.watching = false;
                        rt.has_frame = false;
                        rt.last_size = None;
                    }
                    step_session(
                        &mut self.browser.session,
                        SessionEvent::SetCrashed(tab_id, true),
                        now_u64(),
                    );
                }
                _ => {}
            }
            return;
        }
        let Some(encoded) = payload.get("data").and_then(Value::as_str) else {
            return;
        };
        let Some(frame) = decode_base64_jpeg(encoded) else {
            log::warn!("[browser] failed to decode screencast frame for {tab_id}");
            return;
        };
        if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
            rt.pixels.set(frame.width, frame.height, frame.rgba);
            rt.has_frame = true;
            rt.status_note = None;
        }
    }

    /// Handles `/ws` replies for browser pane requests.
    pub(crate) fn browser_ws_reply(
        &mut self,
        pending: BrowserWsPending,
        result: Result<Option<Value>, WsError>,
    ) {
        match pending {
            BrowserWsPending::Open { tab_id } => {
                if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
                    rt.opening = false;
                }
                match result {
                    Ok(Some(v)) => {
                        let Some(host_tab_id) = v.get("tabId").and_then(Value::as_str).map(str::to_owned) else {
                            log::warn!("[browser] browser_tab_open reply missing tabId");
                            return;
                        };
                        if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
                            rt.host_tab_id = Some(host_tab_id);
                            rt.last_size = None;
                            rt.status_note = None;
                        }
                        if let Some(tab_val) = v.get("tab")
                            && let Ok(mut view) = serde_json::from_value::<BrowserTabView>(tab_val.clone())
                        {
                            view.tab_id = tab_id.clone();
                            step_session(&mut self.browser.session, SessionEvent::ApplyView(view), now_u64());
                        }
                        log::info!("[browser] opened host tab for {tab_id}");
                        self.sync_browser_watch(&tab_id, true);
                        self.maybe_resize_browser(&tab_id);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!("[browser] browser_tab_open failed for {tab_id}: {}", e.code);
                        if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
                            rt.status_note = Some(e.message.clone());
                        }
                        let host = self
                            .browser
                            .session
                            .tabs
                            .get(&tab_id)
                            .map(|t| model::browser::display_host(&t.url))
                            .unwrap_or_default();
                        step_session(
                            &mut self.browser.session,
                            SessionEvent::SetLoadError(
                                tab_id,
                                Some(BrowserLoadError {
                                    code: e.code,
                                    host,
                                    port: None,
                                    message: e.message,
                                }),
                            ),
                            now_u64(),
                        );
                    }
                }
            }
            BrowserWsPending::Attach {
                tab_id,
                host_tab_id,
            } => match result {
                Ok(_) => {
                    log::info!("[browser] re-attached host tab for {tab_id}");
                    if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
                        rt.host_tab_id = Some(host_tab_id);
                        rt.watching = false;
                    }
                    if self.browser.reported_visible.contains(&tab_id) {
                        self.sync_browser_watch(&tab_id, true);
                    }
                }
                Err(e) => {
                    log::warn!("[browser] browser_tab_attach failed for {tab_id}: {}", e.code);
                    if let Some(rt) = self.browser.panes.get_mut(&tab_id) {
                        rt.host_tab_id = None;
                        rt.watching = false;
                        rt.has_frame = false;
                    }
                    step_session(
                        &mut self.browser.session,
                        SessionEvent::SetCrashed(tab_id, true),
                        now_u64(),
                    );
                }
            },
            BrowserWsPending::Navigate { tab_id } => {
                if let Err(e) = result {
                    log::warn!("[browser] browser_tab_navigate failed for {tab_id}: {}", e.code);
                    let host = self
                        .browser
                        .session
                        .tabs
                        .get(&tab_id)
                        .map(|t| model::browser::display_host(&t.url))
                        .unwrap_or_default();
                    step_session(
                        &mut self.browser.session,
                        SessionEvent::SetLoadError(
                            tab_id,
                            Some(BrowserLoadError {
                                code: e.code,
                                host,
                                port: None,
                                message: e.message,
                            }),
                        ),
                        now_u64(),
                    );
                }
            }
            BrowserWsPending::History { tab_id } => match result {
                Ok(Some(v)) => {
                    let back = v.get("canGoBack").and_then(Value::as_bool).unwrap_or(false);
                    let fwd = v.get("canGoForward").and_then(Value::as_bool).unwrap_or(false);
                    step_session(
                        &mut self.browser.session,
                        SessionEvent::SetNavCapabilities(tab_id, back, fwd),
                        now_u64(),
                    );
                }
                Ok(None) => {}
                Err(e) => log::warn!("[browser] browser_tab_history failed: {}", e.code),
            },
            BrowserWsPending::Clipboard { action, .. } => match result {
                Ok(Some(v)) => {
                    if let Some(text) = v.get("text").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                        self.browser.pending_copy.push(text.to_owned());
                    }
                }
                Ok(None) => {}
                Err(e) => log::warn!("[browser] browser_tab_clipboard({action}) failed: {}", e.code),
            },
            BrowserWsPending::Close { .. }
            | BrowserWsPending::Resize { .. }
            | BrowserWsPending::Input => {
                if let Err(e) = result {
                    log::warn!("[browser] pane request failed: {}", e.code);
                }
            }
        }
    }

    /// Handles raw server events relevant to browser tabs (`browser_agent_tab`, `browser_stop_changed`).
    pub(crate) fn browser_raw_event(&mut self, _sid: Option<String>, kind: &str, payload: Value) {
        match kind {
            "browser_agent_tab" => {
                let action = payload.get("action").and_then(Value::as_str).unwrap_or("");
                let tab_id = payload.get("tabId").and_then(Value::as_str).unwrap_or("");
                let session_id = payload.get("sessionId").and_then(Value::as_str).unwrap_or("");
                if tab_id.is_empty() {
                    return;
                }
                match action {
                    "opened" => {
                        if !session_id.is_empty() {
                            step_control(
                                &mut self.browser.control,
                                ControlEvent::TrackAgentTab(tab_id.to_owned(), session_id.to_owned()),
                            );
                        }
                        let project_id = payload
                            .get("projectId")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| self.active_project().map(|p| p.id.clone()))
                            .unwrap_or_default();
                        let url = payload
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        step_session(
                            &mut self.browser.session,
                            SessionEvent::CreateTab(tab_id.to_owned(), url.clone()),
                            now_u64(),
                        );
                        step_session(
                            &mut self.browser.session,
                            SessionEvent::SetAgentControlled(tab_id.to_owned(), true),
                            now_u64(),
                        );
                        let mut rt = PaneRuntime::new(project_id.clone(), &url);
                        rt.session_id = (!session_id.is_empty()).then(|| session_id.to_owned());
                        rt.host_tab_id = Some(tab_id.to_owned());
                        self.browser.panes.insert(tab_id.to_owned(), rt);
                        let attach = self.browser_tab_payload(tab_id, tab_id, json!({}));
                        self.raw_call(
                            "browser_tab_attach",
                            attach,
                            Pending::Browser(BrowserWsPending::Attach {
                                tab_id: tab_id.to_owned(),
                                host_tab_id: tab_id.to_owned(),
                            }),
                        );
                        self.workspace.step(WorkspaceEvent::OpenAgentBrowserTab(
                            tab_id.to_owned(),
                            (!project_id.is_empty()).then_some(project_id),
                        ));
                    }
                    "closed" => {
                        step_control(
                            &mut self.browser.control,
                            ControlEvent::UntrackAgentTab(tab_id.to_owned()),
                        );
                        self.sync_browser_watch(tab_id, false);
                        self.browser.panes.remove(tab_id);
                        step_session(
                            &mut self.browser.session,
                            SessionEvent::RemoveTab(tab_id.to_owned()),
                            now_u64(),
                        );
                        self.workspace
                            .step(WorkspaceEvent::RemoveTab(format!("browser-{tab_id}")));
                    }
                    _ => {}
                }
            }
            "browser_stop_changed" => {
                if let Some(session_id) = payload.get("sessionId").and_then(Value::as_str) {
                    let stopped = payload
                        .get("stopped")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    step_control(
                        &mut self.browser.control,
                        ControlEvent::SetStopped(session_id.to_owned(), stopped),
                    );
                }
            }
            _ => {}
        }
    }

    /// Re-attaches every open browser pane after `/ws` reconnects.
    pub(crate) fn browser_reconnected(&mut self) {
        let tabs: Vec<(String, String)> = self
            .browser
            .panes
            .iter()
            .filter_map(|(id, rt)| rt.host_tab_id.clone().map(|h| (id.clone(), h)))
            .collect();
        for (tab_id, host_tab_id) in tabs {
            let payload = self.browser_tab_payload(&tab_id, &host_tab_id, json!({}));
            self.raw_call(
                "browser_tab_attach",
                payload,
                Pending::Browser(BrowserWsPending::Attach {
                    tab_id,
                    host_tab_id,
                }),
            );
        }
    }

    pub(super) fn sync_browser_watch(&mut self, tab_id: &str, want: bool) {
        let Some(rt) = self.browser.panes.get_mut(tab_id) else {
            return;
        };
        let Some(host_tab_id) = rt.host_tab_id.clone() else {
            return;
        };
        if rt.watching == want {
            return;
        }
        rt.watching = want;
        let cmd = if want {
            Command::Watch {
                watch: WsWatchName::BrowserScreencast,
                scope: host_tab_id,
            }
        } else {
            Command::Unwatch {
                watch: WsWatchName::BrowserScreencast,
                scope: host_tab_id,
            }
        };
        self.send(cmd);
    }

    pub(super) fn maybe_resize_browser(&mut self, tab_id: &str) {
        let phone = *self.browser.phone_now.borrow();
        let Some(rt) = self.browser.panes.get(tab_id) else {
            return;
        };
        let Some(host_tab_id) = rt.host_tab_id.clone() else {
            return;
        };
        let tab = self.browser.session.tabs.get(tab_id);
        let preset = tab.map_or(ViewportPreset::Desktop, |t| t.viewport_preset);
        let (target_w, target_h, mobile) = if let Some(tab) = tab
            && preset != ViewportPreset::Desktop
        {
            let dims = resolve_viewport_dimensions(
                tab.viewport_preset,
                tab.viewport_orientation,
                tab.custom_viewport_width,
            );
            (
                dims.target_width.unwrap_or(390),
                dims.target_height.unwrap_or(844),
                preset == ViewportPreset::Mobile || phone,
            )
        } else {
            let w = rt.measured.0.round() as u32;
            let h = rt.measured.1.round() as u32;
            if w < 64 || h < 64 {
                return;
            }
            (w.min(4096), h.min(4096), phone)
        };
        if rt.last_size == Some((target_w, target_h, mobile)) {
            return;
        }
        if let Some(rt_mut) = self.browser.panes.get_mut(tab_id) {
            rt_mut.last_size = Some((target_w, target_h, mobile));
        }
        let payload = self.browser_tab_payload(
            tab_id,
            &host_tab_id,
            json!({ "width": target_w, "height": target_h, "mobile": mobile }),
        );
        self.raw_call(
            "browser_tab_resize",
            payload,
            Pending::Browser(BrowserWsPending::Resize {
                tab_id: tab_id.to_owned(),
            }),
        );
    }
}

#[cfg(test)]
mod tests;
