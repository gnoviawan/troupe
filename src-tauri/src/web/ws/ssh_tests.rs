#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The SSH `/ws` requests: refused without the host-local grant, served from
//! the state dir with it, and never echoing a payload value.

use std::path::Path;
use std::time::Duration;

use super::tests::{handle_sync_with_access, test_access, test_env, test_outbound};
use super::*;
use crate::web::capabilities::WriteFlags;
use tokio_util::sync::CancellationToken;

fn frame(type_: &str, payload: Value) -> String {
    json!({ "id": "r", "type": type_, "payload": payload }).to_string()
}

fn code(reply: &WsReply) -> String {
    reply
        .err
        .as_ref()
        .map(|e| e.code.clone())
        .unwrap_or_default()
}

fn message(reply: &WsReply) -> String {
    reply
        .err
        .as_ref()
        .map(|e| e.message.clone())
        .unwrap_or_default()
}

/// A direct, gated loopback `/ws` connection that presented the credential.
fn host_local_access() -> ConnectionAccess {
    let mut access = ConnectionAccess::new(CapabilityPolicy {
        carrier: Carrier::Ws,
        peer: SocketAddr::from(([127, 0, 0, 1], 40000)),
        flags: WriteFlags {
            allow_remote_writes: false,
            shared_live_writes_denied: false,
        },
        token_gated: true,
        proxied: false,
    });
    access.grant(true);
    assert!(access.capabilities().host_local);
    access
}

/// A relay with the desktop mode's SSH manager on `dir`.
fn relay_with_ssh(dir: &Path) -> Arc<WsRelaySink> {
    let relay = Arc::new(WsRelaySink::new());
    crate::web::desktop_mode::attach_ssh(&relay, dir);
    relay
}

struct Client {
    conn: ConnState,
    env: HandlerEnv,
    rx: OutboundReceiver,
    exec: KeyedExecutor,
}

impl Client {
    fn new(relay: &Arc<WsRelaySink>) -> Self {
        let (tx, rx) = test_outbound();
        let conn = ConnState::authenticated(host_local_access());
        let env = test_env(
            &Arc::new(AcpManager::new(vec![])),
            relay,
            &Arc::new(ProjectRegistry::new()),
            &tx,
            &conn.subscriptions,
        );
        Self {
            conn,
            env,
            rx,
            exec: KeyedExecutor::new(CancellationToken::new()),
        }
    }

    async fn next(&mut self) -> Outbound {
        tokio::time::timeout(Duration::from_secs(5), self.rx.recv())
            .await
            .expect("a frame within 5 s")
            .expect("the queue is open")
    }

    async fn send(&mut self, type_: &str, payload: Value) -> WsReply {
        let text = frame(type_, payload);
        assert!(dispatch_connection_text(&text, &mut self.conn, &self.env, &self.exec).await);
        loop {
            if let Outbound::Reply(reply) = self.next().await {
                return reply;
            }
        }
    }
}

#[test]
fn every_ssh_request_is_forbidden_to_a_writer_that_is_not_host_local() {
    let mut access = test_access();
    assert!(access.capabilities().write && !access.capabilities().host_local);
    for type_ in SSH_WS_REQUESTS {
        let mut authed = true;
        let reply = handle_sync_with_access(
            &frame(type_, json!({})),
            &mut authed,
            &mut None,
            &mut access,
            None,
        );
        assert_eq!(code(&reply), "forbidden", "{type_}");
        assert!(message(&reply).contains("credentials"), "{type_}");
    }
}

#[tokio::test]
async fn a_host_without_an_ssh_manager_answers_unsupported() {
    let relay = Arc::new(WsRelaySink::new());
    let mut client = Client::new(&relay);
    let reply = client.send("ssh_list_profiles", json!({})).await;
    assert_eq!(code(&reply), "unsupported");
}

