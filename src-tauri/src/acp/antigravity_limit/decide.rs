//! Antigravity inputs for the shared limit decision.

use crate::acp::antigravity_accounts::usage::{AntigravityAccountUsage, AntigravityUsageStatus};
use crate::acp::antigravity_accounts::AntigravityAccountsSnapshot;
use crate::acp::claude_limit::decide::Candidate;

pub use crate::acp::claude_limit::decide::{
    decide, first_with_room, plan_resume, rank_alternatives, ChatHolder, IgnoreReason,
    LimitContext, LimitDecision, ResumePlan, SHORTEST_WINDOW,
};

/// Every Account of `snapshot` except `exclude`, with its readiness and the
/// percent used of its most used window.
#[must_use]
pub fn candidates(
    snapshot: &AntigravityAccountsSnapshot,
    usage: &[AntigravityAccountUsage],
    exclude: Option<&str>,
) -> Vec<Candidate> {
    snapshot
        .profiles
        .iter()
        .filter(|account| Some(account.id.as_str()) != exclude)
        .map(|account| {
            let read = usage.iter().find(|u| u.account_id == account.id);
            Candidate {
                account_id: account.id.clone(),
                ready: account.signed_in == Some(true)
                    && read.is_none_or(|u| u.status != AntigravityUsageStatus::SignedOut),
                percent: most_used(read).map(|(percent, _)| percent),
            }
        })
        .collect()
}

/// The percent used of the most used window, and when it resets.
#[must_use]
pub fn most_used(usage: Option<&AntigravityAccountUsage>) -> Option<(f64, Option<&str>)> {
    let usage = usage.filter(|u| u.status == AntigravityUsageStatus::Ok)?;
    usage
        .windows
        .iter()
        .reduce(|top, window| {
            if window.percent > top.percent {
                window
            } else {
                top
            }
        })
        .map(|window| (window.percent, window.resets_at.as_deref()))
}

#[cfg(test)]
#[path = "decide_tests.rs"]
mod tests;
