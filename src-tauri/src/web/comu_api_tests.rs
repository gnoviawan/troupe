use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use super::{comu_router, ComuRouteState, APP_POLICY_ROUTE, PROJECT_POLICY_ROUTE, STOP_ROUTE};
use crate::comu::policy::{ComuCallerScope, ComuPolicyService};
use crate::web::auth::BearerVerified;
use crate::web::capabilities::WriteFlags;

fn test_router(
    service: Option<Arc<ComuPolicyService>>,
    token_gated: bool,
    shared_live_writes_denied: bool,
) -> Router {
    comu_router::<()>(ComuRouteState {
        service,
        token_gated,
        write_flags: WriteFlags {
            allow_remote_writes: false,
            shared_live_writes_denied,
        },
    })
}

async fn send(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    bearer: bool,
    peer_ip: &str,
) -> Value {
    let mut builder = Request::builder().method(method).uri(uri);
    let bytes = if let Some(json_body) = body {
        builder = builder.header("content-type", "application/json");
        serde_json::to_vec(&json_body).unwrap()
    } else {
        Vec::new()
    };
    let mut req = builder.body(Body::from(bytes)).unwrap();
    let addr: SocketAddr = format!("{peer_ip}:41000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    if bearer {
        req.extensions_mut().insert(BearerVerified);
    }
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let collected = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&collected).unwrap()
}

#[tokio::test]
async fn comu_routes_match_service_views_and_enforce_token_and_write_guards() {
    let dir = TempDir::new().unwrap();
    let service = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));

    // 1. Token-gated host refuses unauthenticated read.
    let unauth = send(
        test_router(Some(service.clone()), true, false),
        Method::GET,
        APP_POLICY_ROUTE,
        None,
        false,
        "127.0.0.1",
    )
    .await;
    assert_eq!(unauth["success"], false);
    assert_eq!(unauth["code"], "FORBIDDEN");

    // 2. View-only shared-live allows bearer GET, denies bearer POST.
    let view_read = send(
        test_router(Some(service.clone()), true, true),
        Method::GET,
        APP_POLICY_ROUTE,
        None,
        true,
        "127.0.0.1",
    )
    .await;
    assert_eq!(view_read["success"], true);
    assert_eq!(view_read["data"]["enabled"], true);

    let view_write = send(
        test_router(Some(service.clone()), true, true),
        Method::POST,
        APP_POLICY_ROUTE,
        Some(json!({ "expectedRevision": 0, "op": { "op": "set_enabled", "enabled": false } })),
        true,
        "127.0.0.1",
    )
    .await;
    assert_eq!(view_write["success"], false);
    assert_eq!(view_write["code"], "FORBIDDEN");

    // 3. FullControl shared-live admits bearer POST and matches Tauri command view.
    let applied = send(
        test_router(Some(service.clone()), true, false),
        Method::POST,
        PROJECT_POLICY_ROUTE,
        Some(json!({
            "projectId": "proj-1",
            "expectedRevision": 0,
            "op": { "op": "add_app", "app": "com.apple.TextEdit" }
        })),
        true,
        "127.0.0.1",
    )
    .await;
    assert_eq!(applied["success"], true);
    let direct = service
        .project_view(&ComuCallerScope::App, "proj-1")
        .await
        .unwrap();
    assert_eq!(applied["data"], serde_json::to_value(direct).unwrap());

    // 4. Headless server without policy/host answers CAPABILITY_UNAVAILABLE.
    let headless_stop = send(
        test_router(None, true, false),
        Method::POST,
        STOP_ROUTE,
        Some(json!({ "sessionId": "s1" })),
        true,
        "127.0.0.1",
    )
    .await;
    assert_eq!(headless_stop["success"], false);
    assert_eq!(headless_stop["code"], "CAPABILITY_UNAVAILABLE");
}
