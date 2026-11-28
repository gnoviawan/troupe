#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The remote listeners switch on and off while the host runs: an agent
//! started before keeps running, and the remote token rotates in place.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::acp::{AcpManager, AgentId};
use crate::web::auth::WEB_AUTH_TOKEN_FILE;
use crate::web::listeners_api::{ListenerParts, Listeners};
use crate::web::project_registry::ProjectRegistry;
use crate::web::sink::WsRelaySink;
use crate::web::store::{StorePolicy, WebStore};

const AGENT: &str = "agent-kept-alive";

fn listeners(dir: &Path, acp: &Arc<AcpManager>) -> Listeners {
    Listeners::new(ListenerParts {
        acp: Arc::clone(acp),
        pty: crate::web::test_pty_manager(),
        relay: Arc::new(WsRelaySink::new()),
        registry: Arc::new(ProjectRegistry::new()),
        acp_catalog: None,
        acp_install: None,
        store: Arc::new(WebStore::open(dir.join("store.json"), StorePolicy::default()).unwrap()),
        state_dir: dir.to_path_buf(),
        allow_remote_writes: false,
    })
}

fn running_agent() -> Arc<AcpManager> {
    let acp = Arc::new(AcpManager::new(vec![]));
    acp.install_test_agent_with_turns(AgentId(AGENT.to_string()), HashSet::new(), vec![], false);
    assert!(acp.list_agents().contains(&AgentId(AGENT.to_string())));
    acp
}

/// `GET /health` on loopback; `None` when nothing answers.
async fn health(port: u16) -> Option<String> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .ok()?;
    let request =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.ok()?;
    let text = String::from_utf8_lossy(&response).into_owned();
    text.lines().next().map(str::to_string)
}

fn token(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(WEB_AUTH_TOKEN_FILE))
        .unwrap()
        .trim()
        .to_string()
}

fn non_ephemeral_lan_test_port() -> u16 {
    const FIRST_NON_EPHEMERAL_PORT: u16 = 20_000;
    const NON_EPHEMERAL_PORT_COUNT: u32 = 10_000;
    let seed = std::process::id();
    for offset in 0..NON_EPHEMERAL_PORT_COUNT {
        let port = FIRST_NON_EPHEMERAL_PORT
            + (seed.wrapping_add(offset) % NON_EPHEMERAL_PORT_COUNT) as u16;
        if std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).is_ok() {
            return port;
        }
    }
    panic!("no LAN test port is free");
}

#[tokio::test]
async fn a_lan_listener_starts_and_stops_while_an_agent_keeps_running() {
    let dir = tempfile::tempdir().unwrap();
    let acp = running_agent();
    let listeners = listeners(dir.path(), &acp);

    let started = listeners
        .start_lan(non_ephemeral_lan_test_port())
        .await
        .unwrap();
    let port = started.lan.as_ref().expect("the LAN listener runs").port;
    assert!(!started.remote_writes);
    assert!(
        health(port).await.unwrap().contains("200"),
        "the LAN listener serves"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(dir.path().join(WEB_AUTH_TOKEN_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the remote token is owner-only");
    }
    for url in &started.lan.as_ref().unwrap().urls {
        assert!(url.ends_with(&format!("#token={}", token(dir.path()))));
    }

    let stopped = listeners.stop_lan().await;
    assert!(stopped.lan.is_none());
    assert_eq!(health(port).await, None, "the port is unbound");
    assert!(
        acp.list_agents().contains(&AgentId(AGENT.to_string())),
        "stopping the listener leaves the agent running"
    );

    let again = listeners.start_lan(0).await.unwrap();
    assert!(again.lan.is_some(), "the listener starts again");
    listeners.stop_all().await;
    assert!(acp.list_agents().contains(&AgentId(AGENT.to_string())));
}

#[tokio::test]
async fn rotating_the_token_replaces_it_and_keeps_the_lan_listener() {
    let dir = tempfile::tempdir().unwrap();
    let acp = running_agent();
    let listeners = listeners(dir.path(), &acp);

    let first_port = listeners
        .start_lan(non_ephemeral_lan_test_port())
        .await
        .unwrap()
        .lan
        .unwrap()
        .port;
    let before = token(dir.path());
    let rotated = listeners.rotate_token().await.unwrap();
    let after = token(dir.path());
    assert_ne!(before, after, "a new token");
    let lan = rotated.lan.expect("the listener is back");
    assert_eq!(lan.port, first_port, "on the same port");
    assert!(health(lan.port).await.unwrap().contains("200"));
    assert!(acp.list_agents().contains(&AgentId(AGENT.to_string())));
    listeners.stop_all().await;
}

#[tokio::test]
async fn rotating_with_nothing_running_only_replaces_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let acp = running_agent();
    let listeners = listeners(dir.path(), &acp);
    let status = listeners.rotate_token().await.unwrap();
    assert!(status.lan.is_none() && !status.tunnel_running);
    assert!(!token(dir.path()).is_empty());
}

#[tokio::test]
async fn rotating_the_token_closes_every_preview_session() {
    let dir = tempfile::tempdir().unwrap();
    let page = dir.path().join("page.html");
    std::fs::write(&page, "<p>hi</p>").unwrap();
    let opened = crate::html_preview::registry()
        .open(&page.to_string_lossy(), String::new())
        .unwrap();
    let acp = running_agent();
    let listeners = listeners(dir.path(), &acp);
    listeners.rotate_token().await.unwrap();
    assert!(
        !crate::html_preview::registry().close(&opened.token),
        "the preview URL stops serving once the token rotates"
    );
}

#[tokio::test]
async fn a_stalled_download_cannot_hold_the_lan_listener_open() {
    let dir = tempfile::tempdir().unwrap();
    let acp = running_agent();
    let listeners = listeners(dir.path(), &acp);
    let port = listeners.start_lan(0).await.unwrap().lan.unwrap().port;

    let big = dir.path().join("big.mp4");
    std::fs::write(&big, vec![7u8; 48 * 1024 * 1024]).unwrap();
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let request = format!(
        "GET /fs/raw?path={} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\n\r\n",
        url::form_urlencoded::byte_serialize(big.to_string_lossy().as_bytes()).collect::<String>(),
        token(dir.path())
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut head = [0u8; 64];
    let read = stream.read(&mut head).await.unwrap();
    assert!(
        String::from_utf8_lossy(&head[..read]).contains("200"),
        "the download started"
    );

    let stopped = tokio::time::timeout(
        crate::web::listeners_api::STOP_BOUND + Duration::from_secs(5),
        listeners.stop_lan(),
    )
    .await
    .expect("the stop does not wait for the stalled reader");
    assert!(stopped.lan.is_none());
    assert!(acp.list_agents().contains(&AgentId(AGENT.to_string())));
    drop(stream);
}
