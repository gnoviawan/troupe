//! Antigravity Account usage: a cached read of each Account's model quota
//! buckets. The read exchanges the refresh token of that home's Google login
//! (`antigravity-acp/acp_token.json`) for an access token in memory, then asks
//! Cloud Code Assist (`loadCodeAssist`, `fetchAvailableModels`). The token
//! file is never written back, and Troupe never logs a token, a path with an
//! Account id, or an Account id.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::AntigravityAgentService;
use crate::acp::antigravity_accounts::usage::{
    usage_from_verdict, AntigravityAccountUsage, AntigravityUsageStatus, ProbeVerdict,
    USAGE_UNAVAILABLE,
};
use crate::acp::antigravity_accounts::{AntigravityAccountStore, BUILTIN_ACCOUNT_ID};

mod http;

/// A cached read is reused for this long.
const USAGE_MAX_AGE: Duration = Duration::from_secs(5 * 60);
/// A forced refresh still reuses a read this young.
const USAGE_MIN_AGE: Duration = Duration::from_secs(60);

/// Runs the usage probe for one Antigravity home and returns the verdict.
/// `builtin` is the host's own home, whose login may be in the macOS
/// keychain. The home directory must already exist.
#[async_trait::async_trait]
pub(crate) trait AntigravityUsageProbe: Send + Sync {
    async fn probe(&self, home: &Path, builtin: bool) -> ProbeVerdict;
}

/// The probe over the Cloud Code Assist HTTP API.
pub(super) struct SystemAntigravityUsageProbe;

#[async_trait::async_trait]
impl AntigravityUsageProbe for SystemAntigravityUsageProbe {
    async fn probe(&self, home: &Path, builtin: bool) -> ProbeVerdict {
        http::probe_home(home, builtin).await
    }
}

/// A cached read plus the time of the last probe, including a failed one.
/// `usage.checked_at` stays on the windows that are shown. `attempted_at`
/// is the throttle: a new probe waits 60 seconds after the last attempt.
pub(super) struct CachedUsage {
    usage: AntigravityAccountUsage,
    attempted_at: i64,
}

/// Latest read per Account id. Shared with in-flight probes so a cancelled
/// caller still leaves the completed read in the cache.
pub(super) type UsageCache = Arc<parking_lot::Mutex<HashMap<String, CachedUsage>>>;

/// In-flight usage probes, shared by concurrent callers of one account.
pub(super) type UsageInflight = std::sync::Arc<
    parking_lot::Mutex<
        HashMap<String, tokio::sync::watch::Receiver<Option<AntigravityAccountUsage>>>,
    >,
>;

