use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    candidates, decide, most_used, Candidate, ChatHolder, IgnoreReason, LimitContext,
    LimitDecision, ResumePlan,
};
use crate::acp::antigravity_accounts::usage::{
    AntigravityAccountUsage, AntigravityUsageStatus, AntigravityUsageWindow,
};
use crate::acp::antigravity_accounts::{AntigravityAccountView, AntigravityAccountsSnapshot};

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn window(percent: f64, resets_at: &str) -> AntigravityUsageWindow {
    AntigravityUsageWindow {
        kind: "model".to_string(),
        label: "Gemini 3 Pro".to_string(),
        model_id: "gemini-3-pro".to_string(),
        percent,
        resets_at: Some(resets_at.to_string()),
    }
}

fn usage(id: &str, percent: f64) -> AntigravityAccountUsage {
    AntigravityAccountUsage {
        account_id: id.to_string(),
        status: AntigravityUsageStatus::Ok,
        plan_type: Some("Google AI Pro".to_string()),
        windows: vec![window(percent, "2099-01-01T00:00:00Z")],
        checked_at: 1,
        message: None,
    }
}

fn account(id: &str, signed_in: Option<bool>) -> AntigravityAccountView {
    AntigravityAccountView {
        id: id.to_string(),
        name: id.to_string(),
        builtin: id == "default",
        signed_in,
    }
}

#[test]
fn the_signed_in_account_with_the_most_room_is_first() {
    let snapshot = AntigravityAccountsSnapshot {
        profiles: vec![
            account("work", Some(true)),
            account("spare", Some(true)),
            account("full", Some(true)),
            account("out", Some(false)),
        ],
        default_account_id: "default".to_string(),
        project_profiles: HashMap::new().into_iter().collect(),
        chat_profiles: HashMap::new().into_iter().collect(),
        auto_switch: true,
    };
    let usage = vec![
        usage("work", 40.0),
        usage("spare", 10.0),
        usage("full", 100.0),
    ];
    let ranked = super::rank_alternatives(&candidates(&snapshot, &usage, Some("work")));
    assert_eq!(ranked, vec!["spare".to_string()]);
    assert_eq!(
        most_used(Some(&usage[1])),
        Some((10.0, Some("2099-01-01T00:00:00Z")))
    );
}

#[test]
fn a_chat_at_its_limit_switches_or_waits_for_the_reset() {
    let now = at(1_000);
    let limited = HashMap::from([("spare".to_string(), at(9_000))]);
    let switch = decide(&LimitContext {
        agent_account: "work",
        chat_account: "work",
        auto_switch: true,
        candidates: &["spare".to_string()],
        limited_until: &HashMap::new(),
        resets_at: Some(at(5_000)),
        now,
    });
    assert_eq!(
        switch,
        LimitDecision::Switch {
            to: "spare".to_string()
        }
    );
    let wait = decide(&LimitContext {
        agent_account: "work",
        chat_account: "work",
        auto_switch: true,
        candidates: &["spare".to_string()],
        limited_until: &limited,
        resets_at: Some(at(5_000)),
        now,
    });
    assert_eq!(wait, LimitDecision::ResumeAt { at: at(5_000) });
    let off = decide(&LimitContext {
        agent_account: "work",
        chat_account: "work",
        auto_switch: false,
        candidates: &["spare".to_string()],
        limited_until: &HashMap::new(),
        resets_at: Some(at(5_000)),
        now,
    });
    assert_eq!(
        off,
        LimitDecision::Ignore {
            reason: IgnoreReason::AutoSwitchOff
        }
    );
    let restart = decide(&LimitContext {
        agent_account: "work",
        chat_account: "spare",
        auto_switch: false,
        candidates: &[],
        limited_until: &HashMap::new(),
        resets_at: None,
        now,
    });
    assert_eq!(restart, LimitDecision::Restart);
    assert_eq!(
        super::plan_resume(ChatHolder::Nobody, Some("work"), Some("work")),
        ResumePlan::Start
    );
    let _ = Candidate {
        account_id: "spare".to_string(),
        ready: true,
        percent: Some(10.0),
    };
}
