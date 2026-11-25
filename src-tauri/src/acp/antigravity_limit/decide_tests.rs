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
use crate::acp::limit_runtime::roomiest_account;

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

fn no_quota(id: &str) -> AntigravityAccountUsage {
    AntigravityAccountUsage::status_only(id, AntigravityUsageStatus::Unavailable, "not read", 1)
}

fn snapshot_of(profiles: Vec<AntigravityAccountView>) -> AntigravityAccountsSnapshot {
    AntigravityAccountsSnapshot {
        profiles,
        default_account_id: "default".to_string(),
        project_profiles: HashMap::new().into_iter().collect(),
        chat_profiles: HashMap::new().into_iter().collect(),
        auto_switch: true,
    }
}

#[test]
fn with_no_quota_the_signed_in_accounts_rank_in_list_order() {
    let snapshot = snapshot_of(vec![
        account("default", Some(false)),
        account("work", Some(true)),
        account("unknown", None),
        account("spare", Some(true)),
        account("side", Some(true)),
    ]);
    let usage: Vec<_> = ["default", "work", "unknown", "spare", "side"]
        .into_iter()
        .map(no_quota)
        .collect();
    let all = candidates(&snapshot, &usage, Some("work"));
    let ready: Vec<(&str, bool, Option<f64>)> = all
        .iter()
        .map(|c| (c.account_id.as_str(), c.ready, c.percent))
        .collect();
    assert_eq!(
        ready,
        vec![
            ("default", false, None),
            ("unknown", false, None),
            ("spare", true, None),
            ("side", true, None),
        ]
    );
    assert_eq!(
        super::rank_alternatives(&all),
        vec!["spare".to_string(), "side".to_string()]
    );
    assert_eq!(most_used(Some(&usage[3])), None);
}

#[test]
fn with_no_quota_a_new_chat_takes_the_first_signed_in_account_not_at_a_limit() {
    let now = at(1_000);
    let snapshot = snapshot_of(vec![
        account("default", Some(false)),
        account("work", Some(true)),
        account("spare", Some(true)),
        account("side", Some(true)),
    ]);
    let usage: Vec<_> = ["default", "work", "spare", "side"]
        .into_iter()
        .map(no_quota)
        .collect();
    let all = candidates(&snapshot, &usage, None);
    assert_eq!(
        roomiest_account(&all, &HashMap::new(), now),
        Some("work".to_string())
    );
    let work_limited = HashMap::from([("work".to_string(), at(9_000))]);
    assert_eq!(
        roomiest_account(&all, &work_limited, now),
        Some("spare".to_string())
    );
    assert_eq!(
        roomiest_account(&all, &work_limited, at(9_000)),
        Some("work".to_string())
    );
    let all_limited = HashMap::from([
        ("work".to_string(), at(9_000)),
        ("spare".to_string(), at(9_000)),
        ("side".to_string(), at(9_000)),
    ]);
    assert_eq!(roomiest_account(&all, &all_limited, now), None);
}

#[test]
fn with_no_quota_a_chat_at_its_limit_moves_to_the_next_account_in_list_order() {
    let now = at(1_000);
    let snapshot = snapshot_of(vec![
        account("default", Some(true)),
        account("work", Some(true)),
        account("spare", Some(true)),
    ]);
    let usage: Vec<_> = ["default", "work", "spare"]
        .into_iter()
        .map(no_quota)
        .collect();
    let ranked = super::rank_alternatives(&candidates(&snapshot, &usage, Some("work")));
    let decide_at = |limited_until: &HashMap<String, SystemTime>, resets_at| {
        decide(&LimitContext {
            agent_account: "work",
            chat_account: "work",
            auto_switch: true,
            candidates: &ranked,
            limited_until,
            resets_at,
            now,
        })
    };
    let work_limited = HashMap::from([("work".to_string(), at(9_000))]);
    assert_eq!(
        decide_at(&work_limited, None),
        LimitDecision::Switch {
            to: "default".to_string()
        }
    );
    let default_limited = HashMap::from([
        ("work".to_string(), at(9_000)),
        ("default".to_string(), at(9_000)),
    ]);
    assert_eq!(
        decide_at(&default_limited, None),
        LimitDecision::Switch {
            to: "spare".to_string()
        }
    );
    let all_limited = HashMap::from([
        ("work".to_string(), at(9_000)),
        ("default".to_string(), at(9_000)),
        ("spare".to_string(), at(9_000)),
    ]);
    assert_eq!(
        decide_at(&all_limited, Some(at(5_000))),
        LimitDecision::ResumeAt { at: at(5_000) }
    );
    assert_eq!(
        decide_at(&all_limited, None),
        LimitDecision::Ignore {
            reason: IgnoreReason::NoRoom
        }
    );
}
