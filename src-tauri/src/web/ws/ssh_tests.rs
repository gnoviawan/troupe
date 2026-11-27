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
    let mut authed = true;
    let watch = handle_sync_with_access(
        &frame("watch", json!({ "watch": "ssh" })),
        &mut authed,
        &mut None,
        &mut access,
        None,
    );
    assert_eq!(code(&watch), "forbidden", "the ssh watch needs host-local");
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
async fn ssh_events_reach_the_ssh_watch() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay_with_ssh(dir.path());
    let mut client = Client::new(&relay);

    let watched = client.send("watch", json!({ "watch": "ssh" })).await;
    assert!(watched.ok, "{watched:?}");

    let emit = Arc::clone(&relay.ssh().unwrap().emit);
    (*emit)("ssh-session-ended", json!({ "sessionId": "s1" }));
    loop {
        if let Outbound::Watch(frame) = client.next().await {
            let frame = serde_json::to_value(&frame).unwrap();
            if frame.get("delta").is_some() {
                assert_eq!(frame["watch"], "ssh");
                assert_eq!(frame["delta"]["event"], "ssh-session-ended");
                assert_eq!(frame["delta"]["payload"]["sessionId"], "s1");
                break;
            }
        }
    }
}
