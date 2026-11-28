#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use super::super::tests::{test_env, test_outbound, test_policy};
use super::*;
use crate::browser_automation::engine::chromium::perimeter::{InternalAddresses, Perimeter};
use crate::web::project_registry::ProjectSummary;

const USER_TAB: &str = "tab-00000000-0000-4000-8000-000000000001";
const AGENT_TAB: &str = "agent-00000000-0000-4000-8000-000000000002";

type HandlerCase<'a> = (
    &'a str,
    &'a HandlerEnv,
    &'a OutboundSender,
    Option<&'a str>,
    Option<&'a str>,
);

struct ClearHost;

impl Drop for ClearHost {
    fn drop(&mut self) {
        crate::browser_automation::clear_browser_host();
    }
}

/// Spy automation host recording `destroy_tab` calls and supplying live agent
/// tab ownership matching `core/tabs.rs:595-605`.
#[derive(Default)]
struct SpyAutomationHost {
    destroyed: Mutex<Vec<String>>,
    agent_owners: Mutex<HashMap<String, (String, String)>>,
}

impl crate::browser_automation::BrowserHost for SpyAutomationHost {
    fn execute<'a>(
        &'a self,
        _session_id: &'a str,
        _agent_id: &'a str,
        _call: crate::browser_automation::BrowserCall,
        _deadline: crate::browser_automation::budgets::Deadline,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        crate::browser_automation::Success,
                        crate::browser_automation::BrowserError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async { Ok(crate::browser_automation::Success::default()) })
    }

    fn end_session(&self, _session_id: &str) {}

    fn destroy_tab<'a>(&'a self, label: &'a str) -> crate::browser_automation::HostFuture<'a, ()> {
        self.destroyed.lock().push(label.to_string());
        Box::pin(async {})
    }

    fn agent_tab_owner(&self, label: &str) -> Option<(String, String)> {
        self.agent_owners.lock().get(label).cloned()
    }

    fn engine_facts(&self) -> crate::browser_automation::engine::EngineFacts {
        crate::browser_automation::engine::EngineFacts::all_trusted()
    }
}

struct TestRig {
    acp: Arc<AcpManager>,
    relay: Arc<WsRelaySink>,
    registry: Arc<ProjectRegistry>,
    host: Arc<BrowserHost>,
}

impl TestRig {
    async fn new() -> Self {
        let acp = Arc::new(AcpManager::new(vec![]));
        let relay = Arc::new(WsRelaySink::new());
        let registry = Arc::new(ProjectRegistry::new());
        let perimeter = Perimeter::start(InternalAddresses::Deny).await.unwrap();
        let host = BrowserHost::new(
            "unused".into(),
            "unused".into(),
            "unused".into(),
            false,
            perimeter,
            Arc::clone(&relay),
        );
        Self {
            acp,
            relay,
            registry,
            host,
        }
    }

    fn client_env(
        &self,
        sessions: &[&str],
        device_id: Option<&str>,
    ) -> (HandlerEnv, OutboundSender, OutboundReceiver) {
        let (tx, rx) = test_outbound();
        let subscriptions = Arc::new(Subscriptions::default());
        for session in sessions {
            subscriptions.replace(session, &[], ClientId::new());
        }
        let env = test_env(&self.acp, &self.relay, &self.registry, &tx, &subscriptions);
        if let Some(device) = device_id {
            let _ = env.browser_principal.set(
                if device == "device-b" {
                    "host-local"
                } else {
                    "host-token"
                }
                .into(),
            );
            let _ = env.identity.set(ClientIdentity {
                device_id: device.to_string(),
                client_instance_id: format!("inst-{}", uuid::Uuid::new_v4()),
            });
        }
        (env, tx, rx)
    }
}

fn err_code(reply: &WsReply) -> String {
    reply
        .err
        .as_ref()
        .map(|err| err.code.clone())
        .unwrap_or_default()
}

fn input_payload(tab_id: &str) -> Value {
    json!({
        "tabId": tab_id,
        "event": { "kind": "text", "text": "hello" }
    })
}

