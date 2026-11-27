#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The host-local grant over a real `/ws` socket against the production
//! `ws_upgrade`. A host-local client takes over the pending import of the
//! retired `termul-data.json` at authenticate; that takeover is the
//! observable proof of the grant here.

use std::path::PathBuf;

use super::tests::{ws_read_frame, ws_test_app_state, ws_write_frame};
use super::*;
use crate::web::auth::{WebAuth, WebAuthToken};
use crate::web::capabilities::PROXY_HEADERS;
use crate::web::store::{desktop_legacy_file, StorePolicy};

const BEARER: &str = "bearer-token-for-tests";
const CREDENTIAL: &str = "host-local-credential-for-tests";

struct Host {
    port: u16,
    legacy: PathBuf,
    server: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

async fn host() -> Host {
    let dir = tempfile::tempdir().unwrap();
    let legacy = desktop_legacy_file(dir.path());
    std::fs::write(
        &legacy,
        json!({ "settings/app": { "_version": 1, "data": { "theme": "dark" } } }).to_string(),
    )
    .unwrap();
    let store = WebStore::open(dir.path().join("store.json"), StorePolicy::default())
        .unwrap()
        .with_desktop_legacy(legacy.clone());
    let gate = WebAuth::new(WebAuthToken::new(BEARER).unwrap());
    gate.set_host_local_credential(WebAuthToken::new(CREDENTIAL).unwrap());
    let mut state = ws_test_app_state();
    state.store = Some(Arc::new(store));
    state.web_auth = Some(Arc::new(gate));

    let app = axum::Router::new()
        .route("/ws", axum::routing::get(ws_upgrade))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Host {
        port,
        legacy,
        server,
        _dir: dir,
    }
}

/// Upgrade (with `extra_header` when given), then authenticate as `device`
/// with the bearer token and `credential`. Returns the authenticate reply.
async fn authenticate(
    port: u16,
    extra_header: Option<&str>,
    device: &str,
    credential: Option<&str>,
) -> Value {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let extra = extra_header.map_or_else(String::new, |name| format!("{name}: 203.0.113.7\r\n"));
    let request = format!(
        "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut headers = Vec::new();
    let mut byte = [0u8; 1];
    while !headers.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await.unwrap();
        headers.push(byte[0]);
    }
    assert!(String::from_utf8_lossy(&headers).contains("101"));
    let (_, first) = ws_read_frame(&mut stream).await;
    assert!(String::from_utf8_lossy(&first).contains("auth_required"));
    let mut payload = json!({
        "protocolVersion": WS_PROTOCOL_VERSION,
        "deviceId": device,
        "clientInstanceId": "t1",
        "token": BEARER,
    });
    if let Some(credential) = credential {
        payload["hostLocalToken"] = json!(credential);
    }
    let frame = json!({ "id": "auth", "type": "authenticate", "payload": payload });
    ws_write_frame(&mut stream, 0x1, frame.to_string().as_bytes()).await;
    loop {
        let (opcode, body) = ws_read_frame(&mut stream).await;
        if opcode != 0x1 {
            continue;
        }
        let value: Value = serde_json::from_slice(&body).unwrap();
        if value.get("ok").is_some() {
            return value;
        }
    }
}

#[tokio::test]
async fn a_proxied_peer_with_the_credential_never_becomes_host_local() {
    let host = host().await;
    for name in PROXY_HEADERS {
        let reply = authenticate(host.port, Some(name), "phone", Some(CREDENTIAL)).await;
        assert_eq!(reply["ok"], true, "the bearer still authenticates: {reply}");
        assert!(
            host.legacy.exists(),
            "an upgrade carrying {name} must not be host-local"
        );
    }

    let reply = authenticate(host.port, None, "desk", Some(CREDENTIAL)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(
        !host.legacy.exists(),
        "the same credential without a proxy header is host-local"
    );
    host.server.abort();
}

#[tokio::test]
async fn a_wrong_or_missing_credential_is_not_host_local() {
    let host = host().await;
    for credential in [None, Some(""), Some(BEARER), Some("guess")] {
        let reply = authenticate(host.port, None, "tab", credential).await;
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(host.legacy.exists(), "{credential:?} is not the credential");
    }
    host.server.abort();
}
