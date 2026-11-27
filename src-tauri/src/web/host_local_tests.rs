#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The host-local credential: issuing it, the capability it grants (never to
//! a proxied peer), and the file routes refusing it.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request};
use tower::ServiceExt;

use crate::acp::AcpManager;
use crate::web::auth::{WebAuth, WebAuthToken};
use crate::web::capabilities::{
    names_a_proxy, CapabilityPolicy, Carrier, WriteFlags, PROXY_HEADERS,
};
use crate::web::host_local::{issue, HOST_LOCAL_TOKEN_FILE};
use crate::web::project_registry::ProjectRegistry;
use crate::web::router::DesktopLocalParts;
use crate::web::sink::WsRelaySink;
use crate::web::store::{StorePolicy, WebStore};
use crate::web::ws::AppState;

#[test]
fn issue_writes_an_owner_only_credential_the_gate_accepts() {
    let dir = tempfile::tempdir().unwrap();
    let (credential, path) = issue(dir.path()).unwrap();
    assert_eq!(path, dir.path().join(HOST_LOCAL_TOKEN_FILE));
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk.trim(), credential.as_str());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "credential file must be 0600, got {mode:o}");
    }
    let gate = WebAuth::new(WebAuthToken::new("bearer").unwrap());
    assert!(
        !gate.accepts_host_local(on_disk.trim()),
        "no credential installed yet"
    );
    gate.set_host_local_credential(credential);
    assert!(gate.accepts_host_local(on_disk.trim()));
    assert!(!gate.accepts_host_local(""));
    assert!(
        !gate.accepts_host_local("bearer"),
        "the bearer token is not the credential"
    );
    assert!(crate::web::host_secrets::snapshot().covers(&path.canonicalize().unwrap()));
}

#[test]
fn each_boot_replaces_the_previous_credential() {
    let dir = tempfile::tempdir().unwrap();
    let (first, path) = issue(dir.path()).unwrap();
    let (second, _) = issue(dir.path()).unwrap();
    assert_ne!(first.as_str(), second.as_str());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().trim(),
        second.as_str()
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, vec![HOST_LOCAL_TOKEN_FILE.to_string()]);
}

fn policy(peer: [u8; 4], token_gated: bool, proxied: bool) -> CapabilityPolicy {
    CapabilityPolicy {
        carrier: Carrier::Ws,
        peer: SocketAddr::from((peer, 40000)),
        flags: WriteFlags {
            allow_remote_writes: true,
            shared_live_writes_denied: false,
        },
        token_gated,
        proxied,
    }
}

const LOOPBACK: [u8; 4] = [127, 0, 0, 1];

#[test]
fn only_a_direct_gated_loopback_ws_client_with_the_credential_is_host_local() {
    assert!(policy(LOOPBACK, true, false).resolve(true).host_local);

    assert!(
        !policy(LOOPBACK, true, true).resolve(true).host_local,
        "a proxied peer never receives hostLocal"
    );
    assert!(
        !policy(LOOPBACK, true, false).resolve(false).host_local,
        "no credential"
    );
    assert!(
        !policy(LOOPBACK, false, false).resolve(true).host_local,
        "ungated host"
    );
    let lan = policy([192, 168, 1, 9], true, false).resolve(true);
    assert!(lan.write, "--allow-remote-writes admits the LAN peer");
    assert!(!lan.host_local, "but never makes it host-local");

    let mut shared_live = policy(LOOPBACK, true, false);
    shared_live.flags.shared_live_writes_denied = true;
    assert!(!shared_live.resolve(true).host_local);

    let mut terminal = policy(LOOPBACK, true, false);
    terminal.carrier = Carrier::TerminalWs;
    assert!(!terminal.resolve(true).host_local);
}

#[test]
fn every_proxy_header_marks_the_upgrade_as_proxied() {
    assert!(!names_a_proxy(&HeaderMap::new()));
    for name in PROXY_HEADERS {
        let mut headers = HeaderMap::new();
        headers.insert(name, HeaderValue::from_static("203.0.113.7"));
        assert!(names_a_proxy(&headers), "{name} names a proxy");
    }
}

