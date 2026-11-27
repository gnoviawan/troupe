//! Tabs of the Chromium engine: opening one as an attached page target,
//! steering it, closing it, and turning its CDP events into the core's
//! [`TabEvents`].

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tauri::Url;
use tokio::sync::{mpsc, oneshot};

use super::browser::Browser;
use super::perimeter::Policy;
use super::pipe::CdpEvent;
use super::BrowserSource;
use crate::browser_automation::budgets::TAB_CONFIRM;
use crate::browser_automation::engine::cdp::{self, CdpMethod};
use crate::browser_automation::engine::events::{DialogAnswer, DialogKind, TabEvent, TabEvents};
use crate::browser_automation::engine::profile::DataStore;
use crate::browser_automation::engine::{EngineError, EngineErrorKind};

/// One open tab: a page target with a flat session attached to it.
pub struct Tab {
    browser: Arc<Browser>,
    target: String,
    session: String,
    /// The disposable context of an ephemeral tab.
    context: Option<String>,
    events: TabEvents,
    /// The script dialog the page holds open, by the id the core knows.
    dialog: Mutex<Option<u64>>,
    /// Fires when the event loop ended and published `Closed`.
    ended: Mutex<Option<oneshot::Receiver<()>>>,
}

impl Tab {
    pub async fn call(&self, method: CdpMethod, params: Value) -> Result<Value, EngineError> {
        self.browser
            .call_session(&self.session, method, params)
            .await
    }

    /// The page's main frame id, which Chromium makes the target id.
    fn is_main_frame(&self, frame: Option<&str>) -> bool {
        frame == Some(self.target.as_str())
    }
}

/// Every tab the engine has open, by pane label.
pub struct Tabs {
    source: Arc<dyn BrowserSource>,
    policy: Policy,
    open: Mutex<HashMap<String, Arc<Tab>>>,
}

