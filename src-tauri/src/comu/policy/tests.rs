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

#[tokio::test]
async fn isolates_case_variant_project_ids_and_fails_closed_on_future_schema() {
    let dir = tempfile::tempdir().unwrap();
    let service = ComuPolicyService::new(dir.path().to_path_buf(), Vec::new());

    assert_ne!(
        super::store::encode_project_storage_key("proj-alpha"),
        super::store::encode_project_storage_key("PROJ-ALPHA")
    );

    service
        .record_project_approval("proj-alpha", "com.apple.calculator")
        .await;
    assert!(
        service
            .is_app_allowlisted("proj-alpha", "com.apple.calculator", "Calculator")
            .await
    );
    assert!(
        !service
            .is_app_allowlisted("PROJ-ALPHA", "com.apple.calculator", "Calculator")
            .await
    );

    std::fs::write(
        dir.path().join("app.json"),
        br#"{"version":99,"enabled":true}"#,
    )
    .unwrap();
    let future_policy = service.read_app_policy().await;
    assert!(future_policy.read_only);
    assert!(!future_policy.enabled);

    // Distinct project IDs `_` (`_5f.json`) and `_5f` (`_5f5f.json`) must never alias.
    service
        .record_project_approval("_", "com.apple.calculator")
        .await;
    assert!(
        service
            .is_app_allowlisted("_", "com.apple.calculator", "")
            .await
    );
    assert!(
        !service
            .is_app_allowlisted("_5f", "com.apple.calculator", "")
            .await
    );
}


#[tokio::test]
async fn screen_indicator_defaults_on_toggles_and_survives_reload() {
    let dir = tempfile::tempdir().unwrap();
    let service = ComuPolicyService::new(dir.path().to_path_buf(), Vec::new());
    assert!(service.read_app_policy().await.screen_indicator_enabled);

    let updated = service
        .apply_app(
            &ComuCallerScope::App,
            0,
            &json!({ "op": "set_screen_indicator_enabled", "enabled": false }),
        )
        .await
        .unwrap();
    assert!(!updated.screen_indicator_enabled);
    assert_eq!(updated.revision, 1);

    let reopened = ComuPolicyService::new(dir.path().to_path_buf(), Vec::new());
    assert!(!reopened.read_app_policy().await.screen_indicator_enabled);
}

/// A policy file written before the screen indicator existed keeps it on.
#[tokio::test]
async fn app_policy_file_without_screen_indicator_field_defaults_it_on() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("app.json"),
        br#"{"version":1,"revision":4,"enabled":true,"cursorEnabled":false}"#,
    )
    .unwrap();
    let service = ComuPolicyService::new(dir.path().to_path_buf(), Vec::new());
    let policy = service.read_app_policy().await;
    assert!(!policy.read_only);
    assert!(!policy.cursor_enabled);
    assert!(policy.screen_indicator_enabled);
}

#[test]
fn app_identities_fold_case_except_for_executable_paths() {
    assert!(app_entry_matches("com.Apple.Calculator", "com.apple.calculator", ""));
    assert!(app_entry_matches(
        "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
        "microsoft.windowscalculator_8wekyb3d8bbwe!app",
        ""
    ));
    assert!(app_entry_matches("Helper", "", "helper"));
    assert!(app_entry_matches("/opt/Editor", "/opt/Editor", ""));
    // Case-sensitive filesystems can hold both paths as different programs.
    assert_eq!(
        app_entry_matches("/opt/Editor", "/opt/editor", ""),
        cfg!(windows)
    );
    assert!(app_entry_matches("C:\\Tools\\Helper.exe", "C:\\Tools\\Helper.exe", ""));
}
