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

/// Dispatch an annotation control command to a user browser tab overlay.
#[tauri::command]
pub async fn browser_tab_annotation_cmd(
    tab_id: String,
    command: serde_json::Value,
    webview: Webview,
    browser_manager: State<'_, Arc<BrowserTabManager>>,
) -> Result<IpcResult<()>, String> {
    require_main_webview(&webview, "browser_tab_annotation_cmd")?;
    let json = serde_json::to_string(&command)
        .map_err(|e| format!("Failed to serialize annotation command: {e}"))?;
    match browser_manager.send_annotation_command(&tab_id, &json) {
        Ok(()) => Ok(IpcResult::success(())),
        Err(e) => Ok(IpcResult::error(e, "BROWSER_TAB_ANNOTATION_CMD_FAILED")),
    }
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

/// Probe whether a localhost TCP port is accepting connections on IPv4 or IPv6.
async fn is_local_port_listening(port: u16) -> bool {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::time::Duration;

    let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    if tokio::time::timeout(
        Duration::from_millis(120),
        tokio::net::TcpStream::connect(v4),
    )
    .await
    .is_ok_and(|res| res.is_ok())
    {
        return true;
    }

    let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    tokio::time::timeout(
        Duration::from_millis(120),
        tokio::net::TcpStream::connect(v6),
    )
    .await
    .is_ok_and(|res| res.is_ok())
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