/// Reconnect recovery: the original connection closes, a fresh connection from
/// the same chat, project and principal cannot watch before attaching, then
/// `browser_tab_attach` re-grants screencast access and every pane control
/// passes authorization.
#[test]
fn reconnect_recovery_regrants_screencast_and_preserves_tab_control() {
    let _host_lock = crate::browser_automation::TEST_HOST_LOCK.lock().unwrap();
    let _clear = ClearHost;
    let spy = Arc::new(SpyAutomationHost::default());
    crate::browser_automation::set_browser_host(spy.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let rig = TestRig::new().await;
        let (orig_env, orig_tx, _orig_rx) = rig.client_env(&["chat-a"], Some("device-a"));
        rig.host.claim_user_tab(
            USER_TAB,
            TabOwner::new("chat-a", "proj-1", Some("host-token".into())),
        );
        rig.host.mark_test_tab_live(USER_TAB);
        rig.host.grant(USER_TAB, &orig_tx);

        // Original connection can watch while alive.
        assert!(screencast_watch_allowed_with_host(
            "w0",
            USER_TAB,
            &orig_env,
            Arc::clone(&rig.host)
        )
        .is_ok());

        // Original connection drops; a new connection reconnects for the same chat & principal.
        orig_tx.closing().cancel();
        assert!(screencast_watch_allowed_with_host(
            "w1",
            USER_TAB,
            &orig_env,
            Arc::clone(&rig.host)
        )
        .is_err());

        let (reconn_env, _reconn_tx, _reconn_rx) = rig.client_env(&["chat-a"], Some("device-a"));

        // Before attach, the reconnected socket has no screencast grant yet.
        let pre_watch =
            screencast_watch_allowed_with_host("w2", USER_TAB, &reconn_env, Arc::clone(&rig.host))
                .err()
                .unwrap();
        assert_eq!(err_code(&pre_watch), "forbidden");

        // Real `browser_tab_attach` re-grants screencast to the reconnected socket.
        let attach_reply = handle_with_host(
            "a1".into(),
            "browser_tab_attach",
            &json!({ "tabId": USER_TAB }),
            &reconn_env,
            &rig.host,
        )
        .await;
        assert!(
            attach_reply.ok,
            "reconnected owner must attach: {:?}",
            attach_reply.err
        );
        assert!(screencast_watch_allowed_with_host(
            "w3",
            USER_TAB,
            &reconn_env,
            Arc::clone(&rig.host)
        )
        .is_ok());

        // Real navigate, resize and input pass authorization and reach the absent engine (`not_found`, never `forbidden`).
        for (req_type, payload) in [
            (
                "browser_tab_navigate",
                json!({ "tabId": USER_TAB, "url": "https://example.com" }),
            ),
            (
                "browser_tab_resize",
                json!({ "tabId": USER_TAB, "width": 800, "height": 600 }),
            ),
            ("browser_tab_input", input_payload(USER_TAB)),
        ] {
            let reply =
                handle_with_host("op".into(), req_type, &payload, &reconn_env, &rig.host).await;
            assert_eq!(
                err_code(&reply),
                "not_found",
                "reconnected owner must pass authz on {req_type}"
            );
        }

        // Real close succeeds and reaches `destroy_tab`.
        let close_reply = handle_with_host(
            "c1".into(),
            "browser_tab_close",
            &json!({ "tabId": USER_TAB }),
            &reconn_env,
            &rig.host,
        )
        .await;
        assert!(close_reply.ok, "reconnected owner must close the tab");
        assert_eq!(spy.destroyed.lock().as_slice(), [USER_TAB.to_string()]);
    });
}

