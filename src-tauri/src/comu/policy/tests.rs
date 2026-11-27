use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::web::sink::AcpEvent;
use crate::web::EventSink;

use super::*;

#[derive(Default)]
struct RecordingSink(Mutex<Vec<(String, Value)>>);

impl EventSink for RecordingSink {
    fn emit(&self, event: &AcpEvent) {
        self.0
            .lock()
            .push((event.type_.to_string(), event.payload.clone()));
    }
}

#[tokio::test]
async fn defaults_ask_first_and_revision_cas_enforces_optimistic_concurrency() {
    let dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(RecordingSink::default());
    let service = ComuPolicyService::new(dir.path().to_path_buf(), vec![sink.clone()]);

    let initial = service.app_view(&ComuCallerScope::App).await.unwrap();
    assert_eq!(initial.revision, 0);
    assert!(initial.enabled);
    assert!(initial.cursor_enabled);
    assert!(initial.ask_before_driving_apps);
    assert!(initial.ask_before_foreground_desktop);
    assert!(initial.ask_before_clipboard_read);

    let updated = service
        .apply_app(
            &ComuCallerScope::App,
            0,
            &json!({ "op": "add_app", "app": "com.apple.calculator" }),
        )
        .await
        .unwrap();
    assert_eq!(updated.revision, 1);
    assert_eq!(updated.allowed_apps, vec!["com.apple.calculator"]);

    let conflict = service
        .apply_app(
            &ComuCallerScope::App,
            0,
            &json!({ "op": "set_enabled", "enabled": false }),
        )
        .await
        .unwrap_err();
    assert_eq!(conflict, ComuPolicyRefusal::RevisionConflict { current: 1 });
    assert_eq!(sink.0.lock().len(), 1);
    assert_eq!(sink.0.lock()[0].0, EVENT_COMU_POLICY_CHANGED);
}

#[tokio::test]
async fn rejects_troupe_self_entry_and_checks_project_allowlist() {
    let dir = tempfile::tempdir().unwrap();
    let service = ComuPolicyService::new(dir.path().to_path_buf(), Vec::new());

    let self_err = service
        .apply_app(
            &ComuCallerScope::App,
            0,
            &json!({ "op": "add_app", "app": "Troupe" }),
        )
        .await
        .unwrap_err();
    assert_eq!(self_err.reason(), "invalid_app_id");

    service
        .record_project_approval("proj-alpha", "Calculator")
        .await;
    assert!(
        service
            .is_app_allowlisted("proj-alpha", "com.apple.calculator", "Calculator")
            .await
    );
    assert!(
        !service
            .is_app_allowlisted("proj-beta", "com.apple.calculator", "Calculator")
            .await
    );
}
