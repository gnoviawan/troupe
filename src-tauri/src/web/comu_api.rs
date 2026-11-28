//! Troupe Comu HTTP routes: web twins of the `comu_*` Tauri commands.
//!
//! - `GET  /host/comu/policy/app`
//! - `POST /host/comu/policy/app` `{expectedRevision, op}`
//! - `GET  /host/comu/policy/project?projectId=`
//! - `POST /host/comu/policy/project` `{projectId, expectedRevision, op}`
//! - `GET  /host/comu/permissions`
//! - `POST /host/comu/permissions/request` `{kind}`
//! - `POST /host/comu/stop` `{sessionId}`
//!
//! Every route sits under `/host/`, which is bearer-gated whenever the server
//! runs a token gate. `POST` routes also pass `check_local_only` so view-only
//! shared-live clients cannot mutate Computer Use policy, trigger OS dialogs,
//! or stop sessions, while `FullControl` shared-live clients holding the host
//! token can. Standalone `troupe-server` has neither a policy service nor a
//! `ComuHost`, so every route answers `CAPABILITY_UNAVAILABLE`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::request::Parts;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::comu::policy::{
    ComuAppPolicyView, ComuCallerScope, ComuPolicyRefusal, ComuProjectPolicyView, ComuPolicyService,
};
use crate::comu::{OsPermissionKind, OsPermissionsStatus};
use crate::web::auth::BearerVerified;
use crate::web::capabilities::WriteFlags;
use crate::web::fs_api::{check_local_only, IpcBody};

#[derive(Clone)]
pub struct ComuRouteState {
    pub service: Option<Arc<ComuPolicyService>>,
    pub token_gated: bool,
    pub write_flags: WriteFlags,
}

pub const APP_POLICY_ROUTE: &str = "/host/comu/policy/app";
pub const PROJECT_POLICY_ROUTE: &str = "/host/comu/policy/project";
pub const PERMISSIONS_ROUTE: &str = "/host/comu/permissions";
pub const REQUEST_PERMISSION_ROUTE: &str = "/host/comu/permissions/request";
pub const STOP_ROUTE: &str = "/host/comu/stop";

