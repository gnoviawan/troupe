#![allow(clippy::unwrap_used, clippy::expect_used)]
//! `get_acp_agent_overlay` answers with the catalog service's overlay
//! presentation, the same value the desktop reads over its channel carrier.

use super::tests::{route, test_env, test_outbound};
use super::*;
use crate::acp::AcpCatalogService;

async fn send(catalog: Option<Arc<AcpCatalogService>>) -> WsReply {
    let relay = Arc::new(WsRelaySink::new());
    let acp = Arc::new(AcpManager::new(vec![]));
    let (tx, _rx) = test_outbound();
    let subs = Arc::new(Subscriptions::default());
    let registry = Arc::new(ProjectRegistry::new());
    route(
        r#"{"id":"r1","type":"get_acp_agent_overlay","payload":{}}"#,
        &HandlerEnv {
            acp_catalog: catalog,
            ..test_env(&acp, &relay, &registry, &tx, &subs)
        },
    )
    .await
}

#[tokio::test]
async fn an_offline_host_answers_with_the_bundled_overlay() {
    let root = tempfile::tempdir().unwrap();
    let catalog = AcpCatalogService::open(root.path().join("catalog"))
        .await
        .unwrap();

    let reply = send(Some(Arc::clone(&catalog))).await;

    assert!(reply.ok);
    let payload = reply.payload.unwrap();
    assert_eq!(payload["source"], json!("bundled"));
    let claude = payload["overrides"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "claude-acp")
        .cloned();
    assert_eq!(
        claude,
        Some(
            json!({ "id": "claude-acp", "brandColor": { "light": "#C15F3C", "dark": "#D97757" } })
        )
    );
    assert_eq!(
        payload,
        serde_json::to_value(catalog.agent_overlay().await).unwrap()
    );
}

#[tokio::test]
async fn a_host_without_a_catalog_store_answers_unavailable() {
    let reply = send(None).await;
    assert_eq!(reply.err.unwrap().code, "ACP_CATALOG_UNAVAILABLE");
}
