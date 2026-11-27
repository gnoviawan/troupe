use super::IpcResult;
use crate::browser_tab_manager::{BrowserBounds, BrowserTabInfo, BrowserTabManager};
use std::sync::Arc;
use tauri::{State, Webview};

// ==================== Browser Tab Commands ====================

/// Create a browser tab: the tab registry validates the label (a
/// `tab-<uuid>` the renderer names, or an agent tab the host asked for),
/// builds the webview and admits it once the engine confirmed it.
#[tauri::command]
pub async fn browser_tab_create(
    tab_id: String,
    url: String,
    bounds: BrowserBounds,
) -> Result<IpcResult<BrowserTabInfo>, String> {
    match crate::browser_automation::create_tab(&tab_id, &url, bounds).await {
        Ok(view) => Ok(IpcResult::success(BrowserTabInfo {
            id: view.tab_id,
            url: view.url,
            title: view.title,
            agent_controlled: view.agent_controlled,
        })),
        Err(e) => Ok(IpcResult::error(e.message, e.code)),
    }
}

/// Navigate a browser tab to a new URL
#[tauri::command]
pub async fn browser_tab_navigate(
    tab_id: String,
    url: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.navigate(&tab_id, url) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_NAVIGATE_FAILED")),
    }
}

/// Resize/reposition a browser tab webview
#[tauri::command]
pub async fn browser_tab_resize(
    tab_id: String,
    bounds: BrowserBounds,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.resize(&tab_id, bounds) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_RESIZE_FAILED")),
    }
}

/// Show a browser tab webview
#[tauri::command]
pub async fn browser_tab_show(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.show(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_SHOW_FAILED")),
    }
}

/// Hide a browser tab webview
#[tauri::command]
pub async fn browser_tab_hide(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.hide(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_HIDE_FAILED")),
    }
}

/// Close a browser tab through the registry; resolves once the engine
/// destroyed the webview. Closing an agent tab ends the agent's control.
#[tauri::command]
pub async fn browser_tab_destroy(tab_id: String) -> Result<IpcResult<()>, String> {
    crate::browser_automation::destroy_tab(&tab_id).await;
    Ok(IpcResult::success(()))
}

/// Go back in browser tab history
#[tauri::command]
pub async fn browser_tab_go_back(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.go_back(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_GO_BACK_FAILED")),
    }
}

/// Go forward in browser tab history
#[tauri::command]
pub async fn browser_tab_go_forward(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.go_forward(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_GO_FORWARD_FAILED")),
    }
}

/// Reload a browser tab
#[tauri::command]
pub async fn browser_tab_reload(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.reload(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_RELOAD_FAILED")),
    }
}

/// Open DevTools for a browser tab.
///
/// Debug-gated: the real implementation calls `BrowserTabManager::open_devtools`
/// (which opens the webview inspector). In release builds the command is a
/// stub that returns `Ok(IpcResult::error("DevTools disabled in production",
/// ...))` so the browser-tab devtools path is fully blocked in prod — mirrors
/// the existing `toggle_devtools` cfg-gate pattern in `lib.rs`. P13: the
/// `BrowserTabManager::open_devtools` method only exists in debug builds (no
/// release stub → no dead_code). The TS side also hides the Debug Console
/// button in prod, so a user never reaches the release stub.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn browser_tab_open_devtools(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    match browser_manager.open_devtools(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_OPEN_DEVTOOLS_FAILED")),
    }
}

#[cfg(not(debug_assertions))]
#[tauri::command]
pub async fn browser_tab_open_devtools(
    _tab_id: String,
    _browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    Ok(IpcResult::error(
        "DevTools disabled in production".to_string(),
        "BROWSER_TAB_OPEN_DEVTOOLS_DISABLED",
    ))
}

