//! The browser pane's `/ws` requests, served by the server's browser host
//! (`troupe-server --browser-host`):
//!
//! - `browser_tab_open {projectId, sessionId, url, width?, height?}` → `{tabId, tab}`;
//! - `browser_tab_attach {tabId, sessionId?, projectId?}` → `{}`, to attach or
//!   re-attach after reconnect to a tab this chat owns (including an agent's);
//! - `browser_tab_close {tabId, sessionId?, projectId?}`,
//!   `browser_tab_navigate {tabId, url, sessionId?, projectId?}`,
//!   `browser_tab_resize {tabId, width, height, mobile?, sessionId?, projectId?}`;
//! - `browser_tab_input {tabId, event, sessionId?, projectId?}`, one pointer,
//!   wheel, key, text, IME or touch event (`browser_host::input::PaneInput`).
//!
//! None of them is on the view allow-list (`capabilities::ws_requirement`),
//! so each needs the write capability: read-only links, shared-live
//! viewers and remote peers without `--allow-remote-writes` are refused
//! before these handlers run. Each tab stores a stable owner (chat/session
//! id, project id, and authenticated principal when present) so ownership
//! survives WebSocket reconnects while refusing other chats and principals.
//! Open and attach also grant the calling connection the tab's
//! `browser_screencast` watch. Logs carry the request type and the outcome,
//! never a URL or an input event.

use super::*;

#[cfg(unix)]
#[path = "browser_pane_controls.rs"]
mod controls;

#[cfg(unix)]
use crate::browser_host::{self, input::PaneInput, BrowserHost, TabOwner};

