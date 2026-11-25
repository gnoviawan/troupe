use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use futures::future::{BoxFuture, FutureExt, Shared};
use parking_lot::Mutex;
use serde_json::Value;

use super::{AntigravityAgentService, AntigravityLaunchAuth};
use crate::acp::antigravity_accounts::usage::{AntigravityAccountUsage, AntigravityUsageStatus};

mod check;
mod report;

pub use check::check_antigravity_usage;

type RunningCheck = Shared<BoxFuture<'static, AntigravityAccountUsage>>;

#[derive(Default)]
struct Entry {
    tier: Option<String>,
    last: Option<AntigravityAccountUsage>,
    /// The number of the running check, and the future every caller joins.
    running: Option<(u64, RunningCheck)>,
    /// The login state the Account had when it was last listed.
    login: Option<bool>,
}

/// What the host knows of each Account's usage: the tier the agent named and
/// the last check. Kept in memory only, and shown to the user only. Limit
/// handling never reads it.
#[derive(Default)]
pub(super) struct UsageBook {
    entries: Mutex<HashMap<String, Entry>>,
    started: AtomicU64,
}

impl UsageBook {
    fn record_tier(&self, account_id: &str, tier: Option<String>) {
        self.entries
            .lock()
            .entry(account_id.to_string())
            .or_default()
            .tier = tier;
    }

    /// Drop the tier and the last check. A check running now still answers
    /// its callers, but its result is not kept.
    pub(super) fn forget(&self, account_id: &str) {
        self.entries.lock().remove(account_id);
    }

    /// Drop the tier and the last check when the login changed since the
    /// Account was last listed. A running check stays: its own sign-in read
    /// is often what lists the change.
    pub(super) fn observe_login(&self, account_id: &str, signed_in: Option<bool>) {
        let Some(signed_in) = signed_in else {
            return;
        };
        let mut entries = self.entries.lock();
        let entry = entries.entry(account_id.to_string()).or_default();
        if entry.login.is_some_and(|before| before != signed_in) {
            log::info!("[antigravity-usage] the login changed; the last check is dropped");
            entry.tier = None;
            entry.last = None;
        }
        entry.login = Some(signed_in);
    }

    fn read(&self, account_id: &str) -> AntigravityAccountUsage {
        let entries = self.entries.lock();
        let entry = entries.get(account_id);
        let mut usage = entry
            .and_then(|entry| entry.last.clone())
            .unwrap_or_else(|| AntigravityAccountUsage {
                account_id: account_id.to_string(),
                status: AntigravityUsageStatus::NotChecked,
                plan_type: None,
                windows: Vec::new(),
                checked_at: 0,
                message: None,
            });
        usage.plan_type = entry.and_then(|entry| entry.tier.clone());
        usage
    }

    fn join_or_start<F>(self: &Arc<Self>, account_id: &str, run: F) -> RunningCheck
    where
        F: Future<Output = AntigravityAccountUsage> + Send + 'static,
    {
        let mut entries = self.entries.lock();
        let entry = entries.entry(account_id.to_string()).or_default();
        if let Some((_, running)) = &entry.running {
            log::info!("[antigravity-usage] a check is already running; joining it");
            return running.clone();
        }
        let number = self.started.fetch_add(1, Ordering::Relaxed);
        let book = Arc::clone(self);
        let id = account_id.to_string();
        let task = tokio::spawn(async move {
            let usage = AssertUnwindSafe(run)
                .catch_unwind()
                .await
                .unwrap_or_else(|_| {
                    log::error!("[antigravity-usage] the check panicked");
                    stopped(&id)
                });
            book.finish(&id, number, &usage);
            usage
        });
        let book = Arc::downgrade(self);
        let id = account_id.to_string();
        let running = async move {
            task.await
                .unwrap_or_else(|_| finish_stopped(&book, &id, number))
        }
        .boxed()
        .shared();
        entry.running = Some((number, running.clone()));
        running
    }

    /// Keep `usage` as the last check, unless the Account was forgotten or a
    /// newer check took its place while this one ran.
    fn finish(&self, account_id: &str, number: u64, usage: &AntigravityAccountUsage) {
        let mut entries = self.entries.lock();
        let Some(entry) = entries.get_mut(account_id) else {
            return;
        };
        if entry.running.as_ref().map(|(running, _)| *running) != Some(number) {
            return;
        }
        entry.running = None;
        entry.last = Some(usage.clone());
    }
}

fn stopped(account_id: &str) -> AntigravityAccountUsage {
    AntigravityAccountUsage::status_only(
        account_id,
        AntigravityUsageStatus::Error,
        "The usage check stopped before it finished.",
        chrono::Utc::now().timestamp_millis(),
    )
}

fn finish_stopped(
    book: &Weak<UsageBook>,
    account_id: &str,
    number: u64,
) -> AntigravityAccountUsage {
    let usage = stopped(account_id);
    if let Some(book) = book.upgrade() {
        book.finish(account_id, number, &usage);
    }
    usage
}

#[must_use]
pub fn antigravity_tier_name(meta: &serde_json::Map<String, Value>) -> Option<String> {
    meta.get("antigravity")?
        .get("tier")?
        .get("name")?
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

impl AntigravityAgentService {
    pub fn accounts_usage(&self) -> Result<Vec<AntigravityAccountUsage>, String> {
        let snapshot = self.account_store()?.snapshot()?;
        Ok(snapshot
            .profiles
            .iter()
            .map(|row| self.usage.read(&row.id))
            .collect())
    }

    /// Record the tier a `session/new` reply named for the Account the agent
    /// runs on. A reply without one clears it.
    pub fn record_session_tier(&self, auth: Option<AntigravityLaunchAuth>, tier: Option<String>) {
        if let Some(AntigravityLaunchAuth::Account(account_id)) = auth {
            self.usage.record_tier(&account_id, tier);
        }
    }

    fn usage_book(&self) -> &Arc<UsageBook> {
        &self.usage
    }

    #[cfg(test)]
    pub(crate) fn usage_checks_started(&self) -> u64 {
        self.usage.started.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