/// Cross-chat, cross-project and cross-principal callers are refused by the
/// real attach, close, navigate, resize, input and screencast-watch handlers.
/// Removing the ownership check in any handler makes its assertion fail.
#[test]
fn cross_chat_project_and_principal_refused_by_all_real_handlers() {
    let _host_lock = crate::browser_automation::TEST_HOST_LOCK.lock().unwrap();
    let _clear = ClearHost;
    let spy = Arc::new(SpyAutomationHost::default());
    crate::browser_automation::set_browser_host(spy.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let rig = TestRig::new().await;
        rig.host.claim_user_tab(
            USER_TAB,
            TabOwner::new("chat-a", "proj-1", Some("host-token".into())),
        );
        rig.host.mark_test_tab_live(USER_TAB);

        // 1. Different chat (subscribed only to chat-b, same device).
        let (other_chat_env, other_chat_tx, _rx1) = rig.client_env(&["chat-b"], Some("device-a"));
        // 2. Different principal (subscribed to chat-a, different device).
        let (other_principal_env, other_principal_tx, _rx2) =
            rig.client_env(&["chat-a"], Some("device-b"));
        // 3. Unidentified caller (subscribed to chat-a, no ClientIdentity).
        let (no_principal_env, no_principal_tx, _rx3) = rig.client_env(&["chat-a"], None);
        // 4. Multiplexed socket subscribed to both chats but claiming chat-b or proj-2.
        let (mux_env, mux_tx, _rx4) = rig.client_env(&["chat-a", "chat-b"], Some("device-a"));

        let cases: &[HandlerCase<'_>] = &[
            ("other_chat", &other_chat_env, &other_chat_tx, None, None),
            (
                "other_principal",
                &other_principal_env,
                &other_principal_tx,
                None,
                None,
            ),
            (
                "missing_principal",
                &no_principal_env,
                &no_principal_tx,
                None,
                None,
            ),
            (
                "claimed_other_chat",
                &mux_env,
                &mux_tx,
                Some("chat-b"),
                None,
            ),
            (
                "claimed_other_project",
                &mux_env,
                &mux_tx,
                Some("chat-a"),
                Some("proj-2"),
            ),
        ];

        for (case_name, env, tx, claimed_session, claimed_project) in cases {
            let mut attach_body = json!({ "tabId": USER_TAB });
            let mut close_body = json!({ "tabId": USER_TAB });
            let mut nav_body = json!({ "tabId": USER_TAB, "url": "https://example.com" });
            let mut resize_body = json!({ "tabId": USER_TAB, "width": 800, "height": 600 });
            let mut input_body = input_payload(USER_TAB);
            for body in [
                &mut attach_body,
                &mut close_body,
                &mut nav_body,
                &mut resize_body,
                &mut input_body,
            ] {
                if let Some(session) = claimed_session {
                    body["sessionId"] = json!(session);
                }
                if let Some(project) = claimed_project {
                    body["projectId"] = json!(project);
                }
            }

            for (req_type, payload) in [
                ("browser_tab_attach", attach_body),
                ("browser_tab_close", close_body),
                ("browser_tab_navigate", nav_body),
                ("browser_tab_resize", resize_body),
                ("browser_tab_input", input_body),
            ] {
                let reply = handle_with_host("r".into(), req_type, &payload, env, &rig.host).await;
                assert!(!reply.ok, "{case_name}: {req_type} must be refused");
                assert_eq!(
                    err_code(&reply),
                    "forbidden",
                    "{case_name}: {req_type} must return forbidden"
                );
            }

            assert!(
                !rig.host.may_watch(USER_TAB, tx),
                "{case_name}: refused attach must not grant screencast"
            );
            let watch =
                screencast_watch_allowed_with_host("w", USER_TAB, env, Arc::clone(&rig.host));
            assert!(
                watch.is_err(),
                "{case_name}: screencast watch must be refused"
            );
            assert_eq!(err_code(&watch.err().unwrap()), "forbidden");
        }

        assert!(
            spy.destroyed.lock().is_empty(),
            "refused close requests must never reach destroy_tab"
        );
    });
}

