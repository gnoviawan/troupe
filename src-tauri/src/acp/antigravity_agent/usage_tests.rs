use std::path::Path;
use std::sync::Arc;

use super::super::{AntigravityAgentService, AntigravityLoginStatus};
use super::{remember, AntigravityUsageProbe, CachedUsage};
use crate::acp::antigravity_accounts::usage::{
    AntigravityAccountUsage, AntigravityUsageStatus, ProbeVerdict, USAGE_UNAVAILABLE,
};
use crate::acp::antigravity_accounts::AntigravityAccountStore;

#[tokio::test]
async fn a_failed_probe_keeps_the_last_good_read() {
    let mut cache = std::collections::HashMap::new();
    let good = AntigravityAccountUsage {
        account_id: "default".to_string(),
        status: AntigravityUsageStatus::Ok,
        plan_type: Some("Google AI Pro".to_string()),
        windows: vec![],
        checked_at: 10,
        message: None,
    };
    remember(&mut cache, good.clone());
    remember(
        &mut cache,
        AntigravityAccountUsage::status_only(
            "default",
            AntigravityUsageStatus::Error,
            "Antigravity usage timed out",
            12,
        ),
    );
    let kept = cache.get("default").expect("cached read");
    assert_eq!(kept.usage, good);
    assert_eq!(kept.attempted_at, 12);
    remember(
        &mut cache,
        AntigravityAccountUsage {
            account_id: "default".to_string(),
            status: AntigravityUsageStatus::SignedOut,
            plan_type: None,
            windows: vec![],
            checked_at: 3,
            message: None,
        },
    );
    assert_eq!(
        cache.get("default").map(|entry| entry.usage.status),
        Some(AntigravityUsageStatus::Ok)
    );
    remember(
        &mut cache,
        AntigravityAccountUsage {
            account_id: "default".to_string(),
            status: AntigravityUsageStatus::SignedOut,
            plan_type: None,
            windows: vec![],
            checked_at: 13,
            message: None,
        },
    );
    assert_eq!(
        cache.get("default").map(|entry| entry.usage.status),
        Some(AntigravityUsageStatus::SignedOut)
    );
}

struct Boom;

#[async_trait::async_trait]
impl AntigravityUsageProbe for Boom {
    async fn probe(&self, _home: &Path, _builtin: bool) -> ProbeVerdict {
        ProbeVerdict::Failed("Antigravity usage timed out")
    }
}

struct SignedIn;

#[async_trait::async_trait]
impl AntigravityLoginStatus for SignedIn {
    async fn login_status(&self, _antigravity_home: Option<&Path>) -> Result<Option<bool>, String> {
        Ok(Some(true))
    }
}

#[tokio::test]
async fn the_service_keeps_a_good_read_when_the_next_probe_fails() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let store = Arc::new(AntigravityAccountStore::new(
        &state,
        root.path().join("shared"),
    ));
    let mut service = AntigravityAgentService::with_probe(store, Arc::new(SignedIn));
    service.usage_probe = Arc::new(Boom);
    service.usage_cache.lock().insert(
        "default".to_string(),
        CachedUsage {
            usage: AntigravityAccountUsage {
                account_id: "default".to_string(),
                status: AntigravityUsageStatus::Ok,
                plan_type: Some("Google AI Pro".to_string()),
                windows: vec![],
                checked_at: 0,
                message: None,
            },
            attempted_at: 0,
        },
    );
    let usage = service.accounts_usage(true).await.unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].status, AntigravityUsageStatus::Ok);
    assert_eq!(usage[0].plan_type.as_deref(), Some("Google AI Pro"));
}

struct SignsOut;

#[async_trait::async_trait]
impl AntigravityUsageProbe for SignsOut {
    async fn probe(&self, _home: &Path, _builtin: bool) -> ProbeVerdict {
        ProbeVerdict::SignedOut
    }
}

struct Gate {
    hits: std::sync::atomic::AtomicUsize,
    hold: Arc<tokio::sync::Mutex<()>>,
}

#[async_trait::async_trait]
impl AntigravityUsageProbe for Gate {
    async fn probe(&self, _home: &Path, _builtin: bool) -> ProbeVerdict {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _guard = self.hold.lock().await;
        ProbeVerdict::Unavailable(USAGE_UNAVAILABLE)
    }
}