/// Inject agentation toolbar into a browser tab webview (on-demand).
/// Called from the browser controls UI button.
#[tauri::command]
pub async fn browser_tab_inject_agentation(
    tab_id: String,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    if !browser_manager.is_agentation_enabled() {
        log::info!(
            "[BrowserTab] Agentation injection rejected — feature disabled for tab={}",
            tab_id
        );
        return Ok(IpcResult::error(
            "Agentation is disabled".to_string(),
            "AGENTATION_DISABLED",
        ));
    }
    match browser_manager.inject_agentation_toolbar(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_INJECT_AGENTATION_FAILED")),
    }
}

/// Only the `main` webview answers host consent and presses Stop: browser
/// tabs share the app-wide event channel and could otherwise observe a
/// ticket id and approve themselves.
fn require_main_webview(webview: &Webview, command: &str) -> Result<(), String> {
    if webview.label() == "main" {
        return Ok(());
    }
    log::warn!("[Security] {command} rejected from a non-main webview");
    Err(format!("{command} must come from the main window"))
}

/// Desktop answer to a host-originated browser consent ticket (`host:`
/// request id). `option_id: None` denies.
#[tauri::command]
pub async fn browser_ticket_respond(
    request_id: String,
    option_id: Option<String>,
    webview: Webview,
    tickets: State<'_, Arc<crate::acp::host_ticket::HostTickets>>,
) -> Result<IpcResult<()>, String> {
    use crate::acp::host_ticket::{AnswerError, Approver};
    require_main_webview(&webview, "browser_ticket_respond")?;
    Ok(
        match tickets.answer(Approver::Desktop, &request_id, option_id.as_deref()) {
            Ok(_) => IpcResult::success(()),
            Err(err) => {
                let code = match err {
                    AnswerError::NotFound => "TICKET_NOT_FOUND",
                    AnswerError::AlreadyResolved => "TICKET_ALREADY_RESOLVED",
                    AnswerError::Unqualified => "TICKET_UNQUALIFIED",
                    AnswerError::TooEarly => "TICKET_TOO_EARLY",
                    AnswerError::InvalidOption => "TICKET_INVALID_OPTION",
                };
                IpcResult::error(err.as_str(), code)
            }
        },
    )
}

/// Stop agent browsing for a session until the user's next prompt turn.
#[tauri::command]
pub async fn browser_stop(session_id: String, webview: Webview) -> Result<IpcResult<()>, String> {
    require_main_webview(&webview, "browser_stop")?;
    Ok(match crate::browser_automation::stop_session(&session_id) {
        Some(_) => IpcResult::success(()),
        None => IpcResult::error(
            "browser automation is not running",
            "BROWSER_STOP_UNAVAILABLE",
        ),
    })
}

/// Stop an in-flight navigation load in a browser tab.
#[tauri::command]
pub async fn browser_tab_stop(
    tab_id: String,
    webview: Webview,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    require_main_webview(&webview, "browser_tab_stop")?;
    match browser_manager.stop_loading(&tab_id) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_STOP_FAILED")),
    }
}