/// Read-only (view-only) connections stay refused on all six pane requests by
/// the `/ws` capability gate and cannot watch the screencast, even when
/// subscribed to the owning chat with the owning principal.
#[tokio::test]
async fn read_only_connection_is_refused_every_pane_request_and_screencast_watch() {
    let rig = TestRig::new().await;
    rig.host.claim_user_tab(
        USER_TAB,
        TabOwner::new("chat-a", "proj-1", Some("host-token".into())),
    );
    rig.host.mark_test_tab_live(USER_TAB);

    let (tx, mut rx) = test_outbound();
    let view_only = ConnectionAccess::granted(test_policy([192, 168, 1, 50], false, false));
    let mut conn = ConnState::authenticated(view_only);
    conn.subscriptions.replace("chat-a", &[], ClientId::new());
    let _ = conn.identity.set(ClientIdentity {
        device_id: "device-a".into(),
        client_instance_id: "readonly-inst".into(),
    });
    let env = HandlerEnv {
        identity: Arc::clone(&conn.identity),
        ..test_env(
            &rig.acp,
            &rig.relay,
            &rig.registry,
            &tx,
            &conn.subscriptions,
        )
    };
    let exec = KeyedExecutor::new(CancellationToken::new());

    for (req_type, payload) in [
        (
            "browser_tab_open",
            json!({ "projectId": "proj-1", "sessionId": "chat-a", "url": "https://example.com" }),
        ),
        ("browser_tab_attach", json!({ "tabId": USER_TAB })),
        ("browser_tab_close", json!({ "tabId": USER_TAB })),
        (
            "browser_tab_navigate",
            json!({ "tabId": USER_TAB, "url": "https://example.com" }),
        ),
        (
            "browser_tab_resize",
            json!({ "tabId": USER_TAB, "width": 800, "height": 600 }),
        ),
        ("browser_tab_input", input_payload(USER_TAB)),
    ] {
        let frame = json!({ "id": "ro", "type": req_type, "payload": payload }).to_string();
        assert!(dispatch_connection_text(&frame, &mut conn, &env, &exec).await);
        let Some(Outbound::Reply(reply)) = rx.recv().await else {
            panic!("expected reply for {req_type}");
        };
        assert!(!reply.ok, "view-only connection must be refused {req_type}");
        assert_eq!(err_code(&reply), "forbidden", "{req_type}");
    }

    let watch = screencast_watch_allowed_with_host("w-ro", USER_TAB, &env, Arc::clone(&rig.host));
    assert!(
        watch.is_err(),
        "view-only connection must not watch screencast"
    );
    assert_eq!(err_code(&watch.err().unwrap()), "forbidden");
}

/// Agent tabs resolve their owning chat and project through the core
/// `TabRegistry` contract (`core/tabs.rs:595-605`): same-chat connections may
/// attach and watch, while other chats and unknown tabs are refused.
#[test]
fn agent_tab_ownership_follows_core_tab_registry_rules() {
    let _host_lock = crate::browser_automation::TEST_HOST_LOCK.lock().unwrap();
    let _clear = ClearHost;
    let spy = Arc::new(SpyAutomationHost::default());
    spy.agent_owners
        .lock()
        .insert(AGENT_TAB.to_string(), ("chat-a".into(), "proj-1".into()));
    crate::browser_automation::set_browser_host(spy.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let rig = TestRig::new().await;
        rig.host.mark_test_tab_live(AGENT_TAB);

        let (owner_env, _owner_tx, _rx1) = rig.client_env(&["chat-a"], Some("device-any"));
        let (other_env, other_tx, _rx2) = rig.client_env(&["chat-b"], Some("device-any"));

        // Another chat is refused on every control handler.
        for (req_type, payload) in [
            ("browser_tab_attach", json!({ "tabId": AGENT_TAB })),
            ("browser_tab_close", json!({ "tabId": AGENT_TAB })),
            (
                "browser_tab_navigate",
                json!({ "tabId": AGENT_TAB, "url": "https://example.com" }),
            ),
            (
                "browser_tab_resize",
                json!({ "tabId": AGENT_TAB, "width": 800, "height": 600 }),
            ),
            ("browser_tab_input", input_payload(AGENT_TAB)),
        ] {
            let reply =
                handle_with_host("r".into(), req_type, &payload, &other_env, &rig.host).await;
            assert_eq!(err_code(&reply), "forbidden", "other chat on {req_type}");
        }
        assert!(!rig.host.may_watch(AGENT_TAB, &other_tx));
        assert!(spy.destroyed.lock().is_empty());

        // The owning chat can attach and watch its agent tab.
        let attach_reply = handle_with_host(
            "a1".into(),
            "browser_tab_attach",
            &json!({ "tabId": AGENT_TAB }),
            &owner_env,
            &rig.host,
        )
        .await;
        assert!(attach_reply.ok, "owning chat must attach to its agent tab");
        assert!(screencast_watch_allowed_with_host(
            "w1",
            AGENT_TAB,
            &owner_env,
            Arc::clone(&rig.host)
        )
        .is_ok());

        // An unknown/unowned tab id is refused (never falls back to another tab).
        let unknown_reply = handle_with_host(
            "a2".into(),
            "browser_tab_attach",
            &json!({ "tabId": "agent-00000000-0000-4000-8000-000000000099" }),
            &owner_env,
            &rig.host,
        )
        .await;
        assert_eq!(err_code(&unknown_reply), "forbidden");
    });
}