fn file_route_state(dir: &std::path::Path) -> AppState {
    let pty = crate::web::test_pty_manager();
    AppState::desktop_local(DesktopLocalParts {
        acp: Arc::new(AcpManager::new(vec![])),
        pty,
        relay: Arc::new(WsRelaySink::new()),
        registry: Arc::new(ProjectRegistry::new()),
        acp_catalog: None,
        acp_install: None,
        store: Arc::new(WebStore::open(dir.join("store.json"), StorePolicy::default()).unwrap()),
    })
}

async fn fs_read(state: AppState, path: &std::path::Path) -> serde_json::Value {
    let app = axum::Router::new()
        .route("/fs/read", axum::routing::get(crate::web::fs_api::read))
        .with_state(state);
    let uri = format!(
        "/fs/read?path={}",
        url::form_urlencoded::byte_serialize(path.to_string_lossy().as_bytes()).collect::<String>()
    );
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn fs_read_refuses_the_host_local_credential_file() {
    let dir = tempfile::tempdir().unwrap();
    let (credential, path) = issue(dir.path()).unwrap();
    let neighbour = dir.path().join("notes.txt");
    std::fs::write(&neighbour, "hello").unwrap();

    let refused = fs_read(file_route_state(dir.path()), &path).await;
    assert_eq!(refused["success"], false, "{refused}");
    assert_eq!(refused["code"], "FORBIDDEN", "{refused}");
    assert!(!refused.to_string().contains(credential.as_str()));

    let served = fs_read(file_route_state(dir.path()), &neighbour).await;
    assert_eq!(served["success"], true, "{served}");
    assert_eq!(served["data"]["content"], "hello");
}

fn git_route_state(dir: &std::path::Path) -> AppState {
    let mut state = file_route_state(dir);
    state.registry = ProjectRegistry::seeded_for_test(&dir.canonicalize().unwrap());
    state
}

async fn post(state: AppState, uri: &str, body: serde_json::Value) -> serde_json::Value {
    let app = axum::Router::new()
        .route(
            "/git/diff",
            axum::routing::post(crate::web::git_api::get_diff),
        )
        .route(
            "/git/stage",
            axum::routing::post(crate::web::git_api::stage),
        )
        .with_state(state);
    let response = app
        .oneshot(
            Request::post(uri)
                .header("content-type", "application/json")
                .header(
                    crate::web::command_ledger::COMMAND_ID_HEADER,
                    crate::web::command_ledger::test_clock::fresh_command_id(),
                )
                .extension(axum::extract::ConnectInfo(SocketAddr::from((
                    [127, 0, 0, 1],
                    40000,
                ))))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn git_routes_refuse_the_credential_in_a_state_dir_inside_a_work_tree() {
    let dir = tempfile::tempdir().unwrap();
    let git_ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .is_ok_and(|status| status.success());
    if !git_ok {
        return;
    }
    let (credential, _) = issue(dir.path()).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "hello\n").unwrap();
    let cwd = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let diff = post(
        git_route_state(dir.path()),
        "/git/diff",
        serde_json::json!({ "cwd": cwd, "path": HOST_LOCAL_TOKEN_FILE }),
    )
    .await;
    assert_eq!(diff["code"], "FORBIDDEN", "{diff}");
    assert!(!diff.to_string().contains(credential.as_str()));

    let neighbour = post(
        git_route_state(dir.path()),
        "/git/diff",
        serde_json::json!({ "cwd": cwd, "path": "notes.txt" }),
    )
    .await;
    assert_eq!(neighbour["success"], true, "{neighbour}");
    assert!(neighbour["data"].as_str().unwrap().contains("hello"));

    for path in [HOST_LOCAL_TOKEN_FILE, "."] {
        let staged = post(
            git_route_state(dir.path()),
            "/git/stage",
            serde_json::json!({ "cwd": cwd, "path": path }),
        )
        .await;
        assert_eq!(staged["code"], "FORBIDDEN", "staging {path}: {staged}");
    }
}
