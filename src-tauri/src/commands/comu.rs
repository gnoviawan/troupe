//! Tauri desktop commands for Troupe Comu policy, OS permissions, and Stop.
//!
//! Thin shells over [`ComuPolicyService`] and [`crate::comu`]; the web routes
//! in `web/comu_api.rs` call the exact same service methods so desktop and
//! remote clients observe identical envelopes.
//!
//! Restricted to the `main` webview so child browser webviews can never inspect
//! or alter Computer Use policy, trigger OS TCC dialogs, or toggle Stop.

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Manager, Webview};

use super::IpcResult;
use crate::comu::policy::{
    ComuAppPolicyView, ComuCallerScope, ComuPolicyRefusal, ComuPolicyService, ComuProjectPolicyView,
};
use crate::web::comu_api::{ComuOsPermissionsView, ComuPermissionKindWire};

fn reply<T>(result: Result<T, ComuPolicyRefusal>) -> IpcResult<T> {
    match result {
        Ok(data) => IpcResult::success(data),
        Err(refusal) => IpcResult::error(refusal.envelope_error(), refusal.code().as_str()),
    }
}

fn require_main_webview(webview: &Webview) -> Result<(), ComuPolicyRefusal> {
    if webview.label() != "main" {
        log::warn!(
            "[Security] comu command rejected from non-main webview '{}'",
            webview.label()
        );
        return Err(ComuPolicyRefusal::AppScopeRequired);
    }
    Ok(())
}

fn service(app: &AppHandle, webview: &Webview) -> Result<Arc<ComuPolicyService>, ComuPolicyRefusal> {
    require_main_webview(webview)?;
    app.try_state::<Arc<ComuPolicyService>>()
        .map(|state| state.inner().clone())
        .ok_or(ComuPolicyRefusal::StoreUnavailable)
}

#[tauri::command]
pub async fn comu_policy_get_app(
    app: AppHandle,
    webview: Webview,
) -> IpcResult<ComuAppPolicyView> {
    match service(&app, &webview) {
        Ok(service) => reply(service.app_view(&ComuCallerScope::App).await),
        Err(refusal) => reply(Err(refusal)),
    }
}

#[tauri::command]
pub async fn comu_policy_apply_app(
    expected_revision: u64,
    op: Value,
    app: AppHandle,
    webview: Webview,
) -> IpcResult<ComuAppPolicyView> {
    match service(&app, &webview) {
        Ok(service) => reply(
            service
                .apply_app(&ComuCallerScope::App, expected_revision, &op)
                .await,
        ),
        Err(refusal) => reply(Err(refusal)),
    }
}

#[tauri::command]
pub async fn comu_policy_get_project(
    project_id: String,
    app: AppHandle,
    webview: Webview,
) -> IpcResult<ComuProjectPolicyView> {
    match service(&app, &webview) {
        Ok(service) => reply(service.project_view(&ComuCallerScope::App, &project_id).await),
        Err(refusal) => reply(Err(refusal)),
    }
}

#[tauri::command]
pub async fn comu_policy_apply_project(
    project_id: String,
    expected_revision: u64,
    op: Value,
    app: AppHandle,
    webview: Webview,
) -> IpcResult<ComuProjectPolicyView> {
    match service(&app, &webview) {
        Ok(service) => reply(
            service
                .apply_project(&ComuCallerScope::App, &project_id, expected_revision, &op)
                .await,
        ),
        Err(refusal) => reply(Err(refusal)),
    }
}

#[tauri::command]
pub async fn comu_permissions_status(webview: Webview) -> IpcResult<ComuOsPermissionsView> {
    if let Err(refusal) = require_main_webview(&webview) {
        return reply(Err(refusal));
    }
    match crate::comu::os_permissions_status() {
        Some(status) => IpcResult::success(ComuOsPermissionsView::from(status)),
        None => reply(Err(ComuPolicyRefusal::StoreUnavailable)),
    }
}

#[tauri::command]
pub async fn comu_request_permission(
    kind: ComuPermissionKindWire,
    webview: Webview,
) -> IpcResult<ComuOsPermissionsView> {
    if let Err(refusal) = require_main_webview(&webview) {
        return reply(Err(refusal));
    }
    match crate::comu::request_os_permission(kind.into()) {
        Some(status) => IpcResult::success(ComuOsPermissionsView::from(status)),
        None => reply(Err(ComuPolicyRefusal::StoreUnavailable)),
    }
}

#[tauri::command]
pub async fn comu_stop(session_id: String, webview: Webview) -> IpcResult<bool> {
    if let Err(refusal) = require_main_webview(&webview) {
        return reply(Err(refusal));
    }
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return IpcResult::error("sessionId must not be empty", "INVALID_PARAMS");
    }
    match crate::comu::stop_session(trimmed) {
        Some(stopped) => IpcResult::success(stopped),
        None => reply(Err(ComuPolicyRefusal::StoreUnavailable)),
    }
}