#[tokio::test]
async fn a_host_local_client_keeps_profiles_in_the_tauri_profile_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(&relay_with_ssh(dir.path()));

    let empty = client.send("ssh_list_profiles", json!({})).await;
    assert!(empty.ok, "{empty:?}");
    assert_eq!(empty.payload, Some(json!([])));

    let saved = client
        .send(
            "ssh_save_profile",
            json!({ "profile": {
                "id": "p1",
                "name": "prod",
                "host": "prod.example",
                "port": 22,
                "username": "deploy",
                "authMethod": "agent",
                "portForwards": [],
            }}),
        )
        .await;
    assert!(saved.ok, "{saved:?}");

    let listed = client.send("ssh_list_profiles", json!({})).await;
    assert_eq!(listed.payload.as_ref().unwrap()[0]["host"], "prod.example");

    let file: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("ssh-profiles.json")).unwrap())
            .unwrap();
    assert_eq!(file["profiles"][0]["id"], "p1");
    assert_eq!(file["profiles"][0]["username"], "deploy");
}

#[tokio::test]
async fn sessions_and_sftp_name_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(&relay_with_ssh(dir.path()));

    let open = client
        .send(
            "ssh_session_open",
            json!({ "request": { "sessionId": "s1", "profileId": "nope", "cols": 80, "rows": 24 } }),
        )
        .await;
    assert_eq!(code(&open), "SSH_PROFILE_NOT_FOUND");

    let list = client
        .send(
            "sftp_list_dir",
            json!({ "request": { "connectionId": "s1", "remotePath": "/" } }),
        )
        .await;
    assert_eq!(code(&list), "SSH_CONNECTION_NOT_FOUND");

    let sessions = client.send("ssh_session_list", json!({})).await;
    assert_eq!(sessions.payload, Some(json!([])));
}

#[tokio::test]
async fn a_malformed_payload_never_repeats_a_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(&relay_with_ssh(dir.path()));
    let reply = client
        .send(
            "ssh_session_open",
            json!({ "request": {
                "sessionId": "s1",
                "profileId": "p1",
                "cols": "hunter2-secret",
                "rows": 24,
                "secret": "hunter2-secret",
            }}),
        )
        .await;
    assert_eq!(code(&reply), "SSH_INVALID_REQUEST");
    assert!(!message(&reply).contains("hunter2"), "{}", message(&reply));
}

#[tokio::test]
async fn trusting_a_host_key_is_refused_until_the_prompt_reaches_the_client() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(&relay_with_ssh(dir.path()));
    let trusted = client
        .send(
            "ssh_session_respond_host_key",
            json!({ "sessionId": "s1", "trust": true }),
        )
        .await;
    assert_eq!(code(&trusted), "SSH_HOST_KEY_TRUST_UNAVAILABLE");
    let rejected = client
        .send(
            "ssh_session_respond_host_key",
            json!({ "sessionId": "s1", "trust": false }),
        )
        .await;
    assert!(rejected.ok, "{rejected:?}");
}

fn password_profile(id: &str, host: &str, claims_stored: bool) -> Value {
    json!({ "profile": {
        "id": id,
        "name": "prod",
        "host": host,
        "port": 22,
        "username": "deploy",
        "authMethod": "password",
        "portForwards": [],
        "hasStoredPassword": claims_stored,
        "hasStoredPassphrase": false,
    }})
}

