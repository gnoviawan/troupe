use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::super::{AntigravityAgentService, AntigravityLaunchAuth};
use super::antigravity_tier_name;
use crate::acp::antigravity_accounts::usage::{AntigravityAccountUsage, AntigravityUsageStatus};

fn checked(account_id: &str, percent: f64) -> AntigravityAccountUsage {
    AntigravityAccountUsage {
        account_id: account_id.to_string(),
        status: AntigravityUsageStatus::Ok,
        plan_type: None,
        windows: vec![
            crate::acp::antigravity_accounts::usage::AntigravityUsageWindow {
                kind: "model".to_string(),
                label: "Gemini Models · Weekly".to_string(),
                model_id: "Gemini Models · Weekly".to_string(),
                percent,
                resets_at: None,
            },
        ],
        checked_at: 1_700_000_000_000,
        message: None,
    }
}

#[test]
fn every_account_reads_not_checked_until_someone_asks() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let work = service.insert_profile("Work").unwrap().id;

    let usage = service.accounts_usage().unwrap();

    let ids: Vec<&str> = usage.iter().map(|read| read.account_id.as_str()).collect();
    assert_eq!(ids, vec!["default", work.as_str()]);
    for read in &usage {
        assert_eq!(read.status, AntigravityUsageStatus::NotChecked);
        assert_eq!(read.plan_type, None);
        assert!(read.windows.is_empty());
        assert_eq!(read.checked_at, 0);
        assert_eq!(read.message, None);
    }
}

fn account(id: &str) -> Option<AntigravityLaunchAuth> {
    Some(AntigravityLaunchAuth::Account(id.to_string()))
}

#[test]
fn the_tier_from_a_chat_session_is_the_plan_until_a_session_names_none() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let plan =
        |service: &AntigravityAgentService| service.accounts_usage().unwrap()[0].plan_type.clone();

    service.record_session_tier(account("default"), Some("Google AI Pro".to_string()));
    assert_eq!(plan(&service).as_deref(), Some("Google AI Pro"));
    assert_eq!(
        service.accounts_usage().unwrap()[0].status,
        AntigravityUsageStatus::NotChecked
    );

    service.record_session_tier(Some(AntigravityLaunchAuth::Fixed), None);
    service.record_session_tier(None, None);
    assert_eq!(plan(&service).as_deref(), Some("Google AI Pro"));

    service.record_session_tier(account("default"), None);
    assert_eq!(plan(&service), None);
}

#[test]
fn the_tier_name_comes_from_the_antigravity_meta_only() {
    let meta = |value: serde_json::Value| value.as_object().unwrap().clone();

    assert_eq!(
        antigravity_tier_name(&meta(serde_json::json!({
            "antigravity": { "tier": { "name": "Google AI Pro", "id": "g1-pro-tier", "upgradeUri": "" } }
        }))),
        Some("Google AI Pro".to_string())
    );
    assert_eq!(
        antigravity_tier_name(&meta(serde_json::json!({
            "antigravity": { "tier": { "name": " ", "id": "free-tier" } }
        }))),
        None
    );
    assert_eq!(
        antigravity_tier_name(&meta(serde_json::json!({ "tier": { "name": "Pro" } }))),
        None
    );
}

#[tokio::test]
async fn a_second_check_joins_the_running_one_and_the_result_is_kept() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    service
        .usage_book()
        .record_tier("default", Some("Google AI Ultra".to_string()));
    let book = Arc::clone(service.usage_book());
    let runs = Arc::new(AtomicUsize::new(0));
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let first_runs = Arc::clone(&runs);
    let first = book.join_or_start("default", async move {
        first_runs.fetch_add(1, Ordering::SeqCst);
        released.await.unwrap();
        checked("default", 9.0)
    });
    let second_runs = Arc::clone(&runs);
    let second = book.join_or_start("default", async move {
        second_runs.fetch_add(1, Ordering::SeqCst);
        checked("default", 50.0)
    });

    release.send(()).unwrap();
    let (first, second) = tokio::join!(first, second);

    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(first, checked("default", 9.0));
    assert_eq!(second, first);
    let kept = &service.accounts_usage().unwrap()[0];
    assert_eq!(kept.windows, checked("default", 9.0).windows);
    assert_eq!(kept.plan_type.as_deref(), Some("Google AI Ultra"));
}

#[tokio::test]
async fn a_check_after_the_last_one_finished_runs_again() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let book = Arc::clone(service.usage_book());

    book.join_or_start("default", async { checked("default", 9.0) })
        .await;
    let again = book
        .join_or_start("default", async { checked("default", 30.0) })
        .await;

    assert_eq!(again, checked("default", 30.0));
    assert_eq!(
        service.accounts_usage().unwrap()[0].windows[0].percent,
        30.0
    );
}

#[tokio::test]
async fn a_panicking_check_answers_an_error_and_the_next_click_runs_again() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let book = Arc::clone(service.usage_book());

    let broken = || -> AntigravityAccountUsage { panic!("the check broke") };
    let failed = book.join_or_start("default", async move { broken() }).await;
    let again = book
        .join_or_start("default", async { checked("default", 30.0) })
        .await;

    assert_eq!(failed.status, AntigravityUsageStatus::Error);
    assert_eq!(
        failed.message.as_deref(),
        Some("The usage check stopped before it finished.")
    );
    assert_eq!(again, checked("default", 30.0));
    assert_eq!(service.usage_checks_started(), 2);
}

#[tokio::test]
async fn a_check_that_finishes_after_its_account_was_forgotten_is_not_kept() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let book = Arc::clone(service.usage_book());
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let running = book.join_or_start("default", async move {
        released.await.unwrap();
        checked("default", 9.0)
    });

    book.forget("default");
    release.send(()).unwrap();

    assert_eq!(running.await, checked("default", 9.0));
    assert_eq!(
        service.accounts_usage().unwrap()[0].status,
        AntigravityUsageStatus::NotChecked
    );
}

#[tokio::test]
async fn a_login_change_drops_the_tier_and_the_last_check() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let book = Arc::clone(service.usage_book());
    book.observe_login("default", Some(true));
    book.record_tier("default", Some("Google AI Pro".to_string()));
    book.join_or_start("default", async { checked("default", 9.0) })
        .await;

    book.observe_login("default", None);
    book.observe_login("default", Some(true));
    let kept = &service.accounts_usage().unwrap()[0];
    assert_eq!(kept.status, AntigravityUsageStatus::Ok);
    assert_eq!(kept.plan_type.as_deref(), Some("Google AI Pro"));

    book.observe_login("default", Some(false));
    let dropped = &service.accounts_usage().unwrap()[0];
    assert_eq!(dropped.status, AntigravityUsageStatus::NotChecked);
    assert_eq!(dropped.plan_type, None);
}

#[tokio::test]
async fn removing_an_account_drops_its_entry() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let work = service.insert_profile("Work").unwrap().id;
    service
        .usage_book()
        .record_tier(&work, Some("Google AI Pro".to_string()));

    service.remove_account(&work).await.unwrap();

    assert!(!service.usage_book().entries.lock().contains_key(&work));
}