impl AntigravityAgentService {
    /// Usage of every Account, in list order. Reads younger than 5 minutes
    /// come from the cache; `force` refreshes reads older than 60 seconds.
    /// A failed or unavailable read does not replace a previous good read.
    pub async fn accounts_usage(
        &self,
        force: bool,
    ) -> Result<Vec<AntigravityAccountUsage>, String> {
        let store = self.account_store()?;
        let snapshot = store.snapshot()?;
        let now = chrono::Utc::now().timestamp_millis();
        let max_age = if force { USAGE_MIN_AGE } else { USAGE_MAX_AGE };
        let max_age_ms = i64::try_from(max_age.as_millis()).unwrap_or(i64::MAX);
        let min_age_ms = i64::try_from(USAGE_MIN_AGE.as_millis()).unwrap_or(i64::MAX);
        let stale: Vec<(String, Option<PathBuf>, bool)> = {
            let cache = self.usage_cache.lock();
            snapshot
                .profiles
                .iter()
                .filter(|row| {
                    cache.get(&row.id).is_none_or(|entry| {
                        now.saturating_sub(entry.attempted_at) >= min_age_ms
                            && now.saturating_sub(entry.usage.checked_at) >= max_age_ms
                    })
                })
                .map(|row| {
                    let home = store
                        .config_dir(&row.id)
                        .or_else(AntigravityAccountStore::host_shared_dir);
                    let builtin = row.id == BUILTIN_ACCOUNT_ID;
                    (row.id.clone(), home, builtin)
                })
                .collect()
        };
        if !stale.is_empty() {
            let mut waiters = Vec::with_capacity(stale.len());
            let mut started = Vec::new();
            {
                let mut inflight = self.usage_inflight.lock();
                for (id, home, builtin) in stale {
                    if let Some(receiver) = inflight.get(&id) {
                        waiters.push(receiver.clone());
                        continue;
                    }
                    let (sender, receiver) = tokio::sync::watch::channel(None);
                    inflight.insert(id.clone(), receiver.clone());
                    waiters.push(receiver);
                    started.push((id, home, builtin, sender));
                }
            }
            for (id, home, builtin, sender) in started {
                let probe = Arc::clone(&self.usage_probe);
                let inflight = Arc::clone(&self.usage_inflight);
                let cache = Arc::clone(&self.usage_cache);
                tokio::spawn(async move {
                    let checked_at = chrono::Utc::now().timestamp_millis();
                    let verdict = match &home {
                        Some(home) => match ensure_home(home) {
                            Ok(()) => probe.probe(home, builtin).await,
                            Err(message) => ProbeVerdict::Failed(message),
                        },
                        None => ProbeVerdict::Unavailable(USAGE_UNAVAILABLE),
                    };
                    let usage = usage_from_verdict(&id, verdict, checked_at);
                    remember(&mut cache.lock(), usage.clone());
                    inflight.lock().remove(&id);
                    let _ = sender.send(Some(usage));
                });
            }
            let mut reads = Vec::with_capacity(waiters.len());
            for mut receiver in waiters {
                loop {
                    if let Some(usage) = receiver.borrow().clone() {
                        reads.push(usage);
                        break;
                    }
                    if receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
            let unavailable = reads
                .iter()
                .filter(|usage| usage.status == AntigravityUsageStatus::Unavailable)
                .count();
            let signed_out = reads
                .iter()
                .filter(|usage| usage.status == AntigravityUsageStatus::SignedOut)
                .count();
            let failed = reads
                .iter()
                .filter(|usage| usage.status == AntigravityUsageStatus::Error)
                .count();
            log::info!(
                "[antigravity-accounts] usage read for {} profiles ({unavailable} unavailable, {signed_out} signed out, {failed} failed)",
                reads.len()
            );
            let mut cache = self.usage_cache.lock();
            for usage in reads {
                remember(&mut cache, usage);
            }
        }
        let cache = self.usage_cache.lock();
        Ok(snapshot
            .profiles
            .iter()
            .filter_map(|row| cache.get(&row.id).map(|entry| entry.usage.clone()))
            .collect())
    }

    /// Replace the usage probe. Tests use this to report fixed windows.
    #[cfg(test)]
    pub(crate) fn set_usage_probe_for_test(&mut self, probe: Arc<dyn AntigravityUsageProbe>) {
        self.usage_probe = probe;
    }
}

/// Keep a good read when a later probe cannot reach Antigravity. A signed-out read
/// replaces it: the login is gone. A result older than the cached read is
/// dropped, so a slow probe cannot overwrite a newer one. A failed attempt
/// still moves `attempted_at`, which is the 60-second probe interval.
fn remember(cache: &mut HashMap<String, CachedUsage>, usage: AntigravityAccountUsage) {
    let attempt = usage.checked_at;
    if let Some(prev) = cache.get_mut(&usage.account_id) {
        if attempt < prev.usage.checked_at {
            return;
        }
        let failed = matches!(
            usage.status,
            AntigravityUsageStatus::Unavailable | AntigravityUsageStatus::Error
        );
        if failed && prev.usage.status == AntigravityUsageStatus::Ok {
            prev.attempted_at = prev.attempted_at.max(attempt);
            return;
        }
        *prev = CachedUsage {
            usage,
            attempted_at: attempt.max(prev.attempted_at),
        };
        return;
    }
    cache.insert(
        usage.account_id.clone(),
        CachedUsage {
            usage,
            attempted_at: attempt,
        },
    );
}

fn ensure_home(home: &Path) -> Result<(), &'static str> {
    if home.is_dir() {
        return Ok(());
    }
    match std::fs::create_dir_all(home) {
        Ok(()) => Ok(()),
        Err(error) => {
            log::warn!(
                "[antigravity-accounts] could not create Antigravity home before usage read: {:?}",
                error.kind()
            );
            Err("Could not create the Antigravity home folder")
        }
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
