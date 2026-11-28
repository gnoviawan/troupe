//! One running Chromium: its process, its CDP connection, and the routing
//! of CDP events to the tab each one belongs to.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::launch::{self, LaunchSpec};
use super::pipe::{CdpEvent, Connection};
use crate::browser_automation::engine::cdp::CdpMethod;
use crate::browser_automation::engine::EngineError;

/// How long `Browser.close` may take before the process is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Routes {
    by_session: HashMap<String, mpsc::UnboundedSender<CdpEvent>>,
    session_of_target: HashMap<String, String>,
    /// Downloads in flight, by guid, to the tab session that started them.
    session_of_download: HashMap<String, String>,
}

pub struct Browser {
    conn: Connection,
    child: Mutex<Option<Child>>,
    routes: Arc<Mutex<Routes>>,
    /// Where Chromium writes a download (as `<guid>`) before the core's
    /// chosen path receives it.
    staging: PathBuf,
}

impl Browser {
    /// Start a browser and check that it answers on its pipe.
    pub async fn launch(spec: &LaunchSpec) -> Result<Arc<Self>, String> {
        let launched = launch::spawn(spec).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::unbounded_channel();
        let mut child = launched.child;
        let conn = match Connection::start(launched.read, launched.write, tx) {
            Ok(conn) => conn,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("could not start the CDP pipe threads: {e}"));
            }
        };
        let browser = Arc::new(Self {
            conn,
            child: Mutex::new(Some(child)),
            routes: Arc::new(Mutex::new(Routes::default())),
            staging: spec.profile_dir.join("downloads-staging"),
        });
        tokio::spawn(route_events(rx, Arc::downgrade(&browser)));
        // Proves the pipe answers, and turns on the target events that
        // carry page titles.
        browser
            .call(
                CdpMethod::TargetSetDiscoverTargets,
                json!({ "discover": true }),
            )
            .await
            .map_err(|_| "the browser did not answer on its CDP pipe".to_string())?;
        // Every new target starts paused, so a popup is closed before it
        // runs (`route_events`); the rest are resumed.
        browser
            .call(
                CdpMethod::TargetSetAutoAttach,
                json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true }),
            )
            .await
            .map_err(|_| "the browser refused to pause new targets".to_string())?;
        browser
            .hold_downloads(None)
            .await
            .map_err(|_| "the browser refused to hand downloads to the host".to_string())?;
        log::info!("[chromium] browser started");
        Ok(browser)
    }

    /// A browser-level command.
    pub async fn call(&self, method: CdpMethod, params: Value) -> Result<Value, EngineError> {
        self.conn.call(None, method, params).await
    }

    /// A command on an attached tab session.
    pub async fn call_session(
        &self,
        session: &str,
        method: CdpMethod,
        params: Value,
    ) -> Result<Value, EngineError> {
        self.conn.call(Some(session), method, params).await
    }

    /// Have `context` (`None`: the default one) save downloads under the
    /// staging dir by guid and report them, so the core decides each one.
    pub async fn hold_downloads(&self, context: Option<&str>) -> Result<(), EngineError> {
        let mut params = json!({
            "behavior": "allowAndName",
            "downloadPath": self.staging,
            "eventsEnabled": true,
        });
        if let Some(context) = context {
            params["browserContextId"] = json!(context);
        }
        self.call(CdpMethod::BrowserSetDownloadBehavior, params)
            .await
            .map(drop)
    }

    /// The file Chromium writes for download `guid`.
    pub fn staged(&self, guid: &str) -> Option<PathBuf> {
        let safe = !guid.is_empty() && guid.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        safe.then(|| self.staging.join(guid))
    }

    pub fn is_alive(&self) -> bool {
        self.conn.is_open()
    }

    /// Send `session`'s events (and the browser's events about `target`)
    /// to `tx`. The sender drops when the browser exits.
    pub fn route(&self, target: &str, session: &str, tx: mpsc::UnboundedSender<CdpEvent>) {
        let mut routes = self.routes.lock();
        routes.by_session.insert(session.to_string(), tx);
        routes
            .session_of_target
            .insert(target.to_string(), session.to_string());
    }

    pub fn unroute(&self, target: &str, session: &str) {
        let mut routes = self.routes.lock();
        routes.by_session.remove(session);
        routes.session_of_target.remove(target);
        routes.session_of_download.retain(|_, s| s != session);
    }

    pub fn unroute_download(&self, guid: &str) {
        self.routes.lock().session_of_download.remove(guid);
    }

    /// Ask the browser to quit, then kill it if it has not after the grace.
    pub async fn close(&self) {
        let _ =
            tokio::time::timeout(CLOSE_GRACE, self.call(CdpMethod::BrowserClose, json!({}))).await;
        let deadline = tokio::time::Instant::now() + CLOSE_GRACE;
        while tokio::time::Instant::now() < deadline {
            if self.reap() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.kill();
    }

    /// True once the process has exited (and was waited for).
    fn reap(&self) -> bool {
        let mut child = self.child.lock();
        match child.as_mut().map(Child::try_wait) {
            None => true,
            Some(Ok(Some(_))) => {
                child.take();
                true
            }
            Some(_) => false,
        }
    }

    fn kill(&self) {
        if let Some(mut child) = self.child.lock().take() {
            let _ = child.kill();
            let _ = child.wait();
            log::info!("[chromium] browser killed after the close grace");
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.kill();
    }
}

async fn route_events(mut rx: mpsc::UnboundedReceiver<CdpEvent>, browser: Weak<Browser>) {
    let Some(routes) = browser.upgrade().map(|b| b.routes.clone()) else {
        return;
    };
    while let Some(event) = rx.recv().await {
        if event.session.is_none() && event.method == "Target.attachedToTarget" {
            if let Some(browser) = browser.upgrade() {
                tokio::spawn(settle_new_target(browser, event.params));
            }
            continue;
        }
        let session = match &event.session {
            Some(session) => Some(session.clone()),
            None => browser_event_session(&event, &routes.lock()),
        };
        let download = if event.method == "Browser.downloadWillBegin" {
            event
                .params
                .get("guid")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        let Some(session) = session else {
            // A download no tab claims (a child frame's) is never kept.
            if let (Some(guid), Some(browser)) = (download, browser.upgrade()) {
                tokio::spawn(async move {
                    let _ = browser
                        .call(CdpMethod::BrowserCancelDownload, json!({ "guid": guid }))
                        .await;
                });
            }
            continue;
        };
        if let Some(guid) = download {
            // Registered here, before the tab sees the start, so no progress
            // event can outrun it.
            routes
                .lock()
                .session_of_download
                .insert(guid, session.clone());
        }
        let tx = routes.lock().by_session.get(&session).cloned();
        if let Some(tx) = tx {
            let _ = tx.send(event);
        }
    }
    // The pipe closed: every tab's stream ends with it.
    *routes.lock() = Routes::default();
    log::info!("[chromium] browser pipe closed");
}

/// A target the browser just created, paused before it ran. One a page
/// opened (a popup) is closed: its opener tab reports it as a new-window
/// request (`Page.windowOpen`) and the core decides. Any other target (a
/// tab the host made, a worker) is resumed, and the extra session dropped.
async fn settle_new_target(browser: Arc<Browser>, params: Value) {
    let target = params
        .pointer("/targetInfo/targetId")
        .and_then(Value::as_str);
    let session = params.get("sessionId").and_then(Value::as_str);
    let (Some(target), Some(session)) = (target, session) else {
        return;
    };
    // The host's own `Target.attachToTarget` reports here too, unpaused:
    // that session is a tab's and stays.
    if params.get("waitingForDebugger").and_then(Value::as_bool) != Some(true) {
        return;
    }
    if params.pointer("/targetInfo/openerId").is_some() {
        log::info!("[chromium] popup closed before it ran");
        let _ = browser
            .call(CdpMethod::TargetCloseTarget, json!({ "targetId": target }))
            .await;
        return;
    }
    let _ = browser
        .call_session(
            session,
            CdpMethod::RuntimeRunIfWaitingForDebugger,
            json!({}),
        )
        .await;
    let _ = browser
        .call(
            CdpMethod::TargetDetachFromTarget,
            json!({ "sessionId": session }),
        )
        .await;
}

/// The tab session a browser-level event is about.
fn browser_event_session(event: &CdpEvent, routes: &Routes) -> Option<String> {
    let params = &event.params;
    match event.method.as_str() {
        "Target.detachedFromTarget" => params
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string),
        // Chromium names the frame that started a download; a main frame's id
        // is its target's.
        "Browser.downloadWillBegin" => {
            let frame = params.get("frameId").and_then(Value::as_str)?;
            routes.session_of_target.get(frame).cloned()
        }
        "Browser.downloadProgress" => {
            let guid = params.get("guid").and_then(Value::as_str)?;
            routes.session_of_download.get(guid).cloned()
        }
        "Target.targetInfoChanged" | "Target.targetCrashed" | "Target.targetDestroyed" => {
            let target = params
                .pointer("/targetInfo/targetId")
                .or_else(|| params.get("targetId"))
                .and_then(Value::as_str)?;
            routes.session_of_target.get(target).cloned()
        }
        _ => None,
    }
}