/// Set native zoom factor (0.5..=2.0) on a browser tab webview.
#[tauri::command]
pub async fn browser_tab_set_zoom(
    tab_id: String,
    zoom_factor: f64,
    webview: Webview,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<f64>, String> {
    require_main_webview(&webview, "browser_tab_set_zoom")?;
    match browser_manager.set_zoom(&tab_id, zoom_factor) {
        Ok(applied) => Ok(IpcResult::success(applied)),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_SET_ZOOM_FAILED")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserScreenshotPayload {
    pub base64: String,
    pub mime_type: &'static str,
}

/// Capture a native PNG screenshot of a browser tab's visible viewport.
#[tauri::command]
pub async fn browser_tab_screenshot(
    tab_id: String,
    webview: Webview,
) -> Result<IpcResult<BrowserScreenshotPayload>, String> {
    use base64::Engine;
    require_main_webview(&webview, "browser_tab_screenshot")?;
    match crate::browser_automation::capture_tab_screenshot(&tab_id).await {
        Ok(bytes) => {
            log::info!(
                "[BrowserTab] Captured viewport screenshot tab={} bytes={}",
                tab_id,
                bytes.len()
            );
            Ok(IpcResult::success(BrowserScreenshotPayload {
                base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                mime_type: "image/png",
            }))
        }
        Err(e) => {
            log::warn!("[BrowserTab] Screenshot capture failed tab={tab_id}: {e}");
            Ok(IpcResult::error(e, "BROWSER_TAB_SCREENSHOT_FAILED"))
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnnotationOverlayTool {
    Comment,
    Rectangle,
    Ruler,
    ScreenshotArea,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationGuideAxis {
    Horizontal,
    Vertical,
}

/// Typed command sent from the host toolbar to the injected Shadow DOM overlay.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserAnnotationCommand {
    SetActive { active: bool },
    SetTool {
        tool: AnnotationOverlayTool,
        #[serde(skip_serializing_if = "Option::is_none")]
        axis: Option<AnnotationGuideAxis>,
    },
    SetPause { paused: bool },
    SetPinsHidden { hidden: bool },
    SetCaptureHidden { hidden: bool },
    PlaceGuide { axis: AnnotationGuideAxis },
    Clear {
        #[serde(skip_serializing_if = "Option::is_none")]
        ids: Option<Vec<String>>,
    },
}

/// Dispatch a typed annotation control command to a user browser tab overlay.
#[tauri::command]
pub async fn browser_tab_annotation_cmd(
    tab_id: String,
    command: BrowserAnnotationCommand,
    webview: Webview,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    require_main_webview(&webview, "browser_tab_annotation_cmd")?;
    if !browser_manager.is_agentation_enabled() {
        return Ok(IpcResult::error(
            "Annotation overlay is disabled in preferences",
            "AGENTATION_DISABLED",
        ));
    }
    let json = serde_json::to_string(&command)
        .map_err(|e| format!("Failed to serialize annotation command: {e}"))?;
    match browser_manager.send_annotation_command(&tab_id, &json) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_ANNOTATION_CMD_FAILED")),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationSyncAction {
    Delete,
    Resolve,
}

/// Sync host-side annotation deletes/clears/sends back into the Rust SQLite store
/// so MCP tools never serve deleted or already-sent annotations.
#[tauri::command]
pub async fn browser_annotations_sync(
    action: AnnotationSyncAction,
    ids: Vec<String>,
    webview: Webview,
) -> Result<IpcResult<usize>, String> {
    use crate::agentation::types::{AnnotationStatus, AnnotationStore};
    use tauri::Manager;

    require_main_webview(&webview, "browser_annotations_sync")?;
    let Some(service) = webview
        .app_handle()
        .try_state::<crate::agentation::AgentationService>()
    else {
        return Ok(IpcResult::success(0));
    };

    let mut updated = 0usize;
    for id in ids {
        let ok = match action {
            AnnotationSyncAction::Delete => service.store.delete_annotation(&id).is_some(),
            AnnotationSyncAction::Resolve => service
                .store
                .update_annotation_status(&id, AnnotationStatus::Resolved, Some("user"))
                .is_some(),
        };
        if ok {
            updated += 1;
        }
    }
    log::info!("[BrowserTab] Synced {updated} annotation(s) via {action:?}");
    Ok(IpcResult::success(updated))
}

const MAX_CROP_PNG_BYTES: usize = 10 * 1024 * 1024;

/// Persist a cropped annotation PNG under `<app_data_dir>/annotation-crops/`
/// and attach `screenshotCropPath` to the annotation row in SQLite when `annotation_id` is given.
#[tauri::command]
pub async fn browser_save_annotation_crop(
    annotation_id: Option<String>,
    base64_png: String,
    webview: Webview,
) -> Result<IpcResult<String>, String> {
    use base64::Engine;
    use tauri::Manager;

    require_main_webview(&webview, "browser_save_annotation_crop")?;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(&base64_png) {
        Ok(b) => b,
        Err(e) => {
            return Ok(IpcResult::error(
                format!("Invalid base64 PNG: {e}"),
                "INVALID_CROP_PNG",
            ))
        }
    };
    if bytes.is_empty() || bytes.len() > MAX_CROP_PNG_BYTES {
        return Ok(IpcResult::error(
            "Crop PNG payload is empty or exceeds size limit",
            "INVALID_CROP_SIZE",
        ));
    }

    let app_data = match webview.app_handle().path().app_data_dir() {
        Ok(p) => p,
        Err(e) => {
            return Ok(IpcResult::error(
                format!("Could not resolve app_data_dir: {e}"),
                "CROP_DIR_FAILED",
            ))
        }
    };
    let crop_dir = app_data.join("annotation-crops");
    if let Err(e) = std::fs::create_dir_all(&crop_dir) {
        return Ok(IpcResult::error(
            format!("Could not create crop directory: {e}"),
            "CROP_DIR_FAILED",
        ));
    }

    let safe_stem: String = annotation_id
        .as_deref()
        .unwrap_or("crop")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(48)
        .collect();
    let stem = if safe_stem.is_empty() {
        "crop".to_string()
    } else {
        safe_stem
    };
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file_path = crop_dir.join(format!("{stem}-{nonce}.png"));
    if let Err(e) = std::fs::write(&file_path, &bytes) {
        return Ok(IpcResult::error(
            format!("Could not write crop PNG: {e}"),
            "CROP_WRITE_FAILED",
        ));
    }

    let path_str = file_path.to_string_lossy().to_string();
    if let (Some(ann_id), Some(service)) = (
        annotation_id.as_deref(),
        webview
            .app_handle()
            .try_state::<crate::agentation::AgentationService>(),
    ) {
        let _ = service.store.set_annotation_crop_path(ann_id, &path_str);
    }
    log::info!(
        "[BrowserTab] Saved annotation crop bytes={} path={}",
        bytes.len(),
        path_str
    );
    Ok(IpcResult::success(path_str))
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDevServer {
    pub host: String,
    pub port: u16,
    pub url: String,
    pub label: String,
}

const LOCAL_DEV_PORTS: &[(u16, &str)] = &[
    (5173, "Vite dev server"),
    (5174, "Vite dev server"),
    (3000, "Local web server"),
    (3001, "Local web server"),
    (4173, "Vite preview"),
    (4321, "Astro dev server"),
    (8000, "Local HTTP server"),
    (8080, "Local server"),
    (8787, "Workers dev server"),
];

/// Probe whether a localhost TCP port is accepting connections on IPv4 or IPv6 concurrently.
async fn is_local_port_listening(port: u16) -> bool {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::time::Duration;

    if port == 0 {
        return false;
    }

    let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    let timeout = Duration::from_millis(120);

    let f4 = async {
        tokio::time::timeout(timeout, tokio::net::TcpStream::connect(v4))
            .await
            .is_ok_and(|res| res.is_ok())
    };
    let f6 = async {
        tokio::time::timeout(timeout, tokio::net::TcpStream::connect(v6))
            .await
            .is_ok_and(|res| res.is_ok())
    };

    tokio::pin!(f4);
    tokio::pin!(f6);
    tokio::select! {
        ok4 = &mut f4 => if ok4 { true } else { f6.await },
        ok6 = &mut f6 => if ok6 { true } else { f4.await },
    }
}

/// Probe common local dev server ports in parallel with a short timeout.
#[tauri::command]
pub async fn browser_list_local_servers(
    webview: Webview,
) -> Result<IpcResult<Vec<LocalDevServer>>, String> {
    require_main_webview(&webview, "browser_list_local_servers")?;
    let mut handles = Vec::with_capacity(LOCAL_DEV_PORTS.len());
    for &(port, label) in LOCAL_DEV_PORTS {
        handles.push(tokio::spawn(async move {
            is_local_port_listening(port)
                .await
                .then(|| LocalDevServer {
                    host: format!("localhost:{port}"),
                    port,
                    url: format!("http://localhost:{port}"),
                    label: label.to_string(),
                })
        }));
    }
    let mut found = Vec::new();
    for handle in handles {
        if let Ok(Some(server)) = handle.await {
            found.push(server);
        }
    }
    Ok(IpcResult::success(found))
}

/// Probe whether a specific localhost TCP port is accepting connections.
#[tauri::command]
pub async fn browser_check_local_port(
    port: u16,
    webview: Webview,
) -> Result<IpcResult<bool>, String> {
    require_main_webview(&webview, "browser_check_local_port")?;
    Ok(IpcResult::success(is_local_port_listening(port).await))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OverlayOutboxEnvelope {
    req_id: String,
    nonce: String,
    kind: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayOutboxAck {
    #[serde(rename = "type")]
    pub(crate) msg_type: &'static str,
    pub(crate) req_id: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutboxOutcome {
    Ack(OverlayOutboxAck),
    Rejected,
}

pub(crate) fn process_outbox_item(
    store: &crate::agentation::store::SqliteStore,
    tab_id: &str,
    expected_nonce: &str,
    fallback_session_id: Option<&str>,
    raw: serde_json::Value,
) -> OutboxOutcome {
    use crate::agentation::types::{
        AFSEventType, ActionRequest, AnnotationInput, AnnotationStore,
    };
    use subtle::ConstantTimeEq;

    let env: OverlayOutboxEnvelope = match serde_json::from_value(raw) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("[BrowserTab] Dropped malformed overlay outbox item tab={tab_id}: {e}");
            return OutboxOutcome::Rejected;
        }
    };
    if env.req_id.is_empty()
        || expected_nonce.is_empty()
        || !bool::from(env.nonce.as_bytes().ct_eq(expected_nonce.as_bytes()))
    {
        log::warn!(
            "[BrowserTab] Rejected overlay outbox item with invalid nonce tab={tab_id}"
        );
        return OutboxOutcome::Rejected;
    }

    let page_url = if env.url.is_empty() {
        "about:blank".to_string()
    } else {
        env.url
    };
    let requested_sid = env
        .session_id
        .as_deref()
        .filter(|sid| !sid.is_empty())
        .or(fallback_session_id);
    let session_id = match requested_sid {
        Some(sid) if !sid.is_empty() && store.get_session(sid).is_some() => sid.to_string(),
        _ => store.create_session(&page_url, None).id,
    };

    match env.kind.as_str() {
        "annotation" => match serde_json::from_value::<AnnotationInput>(env.payload) {
            Ok(mut input) => {
                input.tab_id = Some(tab_id.to_string());
                if input.url.is_none() {
                    input.url = Some(page_url);
                }
                match store.add_annotation(&session_id, &input) {
                    Some(ann) => {
                        log::info!(
                            "[BrowserTab] Saved overlay annotation via host bridge tab={} id={} kind={:?}",
                            tab_id,
                            ann.id,
                            ann.kind
                        );
                        OutboxOutcome::Ack(OverlayOutboxAck {
                            msg_type: "outbox_ack",
                            req_id: env.req_id,
                            ok: true,
                            id: Some(ann.id),
                            session_id: Some(session_id),
                            error: None,
                        })
                    }
                    None => {
                        log::warn!(
                            "[BrowserTab] Failed to persist overlay annotation via host bridge tab={tab_id}"
                        );
                        OutboxOutcome::Ack(OverlayOutboxAck {
                            msg_type: "outbox_ack",
                            req_id: env.req_id,
                            ok: false,
                            id: None,
                            session_id: Some(session_id),
                            error: Some("Failed to persist annotation".to_string()),
                        })
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "[BrowserTab] Invalid annotation payload in host bridge tab={tab_id}: {e}"
                );
                OutboxOutcome::Ack(OverlayOutboxAck {
                    msg_type: "outbox_ack",
                    req_id: env.req_id,
                    ok: false,
                    id: None,
                    session_id: Some(session_id),
                    error: Some("Invalid annotation payload".to_string()),
                })
            }
        },
        "action" => {
            let output = env
                .payload
                .get("output")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| env.payload.to_string());
            let action_req = ActionRequest {
                session_id: session_id.clone(),
                annotations: Vec::new(),
                output,
                timestamp: chrono::Utc::now().to_rfc3339(),
                tab_id: Some(tab_id.to_string()),
            };
            if let Ok(val) = serde_json::to_value(&action_req) {
                store
                    .event_bus()
                    .emit(AFSEventType::ActionRequested, &session_id, val);
            }
            log::info!("[BrowserTab] Dispatched overlay action via host bridge tab={tab_id}");
            OutboxOutcome::Ack(OverlayOutboxAck {
                msg_type: "outbox_ack",
                req_id: env.req_id,
                ok: true,
                id: None,
                session_id: Some(session_id),
                error: None,
            })
        }
        other => {
            log::warn!(
                "[BrowserTab] Unknown overlay outbox kind '{other}' tab={tab_id}"
            );
            OutboxOutcome::Ack(OverlayOutboxAck {
                msg_type: "outbox_ack",
                req_id: env.req_id,
                ok: false,
                id: None,
                session_id: Some(session_id),
                error: Some(format!("Unknown outbox kind: {other}")),
            })
        }
    }
}

fn should_log_outbox_warn(last_secs: &std::sync::atomic::AtomicU64, interval_secs: u64) -> bool {
    use std::sync::atomic::Ordering;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let prev = last_secs.load(Ordering::Relaxed);
    now.saturating_sub(prev) >= interval_secs
        && last_secs
            .compare_exchange(prev, now, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
}

/// Drain any queued overlay annotation/action messages from a user browser tab's
/// DOM outbox via the isolated engine bridge (immune to page CSP, mixed-content,
/// and HTTPS->HTTP loopback blocks) and persist them into `AgentationService`.
#[tauri::command]
pub async fn browser_tab_poll_annotation_outbox(
    tab_id: String,
    webview: Webview,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<usize>, String> {
    use std::sync::atomic::AtomicU64;
    use tauri::Manager;
    static LAST_MISSING_SVC_WARN: AtomicU64 = AtomicU64::new(0);
    static LAST_DRAIN_ERR_WARN: AtomicU64 = AtomicU64::new(0);

    require_main_webview(&webview, "browser_tab_poll_annotation_outbox")?;
    if !crate::host_perimeter::agentation_allowed(&tab_id)
        || !browser_manager.is_agentation_enabled()
    {
        return Ok(IpcResult::success(0));
    }
    let Some(service) = webview
        .app_handle()
        .try_state::<crate::agentation::AgentationService>()
    else {
        if should_log_outbox_warn(&LAST_MISSING_SVC_WARN, 30) {
            log::warn!("[BrowserTab] AgentationService unavailable while polling outbox tab={tab_id}");
        }
        return Ok(IpcResult::success(0));
    };

    let items = match crate::browser_automation::drain_annotation_outbox(&tab_id).await {
        Ok(items) => items,
        Err(e) => {
            if !e.contains("TabNotFound") && should_log_outbox_warn(&LAST_DRAIN_ERR_WARN, 10) {
                log::warn!("[BrowserTab] Outbox drain failed tab={tab_id}: {e}");
            } else {
                log::debug!("[BrowserTab] Outbox drain skipped tab={tab_id}: {e}");
            }
            return Ok(IpcResult::success(0));
        }
    };
    if items.is_empty() {
        return Ok(IpcResult::success(0));
    }

    let expected_nonce = browser_manager.agentation_bridge_nonce();
    let mut processed = 0usize;
    let mut batch_session_id: Option<String> = None;
    for raw in items {
        if let OutboxOutcome::Ack(ack) = process_outbox_item(
            &service.store,
            &tab_id,
            expected_nonce,
            batch_session_id.as_deref(),
            raw,
        ) {
            if let Some(sid) = ack.session_id.as_ref() {
                batch_session_id = Some(sid.clone());
            }
            processed += 1;
            if let Ok(json) = serde_json::to_string(&ack) {
                if let Err(e) = browser_manager.send_annotation_command(&tab_id, &json) {
                    log::warn!(
                        "[BrowserTab] Failed to send outbox ack tab={tab_id}: {e}"
                    );
                }
            }
        }
    }
    Ok(IpcResult::success(processed))
}
