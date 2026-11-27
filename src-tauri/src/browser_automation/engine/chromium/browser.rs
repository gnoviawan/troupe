//! One running Chromium: its process, its CDP connection, and the routing
//! of CDP events to the tab each one belongs to.

use std::collections::HashMap;
use std::process::Child;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::launch::{self, LaunchSpec};
use super::pipe::{CdpEvent, Connection};
use crate::browser_automation::engine::cdp::CdpMethod;
use crate::browser_automation::engine::EngineError;

/// How long `Browser.close` may take before the process is killed.
#[allow(dead_code, reason = "used by close")]
const CLOSE_GRACE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Routes {
    by_session: HashMap<String, mpsc::UnboundedSender<CdpEvent>>,
    session_of_target: HashMap<String, String>,
}

pub struct Browser {
    conn: Connection,
    child: Mutex<Option<Child>>,
    routes: Arc<Mutex<Routes>>,
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
        });
        tokio::spawn(route_events(rx, browser.routes.clone()));
        // Proves the pipe answers, and turns on the target events that
        // carry page titles.
        browser
            .call(
                CdpMethod::TargetSetDiscoverTargets,
                json!({ "discover": true }),
            )
            .await
            .map_err(|_| "the browser did not answer on its CDP pipe".to_string())?;
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

async fn route_events(mut rx: mpsc::UnboundedReceiver<CdpEvent>, routes: Arc<Mutex<Routes>>) {
    while let Some(event) = rx.recv().await {
        let session = match &event.session {
            Some(session) => Some(session.clone()),
            None => browser_event_session(&event, &routes.lock()),
        };
        let Some(session) = session else {
            continue;
        };
        let tx = routes.lock().by_session.get(&session).cloned();
        if let Some(tx) = tx {
            let _ = tx.send(event);
        }
    }
    // The pipe closed: every tab's stream ends with it.
    *routes.lock() = Routes::default();
    log::info!("[chromium] browser pipe closed");
}

/// The tab session a browser-level event is about.
fn browser_event_session(event: &CdpEvent, routes: &Routes) -> Option<String> {
    let params = &event.params;
    match event.method.as_str() {
        "Target.detachedFromTarget" => params
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string),
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
