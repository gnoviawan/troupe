//! Additional write-gated, owner-checked browser pane controls: history
//! navigation, zoom, in-page find, clipboard bridge, script dialog handling,
//! file chooser input, and viewport screenshot capture.
//!
//! Every handler enters through [`super::known_tab`] (or [`super::authorize_tab`]
//! first when validating file paths), so it inherits the write capability gate,
//! principal check, and project/session ownership check. All page scripts run
//! in an isolated world via `CdpMethod::RuntimeCallFunctionOn` and never log
//! page text, queries, URLs, or clipboard contents.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::browser_automation::engine::cdp::{self, CdpMethod, CdpTransport};
use crate::browser_automation::engine::chromium::Tabs;
use crate::browser_host::BrowserHost;

use super::{authorize_tab, known_tab, parse, resolve_owner, HandlerEnv, PaneResult, WsErrorCode};

const MAX_FIND_QUERY: usize = 512;
const MAX_CLIPBOARD_TEXT: usize = 64 * 1024;
const MAX_DIALOG_TEXT: usize = 4096;
const MAX_SELECTOR: usize = 512;
const MAX_CHOSEN_FILES: usize = 32;
const MIN_ZOOM: f64 = 0.5;
const MAX_ZOOM: f64 = 2.0;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct HistoryPayload {
    tab_id: String,
    action: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ZoomPayload {
    tab_id: String,
    factor: f64,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FindPayload {
    tab_id: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    clear: bool,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ClipboardPayload {
    tab_id: String,
    action: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DialogPayload {
    tab_id: String,
    action: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FileChooserPayload {
    tab_id: String,
    selector: String,
    files: Vec<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ScreenshotPayload {
    tab_id: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

pub(super) async fn dispatch(
    host: &Arc<BrowserHost>,
    type_: &str,
    payload: &Value,
    env: &HandlerEnv,
) -> PaneResult {
    match type_ {
        "browser_tab_history" => history(host, parse(payload), env).await,
        "browser_tab_zoom" => zoom(host, parse(payload), env).await,
        "browser_tab_find" => find(host, parse(payload), env).await,
        "browser_tab_clipboard" => clipboard(host, parse(payload), env).await,
        "browser_tab_dialog" => dialog(host, parse(payload), env).await,
        "browser_tab_file_chooser" => file_chooser(host, parse(payload), env).await,
        "browser_tab_screenshot" => screenshot(host, parse(payload), env).await,
        _ => Err((
            WsErrorCode::Unsupported,
            "unknown browser pane request".to_string(),
        )),
    }
}

async fn isolated_context(tabs: &Tabs, tab_id: &str) -> Result<i64, (WsErrorCode, String)> {
    let tree = CdpTransport::call(tabs, tab_id, CdpMethod::PageGetFrameTree, json!({}))
        .await
        .map_err(|e| (WsErrorCode::Unsupported, format!("frame tree failed ({:?})", e.kind)))?;
    let frame_id = cdp::main_frame_id(&tree)
        .ok_or_else(|| (WsErrorCode::Unsupported, "main frame unavailable".to_string()))?
        .to_string();
    let world = CdpTransport::call(
        tabs,
        tab_id,
        CdpMethod::PageCreateIsolatedWorld,
        json!({ "frameId": frame_id, "worldName": "troupe-pane-controls" }),
    )
    .await
    .map_err(|e| (WsErrorCode::Unsupported, format!("isolated world failed ({:?})", e.kind)))?;
    cdp::world_context_id(&world)
        .ok_or_else(|| (WsErrorCode::Unsupported, "isolated context unavailable".to_string()))
}

async fn isolated_eval(
    tabs: &Tabs,
    tab_id: &str,
    function_declaration: &str,
    arg: Value,
) -> Result<Value, (WsErrorCode, String)> {
    let context_id = isolated_context(tabs, tab_id).await?;
    let out = CdpTransport::call(
        tabs,
        tab_id,
        CdpMethod::RuntimeCallFunctionOn,
        json!({
            "functionDeclaration": function_declaration,
            "executionContextId": context_id,
            "arguments": [{ "value": arg }],
            "returnByValue": true,
            "awaitPromise": false,
        }),
    )
    .await
    .map_err(|e| (WsErrorCode::Unsupported, format!("page helper failed ({:?})", e.kind)))?;
    cdp::parse_evaluate_result(&out)
        .map_err(|e| (WsErrorCode::Unsupported, format!("page helper error ({:?})", e.kind)))
}

async fn nav_capabilities(tabs: &Tabs, tab_id: &str) -> (bool, bool) {
    let Ok(history) =
        CdpTransport::call(tabs, tab_id, CdpMethod::PageGetNavigationHistory, json!({})).await
    else {
        return (false, false);
    };
    let idx = history
        .get("currentIndex")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let len = i64::try_from(
        history
            .get("entries")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    )
    .unwrap_or(i64::MAX);
    (idx > 0, idx + 1 < len)
}

async fn history(
    host: &Arc<BrowserHost>,
    payload: Result<HistoryPayload, (WsErrorCode, String)>,
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
    let res = match payload.action.as_str() {
        "back" => tabs.go(&payload.tab_id, -1).await,
        "forward" => tabs.go(&payload.tab_id, 1).await,
        "reload" => tabs.reload(&payload.tab_id).await,
        "hardReload" => CdpTransport::call(
            tabs.as_ref(),
            &payload.tab_id,
            CdpMethod::PageReload,
            json!({ "ignoreCache": true }),
        )
        .await
        .map(|_| ())
        .map_err(|e| format!("hard reload failed ({:?})", e.kind)),
        "query" => Ok(()),
        _ => {
            return Err((
                WsErrorCode::Unsupported,
                "history action must be back, forward, reload, hardReload or query".to_string(),
            ))
        }
    };
    if payload.action != "query" {
        res.map_err(|err| (WsErrorCode::Unsupported, err))?;
    }
    let (can_go_back, can_go_forward) = nav_capabilities(&tabs, &payload.tab_id).await;
    Ok(json!({ "canGoBack": can_go_back, "canGoForward": can_go_forward }))
}

async fn zoom(
    host: &Arc<BrowserHost>,
    payload: Result<ZoomPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    if !payload.factor.is_finite() {
        return Err((
            WsErrorCode::Unsupported,
            "zoom factor must be finite".to_string(),
        ));
    }
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    let factor = (payload.factor.clamp(MIN_ZOOM, MAX_ZOOM) * 100.0).round() / 100.0;
    isolated_eval(
        &tabs,
        &payload.tab_id,
        "function(p) { if (document.documentElement) { document.documentElement.style.zoom = p.factor === 1 ? '' : String(p.factor); } return p.factor; }",
        json!({ "factor": factor }),
    )
    .await?;
    Ok(json!({ "zoomFactor": factor }))
}

async fn find(
    host: &Arc<BrowserHost>,
    payload: Result<FindPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    if payload.query.len() > MAX_FIND_QUERY {
        return Err((
            WsErrorCode::Unsupported,
            "find query is too long".to_string(),
        ));
    }
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    let backward = payload.direction.as_deref() == Some("prev");
    let clear = payload.clear || payload.query.is_empty();
    let result = isolated_eval(
        &tabs,
        &payload.tab_id,
        r#"function(p) {
            const sel = window.getSelection();
            if (p.clear || !p.query) {
                if (sel) sel.removeAllRanges();
                return { current: 0, total: 0 };
            }
            const needle = p.query.toLowerCase();
            const ranges = [];
            const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
            let node;
            while ((node = walker.nextNode())) {
                const text = node.nodeValue || '';
                const lower = text.toLowerCase();
                let pos = 0;
                while ((pos = lower.indexOf(needle, pos)) !== -1) {
                    const r = document.createRange();
                    r.setStart(node, pos);
                    r.setEnd(node, pos + needle.length);
                    ranges.push(r);
                    pos += needle.length;
                    if (ranges.length >= 500) break;
                }
                if (ranges.length >= 500) break;
            }
            if (ranges.length === 0) {
                if (sel) sel.removeAllRanges();
                return { current: 0, total: 0 };
            }
            let idx = 0;
            if (sel && sel.rangeCount > 0) {
                const cur = sel.getRangeAt(0);
                const found = ranges.findIndex((r) =>
                    r.startContainer === cur.startContainer &&
                    r.startOffset === cur.startOffset &&
                    r.endOffset === cur.endOffset
                );
                if (found !== -1) {
                    idx = p.backward
                        ? (found - 1 + ranges.length) % ranges.length
                        : (found + 1) % ranges.length;
                }
            }
            const chosen = ranges[idx];
            if (sel) {
                sel.removeAllRanges();
                sel.addRange(chosen);
            }
            const host = chosen.startContainer.parentElement;
            if (host && typeof host.scrollIntoView === 'function') {
                host.scrollIntoView({ block: 'center', inline: 'nearest' });
            }
            return { current: idx + 1, total: ranges.length };
        }"#,
        json!({ "query": payload.query, "backward": backward, "clear": clear }),
    )
    .await?;
    Ok(result)
}

async fn clipboard(
    host: &Arc<BrowserHost>,
    payload: Result<ClipboardPayload, (WsErrorCode, String)>,
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
    match payload.action.as_str() {
        "copy" | "cut" => {
            let cut = payload.action == "cut";
            let value = isolated_eval(
                &tabs,
                &payload.tab_id,
                r#"function(p) {
                    const active = document.activeElement;
                    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
                        const start = active.selectionStart || 0;
                        const end = active.selectionEnd || 0;
                        const text = active.value.slice(start, end);
                        if (p.cut && end > start && !active.readOnly && !active.disabled) {
                            active.value = active.value.slice(0, start) + active.value.slice(end);
                            active.selectionStart = active.selectionEnd = start;
                            active.dispatchEvent(new Event('input', { bubbles: true }));
                        }
                        return text;
                    }
                    const sel = window.getSelection();
                    const text = sel ? sel.toString() : '';
                    if (p.cut && sel && !sel.isCollapsed) {
                        sel.deleteFromDocument();
                    }
                    return text;
                }"#,
                json!({ "cut": cut }),
            )
            .await?;
            let text = value.as_str().unwrap_or_default().to_string();
            Ok(json!({ "text": text }))
        }
        "paste" => {
            let text = payload.text.unwrap_or_default();
            if text.len() > MAX_CLIPBOARD_TEXT {
                return Err((
                    WsErrorCode::Unsupported,
                    "clipboard text is too long".to_string(),
                ));
            }
            CdpTransport::call(tabs.as_ref(), &payload.tab_id, CdpMethod::InputInsertText, json!({ "text": text }))
                .await
                .map_err(|e| (WsErrorCode::Unsupported, format!("paste failed ({:?})", e.kind)))?;
            Ok(json!({}))
        }
        "select_all" | "selectAll" => {
            isolated_eval(
                &tabs,
                &payload.tab_id,
                r#"function() {
                    const active = document.activeElement;
                    if (active && typeof active.select === 'function') {
                        active.select();
                        return true;
                    }
                    const sel = window.getSelection();
                    if (sel && document.body) {
                        const range = document.createRange();
                        range.selectNodeContents(document.body);
                        sel.removeAllRanges();
                        sel.addRange(range);
                    }
                    return true;
                }"#,
                json!({}),
            )
            .await?;
            Ok(json!({}))
        }
        _ => Err((
            WsErrorCode::Unsupported,
            "clipboard action must be copy, cut, paste or select_all".to_string(),
        )),
    }
}

async fn dialog(
    host: &Arc<BrowserHost>,
    payload: Result<DialogPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    if payload
        .text
        .as_deref()
        .is_some_and(|t| t.len() > MAX_DIALOG_TEXT)
    {
        return Err((
            WsErrorCode::Unsupported,
            "dialog prompt text is too long".to_string(),
        ));
    }
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    let params = match payload.action.as_str() {
        "accept" => json!({ "accept": true, "promptText": payload.text.unwrap_or_default() }),
        "dismiss" => json!({ "accept": false }),
        _ => {
            return Err((
                WsErrorCode::Unsupported,
                "dialog action must be accept or dismiss".to_string(),
            ))
        }
    };
    CdpTransport::call(tabs.as_ref(), &payload.tab_id, CdpMethod::PageHandleJavaScriptDialog, params)
        .await
        .map_err(|e| (WsErrorCode::Unsupported, format!("dialog failed ({:?})", e.kind)))?;
    Ok(json!({}))
}

async fn file_chooser(
    host: &Arc<BrowserHost>,
    payload: Result<FileChooserPayload, (WsErrorCode, String)>,
    env: &HandlerEnv,
) -> PaneResult {
    let payload = payload?;
    if payload.selector.is_empty() || payload.selector.len() > MAX_SELECTOR {
        return Err((
            WsErrorCode::Unsupported,
            "invalid file input selector".to_string(),
        ));
    }
    if payload.files.len() > MAX_CHOSEN_FILES {
        return Err((
            WsErrorCode::Unsupported,
            "too many files selected".to_string(),
        ));
    }
    authorize_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    let owner = resolve_owner(host, &payload.tab_id).ok_or((
        WsErrorCode::Forbidden,
        "tab owner unavailable".to_string(),
    ))?;
    let root = env.registry.canonical_root(&owner.project_id).ok_or((
        WsErrorCode::Forbidden,
        "project root unavailable".to_string(),
    ))?;
    let mut verified: Vec<String> = Vec::with_capacity(payload.files.len());
    for raw in &payload.files {
        let candidate = PathBuf::from(raw);
        let canonical = std::fs::canonicalize(&candidate).map_err(|_| {
            (
                WsErrorCode::Forbidden,
                "chosen file must exist inside the project root".to_string(),
            )
        })?;
        if !canonical.starts_with(&root)
            || !canonical.is_file()
            || crate::web::host_secrets::exposed(&canonical)
        {
            return Err((
                WsErrorCode::Forbidden,
                "chosen file must be inside the project root".to_string(),
            ));
        }
        verified.push(canonical.to_string_lossy().into_owned());
    }
    let tabs = known_tab(
        host,
        &payload.tab_id,
        payload.session_id.as_deref(),
        payload.project_id.as_deref(),
        env,
    )?;
    let context_id = isolated_context(&tabs, &payload.tab_id).await?;
    let out = CdpTransport::call(
        tabs.as_ref(),
        &payload.tab_id,
        CdpMethod::RuntimeCallFunctionOn,
        json!({
            "functionDeclaration": "function(p) { const el = document.querySelector(p.selector); return el && el.tagName === 'INPUT' && el.type === 'file' ? el : null; }",
            "executionContextId": context_id,
            "arguments": [{ "value": { "selector": payload.selector } }],
            "returnByValue": false,
            "awaitPromise": false,
        }),
    )
    .await
    .map_err(|e| (WsErrorCode::Unsupported, format!("file input lookup failed ({:?})", e.kind)))?;
    let object_id = cdp::file_input_object(&out)
        .map_err(|_| (WsErrorCode::NotFound, "file input not found".to_string()))?;
    CdpTransport::call(
        tabs.as_ref(),
        &payload.tab_id,
        CdpMethod::DomSetFileInputFiles,
        json!({ "files": verified, "objectId": object_id }),
    )
    .await
    .map_err(|e| (WsErrorCode::Unsupported, format!("set files failed ({:?})", e.kind)))?;
    Ok(json!({ "count": verified.len() }))
}

async fn screenshot(
    host: &Arc<BrowserHost>,
    payload: Result<ScreenshotPayload, (WsErrorCode, String)>,
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
    let out = CdpTransport::call(
        tabs.as_ref(),
        &payload.tab_id,
        CdpMethod::PageCaptureScreenshot,
        json!({ "format": "png" }),
    )
    .await
    .map_err(|e| (WsErrorCode::Unsupported, format!("screenshot failed ({:?})", e.kind)))?;
    let data = out
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| (WsErrorCode::Unsupported, "screenshot reply had no data".to_string()))?;
    Ok(json!({ "mimeType": "image/png", "base64": data }))
}