impl Tabs {
    pub fn new(source: Arc<dyn BrowserSource>, policy: Policy) -> Self {
        Self {
            source,
            policy,
            open: Mutex::new(HashMap::new()),
        }
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    pub fn get(&self, label: &str) -> Result<Arc<Tab>, EngineError> {
        self.open
            .lock()
            .get(label)
            .cloned()
            .ok_or(EngineError::new(EngineErrorKind::TabNotFound))
    }

    /// Open `label` in the browser that serves `store`, with the engine
    /// hooks publishing into `events`, and start loading `url`.
    pub async fn open(
        self: &Arc<Self>,
        label: &str,
        url: &str,
        size: (f64, f64),
        events: TabEvents,
        store: &DataStore,
    ) -> Result<(), String> {
        let browser = self.source.browser_for(label, store).await?;
        let context = match store {
            DataStore::Ephemeral { .. } => Some(
                browser
                    .call(
                        CdpMethod::TargetCreateBrowserContext,
                        json!({ "disposeOnDetach": true }),
                    )
                    .await
                    .ok()
                    .and_then(|v| {
                        v.get("browserContextId")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .ok_or("the browser did not create an ephemeral context")?,
            ),
            _ => None,
        };
        let mut params = json!({ "url": "about:blank" });
        if let Some(context) = &context {
            params["browserContextId"] = json!(context);
        }
        let target = browser
            .call(CdpMethod::TargetCreateTarget, params)
            .await
            .ok()
            .and_then(|v| {
                v.get("targetId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .ok_or("the browser did not open a tab")?;
        let session = browser
            .call(
                CdpMethod::TargetAttachToTarget,
                json!({ "targetId": target, "flatten": true }),
            )
            .await
            .ok()
            .and_then(|v| {
                v.get("sessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        let Some(session) = session else {
            let _ = browser
                .call(CdpMethod::TargetCloseTarget, json!({ "targetId": target }))
                .await;
            return Err("the browser did not attach to the tab".to_string());
        };
        let (tx, rx) = mpsc::unbounded_channel();
        browser.route(&target, &session, tx);
        let (ended_tx, ended_rx) = oneshot::channel();
        let tab = Arc::new(Tab {
            browser,
            target,
            session,
            context,
            events,
            dialog: Mutex::new(None),
            ended: Mutex::new(Some(ended_rx)),
        });
        self.open.lock().insert(label.to_string(), tab.clone());
        self.source.tab_opened(label);
        tokio::spawn(run_events(
            self.clone(),
            label.to_string(),
            tab.clone(),
            rx,
            ended_tx,
        ));
        if let Err(e) = setup(&tab, size).await {
            self.close(label).await;
            return Err(format!("the tab did not accept its setup ({:?})", e.kind));
        }
        self.navigate(label, url).await
    }

    /// Start loading `url`. A hop the engine's request hook never sees
    /// (`about:`, `data:`) is put to the core's policy here.
    pub async fn navigate(&self, label: &str, url: &str) -> Result<(), String> {
        let tab = self.get(label).map_err(|_| "tab not found".to_string())?;
        let parsed = Url::parse(url).map_err(|_| "not a URL".to_string())?;
        if !matches!(parsed.scheme(), "http" | "https") && !tab.events.nav_starting(&parsed) {
            return Err("navigation target not allowed".to_string());
        }
        tab.call(CdpMethod::PageNavigate, json!({ "url": url }))
            .await
            .map(drop)
            .map_err(|e| format!("navigate failed ({:?})", e.kind))
    }

    /// Back (`-1`) or forward (`1`) in the tab's history.
    pub async fn go(&self, label: &str, delta: i64) -> Result<(), String> {
        let tab = self.get(label).map_err(|_| "tab not found".to_string())?;
        let history = tab
            .call(CdpMethod::PageGetNavigationHistory, json!({}))
            .await
            .map_err(|e| format!("history failed ({:?})", e.kind))?;
        let index = history
            .get("currentIndex")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            + delta;
        let entry = usize::try_from(index)
            .ok()
            .and_then(|i| history.get("entries")?.get(i)?.get("id")?.as_i64())
            .ok_or("no history entry there")?;
        tab.call(
            CdpMethod::PageNavigateToHistoryEntry,
            json!({ "entryId": entry }),
        )
        .await
        .map(drop)
        .map_err(|e| format!("history failed ({:?})", e.kind))
    }

    pub async fn reload(&self, label: &str) -> Result<(), String> {
        let tab = self.get(label).map_err(|_| "tab not found".to_string())?;
        tab.call(CdpMethod::PageReload, json!({}))
            .await
            .map(drop)
            .map_err(|e| format!("reload failed ({:?})", e.kind))
    }

    /// Close the tab and wait until its event stream published `Closed`.
    pub async fn close(&self, label: &str) {
        let Ok(tab) = self.get(label) else {
            return;
        };
        let _ = tab
            .browser
            .call(
                CdpMethod::TargetCloseTarget,
                json!({ "targetId": tab.target }),
            )
            .await;
        let ended = tab.ended.lock().take();
        if let Some(ended) = ended {
            if tokio::time::timeout(TAB_CONFIRM, ended).await.is_err() {
                // The browser never confirmed: end the stream ourselves.
                tab.browser.unroute(&tab.target, &tab.session);
            }
        }
    }

    /// Answer the dialog the tab holds as `id`. False when it holds none.
    pub async fn answer_dialog(
        &self,
        label: &str,
        id: u64,
        answer: DialogAnswer,
    ) -> Result<bool, EngineError> {
        let tab = self.get(label)?;
        if *tab.dialog.lock() != Some(id) {
            return Ok(false);
        }
        let params = match answer {
            DialogAnswer::Accept { text } => {
                json!({ "accept": true, "promptText": text.unwrap_or_default() })
            }
            DialogAnswer::Dismiss => json!({ "accept": false }),
        };
        tab.call(CdpMethod::PageHandleJavaScriptDialog, params)
            .await?;
        tab.dialog.lock().take();
        Ok(true)
    }
}

/// Domains and hooks every tab gets before its first page.
async fn setup(tab: &Tab, (width, height): (f64, f64)) -> Result<(), EngineError> {
    tab.call(CdpMethod::PageEnable, json!({})).await?;
    for (method, params) in cdp::file_chooser_calls() {
        tab.call(method, params).await?;
    }
    tab.call(
        CdpMethod::FetchEnable,
        json!({ "patterns": [{ "urlPattern": "*", "resourceType": "Document", "requestStage": "Request" }] }),
    )
    .await?;
    tab.call(
        CdpMethod::EmulationSetDeviceMetricsOverride,
        json!({
            "width": width.round().max(1.0) as u32,
            "height": height.round().max(1.0) as u32,
            "deviceScaleFactor": 1,
            "mobile": false,
        }),
    )
    .await
    .map(drop)
}

/// Turn the tab's CDP events into core tab events until the tab closes or
/// the browser exits, then publish `Closed` and forget the tab.
async fn run_events(
    tabs: Arc<Tabs>,
    label: String,
    tab: Arc<Tab>,
    mut rx: mpsc::UnboundedReceiver<CdpEvent>,
    ended: oneshot::Sender<()>,
) {
    while let Some(event) = rx.recv().await {
        if !on_event(&tabs, &tab, event) {
            break;
        }
    }
    tab.browser.unroute(&tab.target, &tab.session);
    if let Some(context) = &tab.context {
        let _ = tab
            .browser
            .call(
                CdpMethod::TargetDisposeBrowserContext,
                json!({ "browserContextId": context }),
            )
            .await;
    }
    tabs.open.lock().remove(&label);
    tabs.source.tab_closed(&label);
    tab.events.publish(TabEvent::Closed);
    let _ = ended.send(());
}

/// Apply one event. False ends the tab's stream.
fn on_event(tabs: &Arc<Tabs>, tab: &Arc<Tab>, event: CdpEvent) -> bool {
    let params = &event.params;
    let str_at = |pointer: &str| params.pointer(pointer).and_then(Value::as_str);
    match event.method.as_str() {
        "Fetch.requestPaused" => {
            tokio::spawn(decide_request(tabs.policy, tab.clone(), params.clone()));
        }
        "Page.frameNavigated" => {
            if str_at("/frame/parentId").is_none() {
                let url = str_at("/frame/url").unwrap_or_default();
                let fragment = str_at("/frame/urlFragment").unwrap_or_default();
                tab.events.committed(&format!("{url}{fragment}"));
            }
        }
        "Page.navigatedWithinDocument" => {
            if tab.is_main_frame(str_at("/frameId")) {
                tab.events.same_document(str_at("/url").unwrap_or_default());
            }
        }
        "Page.loadEventFired" => tab.events.publish(TabEvent::LoadFinished),
        "Page.javascriptDialogOpening" => {
            let kind = match str_at("/type") {
                Some("confirm") => DialogKind::Confirm,
                Some("prompt") => DialogKind::Prompt,
                Some("beforeunload") => DialogKind::BeforeUnload,
                _ => DialogKind::Alert,
            };
            let message = str_at("/message").unwrap_or_default().to_string();
            let default_text = str_at("/defaultPrompt").map(str::to_string);
            let id = tab.events.dialog_opened(kind, message, default_text);
            *tab.dialog.lock() = Some(id);
        }
        "Page.javascriptDialogClosed" => {
            tab.dialog.lock().take();
        }
        "Target.targetInfoChanged" => {
            if let Some(title) = str_at("/targetInfo/title") {
                tab.events.publish(TabEvent::TitleChanged {
                    title: title.to_string(),
                });
            }
        }
        "Target.detachedFromTarget"
        | "Target.targetDestroyed"
        | "Target.targetCrashed"
        | "Inspector.detached"
        | "Inspector.targetCrashed" => return false,
        _ => {}
    }
    true
}

/// The hook every document request of the tab passes: the core's verdict
/// for a main-frame hop, or the address policy and the frame policy for a
/// child frame.
async fn decide_request(policy: Policy, tab: Arc<Tab>, params: Value) {
    let Some(request) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let frame = params.get("frameId").and_then(Value::as_str);
    let allowed = params
        .pointer("/request/url")
        .and_then(Value::as_str)
        .and_then(|url| Url::parse(url).ok())
        .is_some_and(|url| {
            // A main-frame hop's verdict includes the address policy
            // (`TabSurface::allows_navigation`) and is published with it.
            if tab.is_main_frame(frame) {
                tab.events.nav_starting(&url)
            } else {
                policy.allows_url(&url) && tab.events.frame_starting(&url)
            }
        });
    let reply = if allowed {
        tab.call(
            CdpMethod::FetchContinueRequest,
            json!({ "requestId": request }),
        )
        .await
    } else {
        // `Aborted` cancels the hop without committing an error page.
        tab.call(
            CdpMethod::FetchFailRequest,
            json!({ "requestId": request, "errorReason": "Aborted" }),
        )
        .await
    };
    if reply.is_err() {
        log::debug!("[chromium] a paused request was already gone");
    }
}