/// `browser_tab_open` requires a known project and an active subscription to
/// the owning chat session.
#[tokio::test]
async fn open_handler_enforces_session_subscription_and_project_binding() {
    let rig = TestRig::new().await;
    let project_dir = tempfile::tempdir().unwrap();
    rig.registry.set(
        vec![ProjectSummary {
            id: "proj-1".into(),
            name: "Project 1".into(),
            color: "blue".into(),
            path: Some(project_dir.path().to_string_lossy().into_owned()),
            is_archived: false,
            is_default: false,
        }],
        None,
    );

    let (unsubscribed_env, _tx, _rx) = rig.client_env(&["chat-other"], Some("device-a"));

    let empty_session = handle_with_host(
        "o1".into(),
        "browser_tab_open",
        &json!({ "projectId": "proj-1", "sessionId": "", "url": "https://example.com" }),
        &unsubscribed_env,
        &rig.host,
    )
    .await;
    assert_eq!(err_code(&empty_session), "unsupported");

    let missing_project = handle_with_host(
        "o2".into(),
        "browser_tab_open",
        &json!({ "projectId": "proj-missing", "sessionId": "chat-a", "url": "https://example.com" }),
        &unsubscribed_env,
        &rig.host,
    )
    .await;
    assert_eq!(err_code(&missing_project), "not_found");

    let unsubscribed = handle_with_host(
        "o3".into(),
        "browser_tab_open",
        &json!({ "projectId": "proj-1", "sessionId": "chat-a", "url": "https://example.com" }),
        &unsubscribed_env,
        &rig.host,
    )
    .await;
    assert_eq!(err_code(&unsubscribed), "forbidden");
}

// Exercise the production authenticate dispatcher rather than injecting identity.
async fn authenticated_browser_env(rig: &TestRig, device: &str, read_only: bool) -> HandlerEnv {
    let (tx, mut rx) = test_outbound();
    let mut conn = ConnState::new(
        ConnectionAccess::new(test_policy([127, 0, 0, 1], false, read_only)),
        Arc::new(Subscriptions::default()),
    );
    let env = HandlerEnv {
        identity: Arc::clone(&conn.identity),
        browser_principal: Arc::clone(&conn.browser_principal),
        web_auth: Some(Arc::new(WebAuth::new(
            crate::web::auth::WebAuthToken::new("test-token").unwrap(),
        ))),
        ..test_env(
            &rig.acp,
            &rig.relay,
            &rig.registry,
            &tx,
            &conn.subscriptions,
        )
    };
    let frame = json!({"id":"auth", "type":"authenticate", "payload": {"protocolVersion":WS_PROTOCOL_VERSION,"deviceId":device,"clientInstanceId":"test-instance","token":"test-token"}}).to_string();
    let exec = KeyedExecutor::new(CancellationToken::new());
    assert!(dispatch_connection_text(&frame, &mut conn, &env, &exec).await);
    match rx.recv().await {
        Some(Outbound::Reply(reply)) => assert!(reply.ok),
        _ => panic!("missing auth reply"),
    }
    env.subscriptions.replace("chat-a", &[], ClientId::new());
    env
}

#[tokio::test]
async fn authenticate_spoofed_device_cannot_cross_auth_class() {
    let rig = TestRig::new().await;
    let env = authenticated_browser_env(&rig, "victim-device", false).await;
    assert_eq!(caller_principal(&env).as_deref(), Some("host-token"));
    rig.host.claim_user_tab(
        USER_TAB,
        TabOwner::new("chat-a", "proj-1", Some("host-local".into())),
    );
    assert!(authorize_tab(&rig.host, USER_TAB, None, None, &env).is_err());
}

