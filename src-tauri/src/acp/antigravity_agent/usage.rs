use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt, Shared};
use parking_lot::Mutex;
use serde_json::Value;

use super::AntigravityAgentService;
use crate::acp::antigravity_accounts::usage::{AntigravityAccountUsage, AntigravityUsageStatus};

mod check;
mod report;

pub use check::check_antigravity_usage;

type RunningCheck = Shared<BoxFuture<'static, AntigravityAccountUsage>>;

#[derive(Default)]
struct Entry {
    tier: Option<String>,
    last: Option<AntigravityAccountUsage>,
    running: Option<RunningCheck>,
}

#[derive(Default)]
pub(super) struct UsageBook {
    entries: Mutex<HashMap<String, Entry>>,
}

impl UsageBook {
    fn record_tier(&self, account_id: &str, tier: String) {
        self.entries
            .lock()
            .entry(account_id.to_string())
            .or_default()
            .tier = Some(tier);
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
        if let Some(running) = &entry.running {
            log::info!("[antigravity-usage] a check is already running; joining it");
            return running.clone();
        }
        let book = Arc::clone(self);
        let id = account_id.to_string();
        let task = tokio::spawn(async move {
            let usage = run.await;
            book.finish(&id, usage.clone());
            usage
        });
        let fallback = account_id.to_string();
        let running = async move {
            task.await.unwrap_or_else(|_| {
                AntigravityAccountUsage::status_only(
                    &fallback,
                    AntigravityUsageStatus::Error,
                    "The usage check stopped before it finished.",
                    chrono::Utc::now().timestamp_millis(),
                )
            })
        }
        .boxed()
        .shared();
        entry.running = Some(running.clone());
        running
    }

    fn finish(&self, account_id: &str, usage: AntigravityAccountUsage) {
        let mut entries = self.entries.lock();
        let entry = entries.entry(account_id.to_string()).or_default();
        entry.running = None;
        entry.last = Some(usage);
    }
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

    pub fn record_tier(&self, account_id: &str, tier: &str) {
        self.usage.record_tier(account_id, tier.to_string());
    }

    fn usage_book(&self) -> &Arc<UsageBook> {
        &self.usage
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