pub fn comu_router<S>(state: ComuRouteState) -> Router<S> {
    Router::new()
        .route(APP_POLICY_ROUTE, get(get_app_policy).post(apply_app_policy))
        .route(
            PROJECT_POLICY_ROUTE,
            get(get_project_policy).post(apply_project_policy),
        )
        .route(PERMISSIONS_ROUTE, get(get_permissions))
        .route(REQUEST_PERMISSION_ROUTE, post(request_permission))
        .route(STOP_ROUTE, post(stop_session))
        .with_state(state)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComuPermissionKindWire {
    Accessibility,
    ScreenRecording,
}

impl From<ComuPermissionKindWire> for OsPermissionKind {
    fn from(value: ComuPermissionKindWire) -> Self {
        match value {
            ComuPermissionKindWire::Accessibility => Self::Accessibility,
            ComuPermissionKindWire::ScreenRecording => Self::ScreenRecording,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuOsPermissionsView {
    pub platform: String,
    pub accessibility_granted: bool,
    pub screen_recording_granted: bool,
    pub display_available: bool,
    pub ready: bool,
    pub summary: String,
}

impl From<OsPermissionsStatus> for ComuOsPermissionsView {
    fn from(status: OsPermissionsStatus) -> Self {
        Self {
            platform: status.platform,
            accessibility_granted: status.accessibility_granted,
            screen_recording_granted: status.screen_recording_granted,
            display_available: status.display_available,
            ready: status.ready,
            summary: status.summary,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuProjectQuery {
    #[serde(default)]
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuApplyAppBody {
    pub expected_revision: u64,
    #[schemars(with = "serde_json::Map<String, Value>")]
    pub op: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuApplyProjectBody {
    pub project_id: String,
    pub expected_revision: u64,
    #[schemars(with = "serde_json::Map<String, Value>")]
    pub op: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuRequestPermissionBody {
    pub kind: ComuPermissionKindWire,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComuStopBody {
    pub session_id: String,
}

fn body<T>(result: Result<T, ComuPolicyRefusal>) -> Json<IpcBody<T>> {
    Json(match result {
        Ok(data) => IpcBody::ok(data),
        Err(refusal) => IpcBody::err(refusal.envelope_error(), refusal.code().as_str()),
    })
}

fn refuse<T>(refusal: ComuPolicyRefusal) -> Json<IpcBody<T>> {
    body(Err(refusal))
}

fn admit_caller<T>(
    state: &ComuRouteState,
    parts: &Parts,
    peer: SocketAddr,
    write_route: Option<&str>,
) -> Result<(), Json<IpcBody<T>>> {
    let admitted = parts.extensions.get::<BearerVerified>().is_some()
        || (!state.token_gated && peer.ip().is_loopback());
    if !admitted {
        tracing::warn!(
            target: "termul::web::comu",
            peer = %peer,
            "comu request refused: no verified bearer token"
        );
        return Err(Json(IpcBody::err(
            "Computer Use requires the host token",
            "FORBIDDEN",
        )));
    }
    if let Some(route) = write_route {
        if let Some(forbidden) = check_local_only::<T>(
            peer,
            state.write_flags.allow_remote_writes,
            state.write_flags.shared_live_writes_denied,
            route,
        ) {
            return Err(Json(forbidden));
        }
    }
    Ok(())
}

fn policy_service<T>(
    state: &ComuRouteState,
    parts: &Parts,
    peer: SocketAddr,
    write_route: Option<&str>,
) -> Result<Arc<ComuPolicyService>, Json<IpcBody<T>>> {
    admit_caller(state, parts, peer, write_route)?;
    state
        .service
        .clone()
        .ok_or_else(|| refuse(ComuPolicyRefusal::StoreUnavailable))
}

async fn get_app_policy(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
) -> Json<IpcBody<ComuAppPolicyView>> {
    match policy_service(&state, &parts, peer, None) {
        Ok(service) => body(service.app_view(&ComuCallerScope::App).await),
        Err(refused) => refused,
    }
}

async fn apply_app_policy(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    request: Result<Json<ComuApplyAppBody>, JsonRejection>,
) -> Json<IpcBody<ComuAppPolicyView>> {
    let service = match policy_service(&state, &parts, peer, Some(APP_POLICY_ROUTE)) {
        Ok(service) => service,
        Err(refused) => return refused,
    };
    match request {
        Ok(Json(req)) => body(
            service
                .apply_app(&ComuCallerScope::App, req.expected_revision, &req.op)
                .await,
        ),
        Err(_) => refuse(ComuPolicyRefusal::MalformedOperation(
            "invalid JSON request body",
        )),
    }
}

async fn get_project_policy(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    Query(query): Query<ComuProjectQuery>,
) -> Json<IpcBody<ComuProjectPolicyView>> {
    match policy_service(&state, &parts, peer, None) {
        Ok(service) => body(
            service
                .project_view(&ComuCallerScope::App, &query.project_id)
                .await,
        ),
        Err(refused) => refused,
    }
}

async fn apply_project_policy(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    request: Result<Json<ComuApplyProjectBody>, JsonRejection>,
) -> Json<IpcBody<ComuProjectPolicyView>> {
    let service = match policy_service(&state, &parts, peer, Some(PROJECT_POLICY_ROUTE)) {
        Ok(service) => service,
        Err(refused) => return refused,
    };
    match request {
        Ok(Json(req)) => body(
            service
                .apply_project(
                    &ComuCallerScope::App,
                    &req.project_id,
                    req.expected_revision,
                    &req.op,
                )
                .await,
        ),
        Err(_) => refuse(ComuPolicyRefusal::MalformedOperation(
            "invalid JSON request body",
        )),
    }
}

async fn get_permissions(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
) -> Json<IpcBody<ComuOsPermissionsView>> {
    if let Err(refused) = admit_caller(&state, &parts, peer, None) {
        return refused;
    }
    match crate::comu::os_permissions_status() {
        Some(status) => Json(IpcBody::ok(ComuOsPermissionsView::from(status))),
        None => refuse(ComuPolicyRefusal::StoreUnavailable),
    }
}

async fn request_permission(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    request: Result<Json<ComuRequestPermissionBody>, JsonRejection>,
) -> Json<IpcBody<ComuOsPermissionsView>> {
    if let Err(refused) = admit_caller(&state, &parts, peer, Some(REQUEST_PERMISSION_ROUTE)) {
        return refused;
    }
    let Ok(Json(req)) = request else {
        return refuse(ComuPolicyRefusal::MalformedOperation(
            "invalid JSON request body",
        ));
    };
    match crate::comu::request_os_permission(req.kind.into()) {
        Some(status) => Json(IpcBody::ok(ComuOsPermissionsView::from(status))),
        None => refuse(ComuPolicyRefusal::StoreUnavailable),
    }
}

async fn stop_session(
    State(state): State<ComuRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    request: Result<Json<ComuStopBody>, JsonRejection>,
) -> Json<IpcBody<bool>> {
    if let Err(refused) = admit_caller(&state, &parts, peer, Some(STOP_ROUTE)) {
        return refused;
    }
    let Ok(Json(req)) = request else {
        return Json(IpcBody::err("invalid JSON request body", "INVALID_PARAMS"));
    };
    let trimmed = req.session_id.trim();
    if trimmed.is_empty() {
        return Json(IpcBody::err("sessionId must not be empty", "INVALID_PARAMS"));
    }
    match crate::comu::stop_session(trimmed) {
        Some(stopped) => Json(IpcBody::ok(stopped)),
        None => refuse(ComuPolicyRefusal::StoreUnavailable),
    }
}

#[cfg(test)]
#[path = "comu_api_tests.rs"]
mod tests;