#[tokio::test]
async fn authenticate_remote_read_only_cannot_recover_host_tab() {
    let rig = TestRig::new().await;
    let env = authenticated_browser_env(&rig, "victim-device", true).await;
    assert_eq!(caller_principal(&env).as_deref(), Some("remote-read-only"));
    rig.host.claim_user_tab(
        USER_TAB,
        TabOwner::new("chat-a", "proj-1", Some("host-token".into())),
    );
    assert!(authorize_tab(&rig.host, USER_TAB, None, None, &env).is_err());
}

#[tokio::test]
async fn authenticate_same_chat_reconnect_recovers_with_new_device_id() {
    let rig = TestRig::new().await;
    let first = authenticated_browser_env(&rig, "first-device", false).await;
    rig.host.claim_user_tab(
        USER_TAB,
        TabOwner::new("chat-a", "proj-1", caller_principal(&first)),
    );
    let reconnect = authenticated_browser_env(&rig, "new-device", false).await;
    assert!(authorize_tab(&rig.host, USER_TAB, None, None, &reconnect).is_ok());
}

#[tokio::test]
async fn project_only_tab_enforces_principal_project_and_controls() {
    let rig = TestRig::new().await;
    let project_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let outside_file = outside_dir.path().join("secret.txt");
    std::fs::write(&outside_file, "secret").unwrap();
    rig.registry.set(
        vec![ProjectSummary {
            id: "proj-1".into(),
            name: "Project 1".into(),
            color: "blue".into(),
            path: Some(project_dir.path().to_string_lossy().into_owned()),
            is_archived: false,
            is_default: false,
        }],
        None,
    );
    rig.host.claim_user_tab(
        USER_TAB,
        TabOwner::project_only("proj-1", Some("host-token".into())),
    );
    rig.host.mark_test_tab_live(USER_TAB);

    let (owner_env, _tx1, _rx1) = rig.client_env(&[], Some("device-a"));
    let (other_principal_env, _tx2, _rx2) = rig.client_env(&[], Some("device-b"));

    // Owner with same principal passes authz on all control requests (reaches absent engine -> not_found).
    for (req_type, payload) in [
        ("browser_tab_history", json!({ "tabId": USER_TAB, "action": "back" })),
        ("browser_tab_zoom", json!({ "tabId": USER_TAB, "factor": 1.25 })),
        ("browser_tab_find", json!({ "tabId": USER_TAB, "query": "hello" })),
        ("browser_tab_clipboard", json!({ "tabId": USER_TAB, "action": "copy" })),
        ("browser_tab_dialog", json!({ "tabId": USER_TAB, "action": "accept" })),
        ("browser_tab_screenshot", json!({ "tabId": USER_TAB })),
    ] {
        let reply = handle_with_host("ok".into(), req_type, &payload, &owner_env, &rig.host).await;
        assert_eq!(err_code(&reply), "not_found", "owner passes authz on {req_type}");
        let refused = handle_with_host("no".into(), req_type, &payload, &other_principal_env, &rig.host).await;
        assert_eq!(err_code(&refused), "forbidden", "other principal refused on {req_type}");
    }

    // Cross-project claim on a project-only tab is forbidden.
    let cross_proj = handle_with_host(
        "cp".into(),
        "browser_tab_history",
        &json!({ "tabId": USER_TAB, "action": "reload", "projectId": "proj-2" }),
        &owner_env,
        &rig.host,
    )
    .await;
    assert_eq!(err_code(&cross_proj), "forbidden");

    // Oversized find query and outside-root file chooser are refused.
    let big_find = handle_with_host(
        "bf".into(),
        "browser_tab_find",
        &json!({ "tabId": USER_TAB, "query": "a".repeat(600) }),
        &owner_env,
        &rig.host,
    )
    .await;
    assert_eq!(err_code(&big_find), "unsupported");

    let escape_upload = handle_with_host(
        "eu".into(),
        "browser_tab_file_chooser",
        &json!({
            "tabId": USER_TAB,
            "selector": "input[type=file]",
            "files": [outside_file.to_string_lossy()]
        }),
        &owner_env,
        &rig.host,
    )
    .await;
    assert_eq!(err_code(&escape_upload), "forbidden");
}
