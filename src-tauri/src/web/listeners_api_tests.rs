#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The remote listeners switch on and off while the host runs: an agent
//! started before keeps running, and the remote token rotates in place.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

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

#[tokio::test]
async fn a_lan_listener_starts_and_stops_while_an_agent_keeps_running() {
    let dir = tempfile::tempdir().unwrap();
    let acp = running_agent();
    let listeners = listeners(dir.path(), &acp);

    let started = listeners.start_lan(0).await.unwrap();
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

    let first_port = listeners.start_lan(0).await.unwrap().lan.unwrap().port;
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
