//! The browser pane's `/ws` requests, served by the server's browser host
//! (`troupe-server --browser-host`):
//!
//! - `browser_tab_open {projectId, url, width?, height?}` → `{tabId, tab}`;
//! - `browser_tab_attach {tabId}` → `{}`, to watch a tab this connection did
//!   not open (an agent's);
//! - `browser_tab_close {tabId}`, `browser_tab_navigate {tabId, url}`,
//!   `browser_tab_resize {tabId, width, height, mobile?}`;
//! - `browser_tab_input {tabId, event}`, one pointer, wheel, key, text, IME
//!   or touch event (`browser_host::input::PaneInput`).
//!
//! None of them is on the view allow-list (`capabilities::ws_requirement`),
//! so each needs the write capability: read-only links, shared-live
//! viewers and remote peers without `--allow-remote-writes` are refused
//! before these handlers run. Open and attach also grant the connection the
//! tab's `browser_screencast` watch. Logs carry the request type and the
//! outcome, never a URL or an input event.

use super::*;

#[cfg(unix)]
use crate::browser_host::{self, input::PaneInput, BrowserHost};

const NO_HOST: &str = "capability_unavailable";

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenPayload {
    project_id: String,
    url: String,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TabPayload {
    tab_id: String,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NavigatePayload {
    tab_id: String,
    url: String,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResizePayload {
    tab_id: String,
    width: u32,
    height: u32,
    #[serde(default)]
    mobile: bool,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InputPayload {
    tab_id: String,
    event: PaneInput,
}

/// The largest viewport a pane may ask for, in CSS px.
#[cfg(unix)]
const MAX_VIEWPORT: u32 = 4096;

pub(super) fn is_pane_request(type_: &str) -> bool {
    matches!(
        type_,
        "browser_tab_open"
            | "browser_tab_attach"
            | "browser_tab_close"
            | "browser_tab_navigate"
            | "browser_tab_resize"
            | "browser_tab_input"
    )
}

#[cfg(not(unix))]
pub(super) async fn handle(
    id: String,
    _type: &str,
    _payload: &Value,
    _env: &HandlerEnv,
) -> WsReply {
    WsReply::err_with_code(
        id,
        NO_HOST,
        "the browser host is not available on this OS yet",
    )
}

#[cfg(unix)]
pub(super) async fn handle(id: String, type_: &str, payload: &Value, env: &HandlerEnv) -> WsReply {
    let Some(host) = browser_host::installed() else {
        return WsReply::err_with_code(
            id,
            NO_HOST,
            "this server has no browser host (--browser-host)",
        );
    };
    let reply = match type_ {
        "browser_tab_open" => open(&host, parse(payload), env).await,
        "browser_tab_attach" => attach(&host, parse(payload), env),
        "browser_tab_close" => close(&host, parse(payload)).await,
        "browser_tab_navigate" => navigate(&host, parse(payload)).await,
        "browser_tab_resize" => resize(&host, parse(payload)).await,
        "browser_tab_input" => input(&host, parse(payload)).await,
        _ => Err((
            WsErrorCode::Unsupported,
            "unknown browser pane request".to_string(),
        )),
    };
    match reply {
        Ok(payload) => WsReply::ok(id, Some(payload)),
        Err((code, message)) => {
            log::info!("[ws] {type_} refused: code={}", code.as_str());
            WsReply::err(id, code, message)
        }
    }
}

#[cfg(unix)]
type PaneResult = Result<Value, (WsErrorCode, String)>;

#[cfg(unix)]
fn parse<T: serde::de::DeserializeOwned>(payload: &Value) -> Result<T, (WsErrorCode, String)> {
    // The serde message can quote the payload (a URL, typed text), so it
    // never reaches the client or the log.
    serde_json::from_value(payload.clone()).map_err(|_| {
        (
            WsErrorCode::Unsupported,
            "malformed browser pane payload".to_string(),
        )
    })
}

#[cfg(unix)]
fn known_tab(
    host: &BrowserHost,
    tab_id: &str,
) -> Result<Arc<crate::browser_automation::engine::chromium::Tabs>, (WsErrorCode, String)> {
    let tabs = host.tabs().ok_or((
        WsErrorCode::NotFound,
        "the browser host is shutting down".to_string(),
    ))?;
    tabs.get(tab_id)
        .map_err(|_| (WsErrorCode::NotFound, "tab not found".to_string()))?;
    Ok(tabs)
}

#[cfg(unix)]
async fn open(
    host: &Arc<BrowserHost>,
    payload: Result<OpenPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    if env.registry.canonical_root(&payload.project_id).is_none() {
        return Err((WsErrorCode::NotFound, "project not found".to_string()));
    }
    let label = format!("tab-{}", uuid::Uuid::new_v4());
    let side = |value: Option<f64>, default: f64| {
        value
            .filter(|v| v.is_finite())
            .unwrap_or(default)
            .clamp(1.0, f64::from(MAX_VIEWPORT))
    };
    let bounds = crate::browser_tab_manager::BrowserBounds {
        x: 0.0,
        y: 0.0,
        width: side(payload.width, 1280.0),
        height: side(payload.height, 800.0),
    };
    host.claim_user_tab(&label, &payload.project_id);
    let view = crate::browser_automation::create_tab(&label, &payload.url, bounds)
        .await
        .map_err(|e| {
            (
                WsErrorCode::Unsupported,
                format!("{}: {}", e.code, e.message),
            )
        })?;
    host.grant(&label, &env.out_tx);
    log::info!("[ws] browser_tab_open ok");
    Ok(json!({ "tabId": label, "tab": view }))
}

#[cfg(unix)]
fn attach(
    host: &Arc<BrowserHost>,
    payload: Result<TabPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    known_tab(host, &payload.tab_id)?;
    host.grant(&payload.tab_id, &env.out_tx);
    Ok(json!({}))
}

#[cfg(unix)]
async fn close(
    host: &Arc<BrowserHost>,
    payload: Result<TabPayload, (WsErrorCode, String)>,
) -> PaneResult {
    let payload = payload?;
    known_tab(host, &payload.tab_id)?;
    crate::browser_automation::destroy_tab(&payload.tab_id).await;
    Ok(json!({}))
}

#[cfg(unix)]
async fn navigate(
    host: &Arc<BrowserHost>,
    payload: Result<NavigatePayload, (WsErrorCode, String)>,
) -> PaneResult {
    let payload = payload?;
    let tabs = known_tab(host, &payload.tab_id)?;
    tabs.navigate(&payload.tab_id, &payload.url)
        .await
        .map_err(|e| (WsErrorCode::Unsupported, e))?;
    Ok(json!({}))
}

#[cfg(unix)]
async fn resize(
    host: &Arc<BrowserHost>,
    payload: Result<ResizePayload, (WsErrorCode, String)>,
) -> PaneResult {
    let payload = payload?;
    let tabs = known_tab(host, &payload.tab_id)?;
    let (width, height) = (
        payload.width.min(MAX_VIEWPORT),
        payload.height.min(MAX_VIEWPORT),
    );
    tabs.resize(&payload.tab_id, width, height, payload.mobile)
        .await
        .map_err(|e| {
            (
                WsErrorCode::Unsupported,
                format!("resize failed ({:?})", e.kind),
            )
        })?;
    Ok(json!({}))
}

#[cfg(unix)]
async fn input(
    host: &Arc<BrowserHost>,
    payload: Result<InputPayload, (WsErrorCode, String)>,
) -> PaneResult {
    let payload = payload?;
    let tabs = known_tab(host, &payload.tab_id)?;
    let (method, params) = payload
        .event
        .to_cdp()
        .map_err(|refusal| (WsErrorCode::Unsupported, refusal.message().to_string()))?;
    tabs.input(&payload.tab_id, method, params)
        .await
        .map_err(|e| {
            (
                WsErrorCode::Unsupported,
                format!("input failed ({:?})", e.kind),
            )
        })?;
    Ok(json!({}))
}

/// `watch browser_screencast <tabId>`: only for a connection a pane request
/// granted the tab. Starts the tab's stream once subscribed.
#[cfg(unix)]
pub(super) fn screencast_watch_allowed(
    id: &str,
    scope: &str,
    env: &HandlerEnv,
) -> Result<Arc<BrowserHost>, Box<WsReply>> {
    let Some(host) = browser_host::installed() else {
        return Err(Box::new(WsReply::err_with_code(
            id,
            NO_HOST,
            "this server has no browser host (--browser-host)",
        )));
    };
    if !host.may_watch(scope, &env.out_tx) {
        return Err(Box::new(WsReply::err(
            id,
            WsErrorCode::Forbidden,
            "open or attach the tab before watching it",
        )));
    }
    Ok(host)
}