#[tokio::test]
async fn concurrent_usage_reads_share_one_probe() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let store = Arc::new(AntigravityAccountStore::new(
        &state,
        root.path().join("shared"),
    ));
    let mut service = AntigravityAgentService::with_probe(store, Arc::new(SignedIn));
    let hold = Arc::new(tokio::sync::Mutex::new(()));
    let guard = hold.lock().await;
    let gate = Arc::new(Gate {
        hits: std::sync::atomic::AtomicUsize::new(0),
        hold: Arc::clone(&hold),
    });
    service.usage_probe = Arc::clone(&gate) as Arc<dyn AntigravityUsageProbe>;
    let first = service.accounts_usage(true);
    tokio::pin!(first);
    for _ in 0..50 {
        if gate.hits.load(std::sync::atomic::Ordering::SeqCst) >= 1 {
            break;
        }
        tokio::select! {
            result = &mut first => panic!("probe finished before it was shared: {result:?}"),
            () = tokio::task::yield_now() => {}
        }
    }
    assert_eq!(gate.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    let second = service.accounts_usage(true);
    drop(guard);
    let (first, second) = tokio::join!(first, second);
    assert!(first.is_ok());
    assert!(second.is_ok());
    assert_eq!(gate.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_cancelled_usage_read_still_caches_the_probe() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let store = Arc::new(AntigravityAccountStore::new(
        &state,
        root.path().join("shared"),
    ));
    let mut service = AntigravityAgentService::with_probe(store, Arc::new(SignedIn));
    let hold = Arc::new(tokio::sync::Mutex::new(()));
    let guard = hold.lock().await;
    let gate = Arc::new(Gate {
        hits: std::sync::atomic::AtomicUsize::new(0),
        hold: Arc::clone(&hold),
    });
    service.usage_probe = Arc::clone(&gate) as Arc<dyn AntigravityUsageProbe>;
    {
        let first = service.accounts_usage(true);
        tokio::pin!(first);
        for _ in 0..50 {
            if gate.hits.load(std::sync::atomic::Ordering::SeqCst) >= 1 {
                break;
            }
            tokio::select! {
                result = &mut first => panic!("probe finished before cancel: {result:?}"),
                () = tokio::task::yield_now() => {}
            }
        }
    }
    drop(guard);
    for _ in 0..50 {
        if !service.usage_cache.lock().is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(!service.usage_cache.lock().is_empty());
    let again = service.accounts_usage(true).await.unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(gate.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_recent_attempt_is_not_probed_again() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let store = Arc::new(AntigravityAccountStore::new(
        &state,
        root.path().join("shared"),
    ));
    let mut service = AntigravityAgentService::with_probe(store, Arc::new(SignedIn));
    service.usage_probe = Arc::new(SignsOut);
    service.usage_cache.lock().insert(
        "default".to_string(),
        CachedUsage {
            usage: AntigravityAccountUsage {
                account_id: "default".to_string(),
                status: AntigravityUsageStatus::Ok,
                plan_type: Some("Google AI Pro".to_string()),
                windows: vec![],
                checked_at: 0,
                message: None,
            },
            attempted_at: chrono::Utc::now().timestamp_millis(),
        },
    );
    let usage = service.accounts_usage(true).await.unwrap();
    assert_eq!(usage[0].status, AntigravityUsageStatus::Ok);
    assert_eq!(usage[0].plan_type.as_deref(), Some("Google AI Pro"));
}

/// Records whether each probe was for the built-in Account.
struct Builtins(parking_lot::Mutex<Vec<bool>>);

#[async_trait::async_trait]
impl AntigravityUsageProbe for Builtins {
    async fn probe(&self, _home: &Path, builtin: bool) -> ProbeVerdict {
        self.0.lock().push(builtin);
        ProbeVerdict::SignedOut
    }
}

#[tokio::test]
async fn only_the_builtin_account_is_probed_as_the_host_home() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let store = Arc::new(AntigravityAccountStore::new(
        &state,
        root.path().join("shared"),
    ));
    store.add("Work").unwrap();
    let mut service = AntigravityAgentService::with_probe(store, Arc::new(SignedIn));
    let probe = Arc::new(Builtins(parking_lot::Mutex::new(Vec::new())));
    service.usage_probe = Arc::clone(&probe) as Arc<dyn AntigravityUsageProbe>;
    let usage = service.accounts_usage(false).await.unwrap();
    assert_eq!(usage.len(), 2);
    assert!(usage
        .iter()
        .all(|read| read.status == AntigravityUsageStatus::SignedOut));
    let mut seen = probe.0.lock().clone();
    seen.sort_unstable();
    assert_eq!(seen, [false, true]);
}
