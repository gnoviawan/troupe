//! Antigravity Account usage. Troupe does not read Antigravity quotas: the
//! agent's Google login is only for the agent itself. Every Account reads as
//! `unavailable`, and the agent's own "Usage Limit Reached" turn tells the
//! host that a limit was reached.

use super::AntigravityAgentService;
use crate::acp::antigravity_accounts::usage::{AntigravityAccountUsage, AntigravityUsageStatus};

const USAGE_NOT_READ: &str =
    "Troupe does not read Antigravity limits. Antigravity says when a limit is reached.";

impl AntigravityAgentService {
    /// Usage of every Account, in list order: `unavailable` for each.
    pub fn accounts_usage(&self) -> Result<Vec<AntigravityAccountUsage>, String> {
        let snapshot = self.account_store()?.snapshot()?;
        let now = chrono::Utc::now().timestamp_millis();
        Ok(snapshot
            .profiles
            .iter()
            .map(|row| {
                AntigravityAccountUsage::status_only(
                    &row.id,
                    AntigravityUsageStatus::Unavailable,
                    USAGE_NOT_READ,
                    now,
                )
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