const NO_HOST: &str = "capability_unavailable";

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenPayload {
    project_id: String,
    #[serde(default)]
    session_id: Option<String>,
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
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NavigatePayload {
    tab_id: String,
    url: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
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
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InputPayload {
    tab_id: String,
    event: PaneInput,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
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
            | "browser_tab_history"
            | "browser_tab_zoom"
            | "browser_tab_find"
            | "browser_tab_clipboard"
            | "browser_tab_dialog"
            | "browser_tab_file_chooser"
            | "browser_tab_screenshot"
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
    handle_with_host(id, type_, payload, env, &host).await
}

#[cfg(unix)]
async fn handle_with_host(
    id: String,
    type_: &str,
    payload: &Value,
    env: &HandlerEnv,
    host: &Arc<BrowserHost>,
) -> WsReply {
    let reply = match type_ {
        "browser_tab_open" => open(host, parse(payload), env).await,
        "browser_tab_attach" => attach(host, parse(payload), env),
        "browser_tab_close" => close(host, parse(payload), env).await,
        "browser_tab_navigate" => navigate(host, parse(payload), env).await,
        "browser_tab_resize" => resize(host, parse(payload), env).await,
        "browser_tab_input" => input(host, parse(payload), env).await,
        "browser_tab_history"
        | "browser_tab_zoom"
        | "browser_tab_find"
        | "browser_tab_clipboard"
        | "browser_tab_dialog"
        | "browser_tab_file_chooser"
        | "browser_tab_screenshot" => controls::dispatch(host, type_, payload, env).await,
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

/// Server-verified auth class. Client-chosen deviceId is display/routing data,
/// never a credential. All holders of this host's token are one user.
#[cfg(unix)]
fn caller_principal(env: &HandlerEnv) -> Option<String> {
    env.browser_principal.get().cloned()
}

/// The persisted project id of `session_id`, when session persistence knows it.
#[cfg(unix)]
fn persisted_session_project(env: &HandlerEnv, session_id: &str) -> Option<String> {
    env.relay
        .persistence()
        .and_then(|persistence| persistence.metadata(session_id).ok())
        .and_then(|meta| meta.project_id)
}

/// Resolve the stable owner of `tab_id`: user tabs record their owner on
/// `BrowserHost` at open time; live agent tabs resolve through the core
/// `TabRegistry` with the same live-agent check as `core/tabs.rs:595-605`.
#[cfg(unix)]
fn resolve_owner(host: &BrowserHost, tab_id: &str) -> Option<TabOwner> {
    if let Some(owner) = host.tab_owner(tab_id) {
        return Some(owner);
    }
    let (session_id, project_id) = crate::browser_automation::agent_tab_owner(tab_id)?;
    Some(TabOwner::new(
        session_id,
        project_id,
        Some("host-token".into()),
    ))
}

#[cfg(unix)]
fn authorize_tab(
    host: &BrowserHost,
    tab_id: &str,
    claimed_session: Option<&str>,
    claimed_project: Option<&str>,
    env: &HandlerEnv,
) -> Result<(), (WsErrorCode, String)> {
    let Some(owner) = resolve_owner(host, tab_id) else {
        return Err((
            WsErrorCode::Forbidden,
            "tab belongs to another chat".to_string(),
        ));
    };
    if !owner.project_only
        && persisted_session_project(env, &owner.session_id)
            .is_some_and(|persisted_project| persisted_project != owner.project_id)
    {
        return Err((
            WsErrorCode::Forbidden,
            "tab belongs to another project".to_string(),
        ));
    }
    if caller_principal(env).as_deref() == Some("remote-read-only")
        || caller_principal(env).is_none()
    {
        return Err((
            WsErrorCode::Forbidden,
            "browser requires authenticated write access".into(),
        ));
    }
    let subscribed = !owner.project_only && !env.subscriptions.clients_for(&owner.session_id).is_empty();
    let principal = caller_principal(env);
    if !owner.allows(
        subscribed,
        claimed_session,
        claimed_project,
        principal.as_deref(),
    ) {
        return Err((
            WsErrorCode::Forbidden,
            "tab belongs to another chat".to_string(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn known_tab(
    host: &BrowserHost,
    tab_id: &str,
    claimed_session: Option<&str>,
    claimed_project: Option<&str>,
    env: &HandlerEnv,
) -> Result<Arc<crate::browser_automation::engine::chromium::Tabs>, (WsErrorCode, String)> {
    authorize_tab(host, tab_id, claimed_session, claimed_project, env)?;
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
    if !matches!(
        caller_principal(env).as_deref(),
        Some("host-token" | "host-local" | "host-ungated")
    ) {
        return Err((
            WsErrorCode::Forbidden,
            "browser requires authenticated write access".into(),
        ));
    }
    let owner = match payload.session_id {
        Some(session_id) => {
            if session_id.is_empty() {
                return Err((
                    WsErrorCode::Unsupported,
                    "sessionId is required".to_string(),
                ));
            }
            if env.registry.canonical_root(&payload.project_id).is_none() {
                return Err((WsErrorCode::NotFound, "project not found".to_string()));
            }
            if env.subscriptions.clients_for(&session_id).is_empty() {
                return Err((
                    WsErrorCode::Forbidden,
                    "subscribe to the session before opening a browser tab".to_string(),
                ));
            }
            if persisted_session_project(env, &session_id)
                .is_some_and(|persisted_project| persisted_project != payload.project_id)
            {
                return Err((
                    WsErrorCode::Forbidden,
                    "session belongs to another project".to_string(),
                ));
            }
            TabOwner::new(session_id, payload.project_id, caller_principal(env))
        }
        None => {
            if env.registry.canonical_root(&payload.project_id).is_none() {
                return Err((WsErrorCode::NotFound, "project not found".to_string()));
            }
            TabOwner::project_only(payload.project_id, caller_principal(env))
        }
    };
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
    host.claim_user_tab(&label, owner);
    let view = match crate::browser_automation::create_tab(&label, &payload.url, bounds).await {
        Ok(view) => view,
        Err(e) => {
            host.release_tab_claim(&label);
            return Err((
                WsErrorCode::Unsupported,
                format!("{}: {}", e.code, e.message),
            ));
        }
    };
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
    authorize_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    if !host.tab_is_live(&payload.tab_id) {
        return Err((WsErrorCode::NotFound, "tab not found".to_string()));
    }
    host.grant(&payload.tab_id, &env.out_tx);
    Ok(json!({}))
}

#[cfg(unix)]
async fn close(
    host: &Arc<BrowserHost>,
    payload: Result<TabPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    authorize_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    if !host.tab_is_live(&payload.tab_id) {
        return Err((WsErrorCode::NotFound, "tab not found".to_string()));
    }
    crate::browser_automation::destroy_tab(&payload.tab_id).await;
    Ok(json!({}))
}

#[cfg(unix)]
async fn navigate(
    host: &Arc<BrowserHost>,
    payload: Result<NavigatePayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    tabs.navigate(&payload.tab_id, &payload.url)
        .await
        .map_err(|e| (WsErrorCode::Unsupported, e))?;
    Ok(json!({}))
}

#[cfg(unix)]
async fn resize(
    host: &Arc<BrowserHost>,
    payload: Result<ResizePayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
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
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
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

/// `watch browser_screencast <tabId>`: only for a connection a write-gated pane
/// request granted the tab, and still matching the tab's stable owner.
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
    screencast_watch_allowed_with_host(id, scope, env, host)
}

#[cfg(unix)]
fn screencast_watch_allowed_with_host(
    id: &str,
    scope: &str,
    env: &HandlerEnv,
    host: Arc<BrowserHost>,
) -> Result<Arc<BrowserHost>, Box<WsReply>> {
    if !host.may_watch(scope, &env.out_tx) {
        return Err(Box::new(WsReply::err(
            id,
            WsErrorCode::Forbidden,
            "open or attach the tab before watching it",
        )));
    }
    if let Err((code, message)) = authorize_tab(&host, scope, None, None, env) {
        return Err(Box::new(WsReply::err(id, code, message)));
    }
    Ok(host)
}

#[cfg(all(test, unix))]
#[path = "browser_pane_security_tests.rs"]
mod security_tests;
