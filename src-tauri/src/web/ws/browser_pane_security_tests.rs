#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::browser_automation::engine::chromium::perimeter::{InternalAddresses, Perimeter};

#[tokio::test]
async fn tab_controls_refuse_other_connections_before_engine_access() {
    let acp = Arc::new(AcpManager::new(vec![]));
    let relay = Arc::new(WsRelaySink::new());
    let registry = Arc::new(ProjectRegistry::new());
    let (owner, _owner_rx) = super::super::tests::test_outbound();
    let (other, _other_rx) = super::super::tests::test_outbound();
    let subscriptions = Arc::new(Subscriptions::default());
    let owner_env = super::super::tests::test_env(&acp, &relay, &registry, &owner, &subscriptions);
    let other_env = super::super::tests::test_env(&acp, &relay, &registry, &other, &subscriptions);
    let perimeter = Perimeter::start(InternalAddresses::Deny).await.unwrap();
    let host = BrowserHost::new(
        "unused".into(),
        "unused".into(),
        "unused".into(),
        false,
        perimeter,
        relay,
    );
    host.claim_user_tab("tab", "project");
    host.grant("tab", &owner);
    for operation in ["attach", "navigate", "input", "close", "resize"] {
        let err = known_tab(&host, "tab", &other_env).err().unwrap();
        assert!(matches!(err.0, WsErrorCode::Forbidden), "{operation}");
        // The owner passes authorization and reaches the absent engine.
        let err = known_tab(&host, "tab", &owner_env).err().unwrap();
        assert!(matches!(err.0, WsErrorCode::NotFound), "{operation}");
    }
}
