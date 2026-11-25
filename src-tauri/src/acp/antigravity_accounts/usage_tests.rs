use serde_json::json;

use super::*;

const NOW: i64 = 1_700_000_000_000;

#[test]
fn usage_serializes_with_the_renderer_field_names() {
    let usage = AntigravityAccountUsage::status_only(
        "acct",
        AntigravityUsageStatus::Unavailable,
        "not read",
        NOW,
    );
    assert_eq!(
        serde_json::to_value(&usage).unwrap(),
        json!({
            "accountId": "acct",
            "status": "unavailable",
            "planType": null,
            "windows": [],
            "checkedAt": NOW,
            "message": "not read"
        })
    );
}

const BODY: &str = "Usage Limit Reached\n\nYou have reached your current quota for this period.";

#[test]
fn a_quota_message_with_a_relative_reset_is_read() {
    let text = format!("{BODY} Your limit will reset in 2 hours, 30 minutes.");
    let hit = antigravity_quota_message_at(&text, NOW).unwrap();
    assert_eq!(hit.reset_at, Some(NOW + (2 * 60 + 30) * 60_000));

    let text = format!("{BODY} Your limit will reset in 1 day, 1 hour, 1 minute, 1 second.");
    let hit = antigravity_quota_message_at(&text, NOW).unwrap();
    assert_eq!(
        hit.reset_at,
        Some(NOW + 86_400_000 + 3_600_000 + 60_000 + 1_000)
    );

    let text = format!("  {BODY} Your limit will reset in 45 seconds.  \n");
    let hit = antigravity_quota_message_at(&text, NOW).unwrap();
    assert_eq!(hit.reset_at, Some(NOW + 45_000));
}

#[test]
fn a_quota_message_with_an_absolute_reset_is_read() {
    let text = format!("{BODY} Your limit will reset on Jan 5, 2026 14:30 UTC.");
    let hit = antigravity_quota_message_at(&text, NOW).unwrap();
    let expected = chrono::DateTime::parse_from_rfc3339("2026-01-05T14:30:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(hit.reset_at, Some(expected));

    let text = format!("{BODY} Your limit will reset on Dec 25, 2026 09:05 UTC.");
    let expected = chrono::DateTime::parse_from_rfc3339("2026-12-25T09:05:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        antigravity_quota_message_at(&text, NOW).unwrap().reset_at,
        Some(expected)
    );
}

#[test]
fn a_quota_message_without_a_readable_reset_has_no_reset() {
    assert_eq!(
        antigravity_quota_message_at(BODY, NOW),
        Some(AntigravityQuotaHit { reset_at: None })
    );
    let text = format!("{BODY} Your limit will reset in a while.");
    assert_eq!(
        antigravity_quota_message_at(&text, NOW).unwrap().reset_at,
        None
    );
    let text = format!("{BODY} Your limit will reset on someday UTC.");
    assert_eq!(
        antigravity_quota_message_at(&text, NOW).unwrap().reset_at,
        None
    );
    let text = format!("{BODY} Your limit will reset in 3 fortnights.");
    assert_eq!(
        antigravity_quota_message_at(&text, NOW).unwrap().reset_at,
        None
    );
}

#[test]
fn other_text_is_not_a_quota_message() {
    assert_eq!(antigravity_quota_message_at("", NOW), None);
    assert_eq!(
        antigravity_quota_message_at("Usage Limit Reached", NOW),
        None
    );
    assert_eq!(
        antigravity_quota_message_at(
            "I saw \"Usage Limit Reached\" in the log. You have reached your current quota for this period.",
            NOW
        ),
        None
    );
    assert_eq!(
        antigravity_quota_message_at(
            "Here is the fix.\nUsage Limit Reached\n\nYou have reached your current quota for this period.",
            NOW
        ),
        None
    );
    assert!(antigravity_quota_message(BODY).is_some());
}

#[test]
fn plan_limit_errors_are_read_only_for_antigravity() {
    let data = json!({ "errorKind": "quota_exhausted", "agent": "antigravity", "resetsAt": 123 });
    assert_eq!(
        antigravity_plan_limit(&data),
        Some(("quota_exhausted", Some(123)))
    );
    let data = json!({ "errorKind": "quota_exhausted", "agent": "antigravity", "resetsAt": null });
    assert_eq!(
        antigravity_plan_limit(&data),
        Some(("quota_exhausted", None))
    );
    // A Codex plan limit has no `agent`.
    assert_eq!(
        antigravity_plan_limit(&json!({ "errorKind": "quota_exhausted" })),
        None
    );
    assert_eq!(
        antigravity_plan_limit(&json!({ "errorKind": "context", "agent": "antigravity" })),
        None
    );
}