#[tokio::test]
async fn whether_a_password_is_stored_is_the_hosts_record_and_a_moved_profile_loses_it() {
    let dir = tempfile::tempdir().unwrap();
    // A profile whose password the keychain holds, as an earlier run left it.
    std::fs::write(
        dir.path().join("ssh-profiles.json"),
        json!({ "profiles": [{
            "id": "p1",
            "name": "prod",
            "host": "a.example",
            "port": 22,
            "username": "deploy",
            "authMethod": "password",
            "jumpHostId": null,
            "portForwards": [],
            "tags": null,
            "lastConnected": null,
            "importedFrom": null,
            "hasStoredPassword": true,
        }]})
        .to_string(),
    )
    .unwrap();
    let mut client = Client::new(&relay_with_ssh(dir.path()));
    let stored = |reply: &WsReply, id: &str| {
        reply
            .payload
            .as_ref()
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|profile| profile["id"] == id)
            .unwrap()["hasStoredPassword"]
            .clone()
    };
    let edit = password_profile("p1", "a.example", false);
    assert!(client.send("ssh_save_profile", edit).await.ok);
    let listed = client.send("ssh_list_profiles", json!({})).await;
    assert_eq!(stored(&listed, "p1"), json!(true), "the same host keeps it");

    for attempt in 0..2 {
        let moved = password_profile("p1", "b.example", true);
        assert!(client.send("ssh_save_profile", moved).await.ok);
        let listed = client.send("ssh_list_profiles", json!({})).await;
        assert_eq!(listed.payload.as_ref().unwrap()[0]["host"], "b.example");
        assert_eq!(
            stored(&listed, "p1"),
            json!(false),
            "save {attempt} to the new host must not bring the old password back"
        );
    }

    let fresh = password_profile("p2", "c.example", true);
    assert!(client.send("ssh_save_profile", fresh).await.ok);
    let listed = client.send("ssh_list_profiles", json!({})).await;
    assert_eq!(
        stored(&listed, "p2"),
        json!(false),
        "a client cannot claim a stored password"
    );
}

/// A TCP server that accepts and never speaks SSH, so a connect hangs in the
/// handshake.
async fn silent_host() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    (port, server)
}

fn silent_target(port: u16) -> crate::ssh::session::SessionTarget {
    crate::ssh::session::SessionTarget {
        profile_id: "p1".to_string(),
        host: "127.0.0.1".to_string(),
        port,
        username: "deploy".to_string(),
        auth_method: "agent".to_string(),
        key_path: None,
    }
}

#[tokio::test]
async fn a_dropped_open_releases_its_session_id() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay_with_ssh(dir.path());
    let ssh = relay.ssh().unwrap();
    let pty = crate::web::test_pty_manager();
    let (port, server) = silent_host().await;

    let open = ssh
        .sessions
        .open(&pty, "s1", silent_target(port), None, 80, 24);
    assert!(
        tokio::time::timeout(Duration::from_millis(500), open)
            .await
            .is_err(),
        "the handshake hangs"
    );
    assert!(
        !ssh.sessions.cancel("s1"),
        "the dropped open no longer holds s1"
    );
    server.abort();
}

#[tokio::test]
async fn pending_connects_are_capped() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay_with_ssh(dir.path());
    let ssh = relay.ssh().unwrap();
    let pty = crate::web::test_pty_manager();
    let (port, server) = silent_host().await;

    let mut opens = Vec::new();
    for n in 0..crate::ssh::session::MAX_PENDING_CONNECTS {
        let (ssh, pty) = (Arc::clone(&ssh), Arc::clone(&pty));
        opens.push(tokio::spawn(async move {
            let _ = ssh
                .sessions
                .open(&pty, &format!("s{n}"), silent_target(port), None, 80, 24)
                .await;
        }));
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let over = tokio::time::timeout(
        Duration::from_secs(2),
        ssh.sessions
            .open(&pty, "one-too-many", silent_target(port), None, 80, 24),
    )
    .await
    .expect("refused at once, not queued");
    let error = over.expect_err("refused");
    assert!(error.detail.contains("Too many"), "{}", error.detail);
    for open in opens {
        open.abort();
    }
    server.abort();
}

#[test]
fn the_tauri_commands_save_and_open_through_the_same_service() {
    // The desktop app's commands must apply the same rules as these routes:
    // no client-claimed stored secret, no secret reused across targets.
    let commands = include_str!("../../commands/ssh.rs");
    assert!(commands.contains("service::save_profile(&ssh_manager, profile)"));
    assert!(commands.contains("service::open_session("));
    assert!(
        !commands.contains("profiles.save("),
        "ssh_save_profile bypasses ssh::service"
    );
    assert!(
        !commands.contains("cached_secret("),
        "ssh_session_open uses the target-blind cache"
    );
}
