//! Antigravity inputs for the shared limit decision.

use crate::acp::antigravity_accounts::AntigravityAccountsSnapshot;
use crate::acp::claude_limit::decide::Candidate;

pub use crate::acp::claude_limit::decide::{
    decide, first_with_room, plan_resume, rank_alternatives, ChatHolder, IgnoreReason,
    LimitContext, LimitDecision, ResumePlan, SHORTEST_WINDOW,
};

/// Every Account of `snapshot` except `exclude`, ready when its login is
/// confirmed. A usage check is only shown to the user, so no Account has a
/// known percent and the ranking keeps list order.
#[must_use]
pub fn candidates(snapshot: &AntigravityAccountsSnapshot, exclude: Option<&str>) -> Vec<Candidate> {
    snapshot
        .profiles
        .iter()
        .filter(|account| Some(account.id.as_str()) != exclude)
        .map(|account| Candidate {
            account_id: account.id.clone(),
            ready: account.signed_in == Some(true),
            percent: None,
        })
        .collect()
}

#[cfg(test)]
#[path = "decide_tests.rs"]
mod tests;
