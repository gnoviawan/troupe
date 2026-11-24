use serde_json::json;

use super::*;

const NOW: i64 = 1_700_000_000_000;

#[test]
fn models_with_quota_become_windows_sorted_by_use() {
    let reply = json!({
        "models": {
            "gemini-3-pro": {
                "displayName": "Gemini 3 Pro",
                "quotaInfo": { "remainingFraction": 0.25, "resetTime": "2026-01-05T14:30:00Z" }
            },
            "gemini-3-flash": {
                "displayName": "Gemini 3 Flash",
                "quotaInfo": { "remainingFraction": 1.0 }
            },
            "claude-sonnet": {
                "quotaInfo": { "resetTime": "2026-01-05T10:00:00Z" }
            },
            "internal-model": {
                "displayName": "Internal",
                "isInternal": true,
                "quotaInfo": { "remainingFraction": 0.0 }
            },
            "off": {
                "displayName": "Off",
                "disabled": true,
                "quotaInfo": { "remainingFraction": 0.0 }
            },
            "no-quota": { "displayName": "No quota" }
        }
    });
    let usage = usage_from_models("acct", Some("Pro".to_string()), &reply, NOW);
    assert_eq!(usage.status, AntigravityUsageStatus::Ok);
    assert_eq!(usage.plan_type.as_deref(), Some("Pro"));
    assert_eq!(usage.checked_at, NOW);
    let labels: Vec<&str> = usage.windows.iter().map(|w| w.label.as_str()).collect();
    // A reset time without a remaining fraction is a used-up bucket; a model
    // without a display name is labelled by its id.
    assert_eq!(labels, ["claude-sonnet", "Gemini 3 Pro", "Gemini 3 Flash"]);
    assert_eq!(usage.windows[0].percent, 100.0);
    assert_eq!(usage.windows[1].percent, 75.0);
    assert_eq!(usage.windows[1].model_id, "gemini-3-pro");
    assert_eq!(
        usage.windows[1].resets_at.as_deref(),
        Some("2026-01-05T14:30:00Z")
    );
    assert_eq!(usage.windows[2].percent, 0.0);
    assert_eq!(usage.windows[2].resets_at, None);
    assert!(usage.windows.iter().all(|w| w.kind == "model"));
}

#[test]
fn percent_is_clamped_and_ties_sort_by_label() {
    let reply = json!({
        "models": {
            "b": { "displayName": "B", "quotaInfo": { "remainingFraction": -0.5 } },
            "a": { "displayName": "A", "quotaInfo": { "remainingFraction": 0.0 } },
            "c": { "displayName": "C", "quotaInfo": { "remainingFraction": 1.5 } }
        }
    });
    let usage = usage_from_models("acct", None, &reply, NOW);
    let shown: Vec<(&str, f64)> = usage
        .windows
        .iter()
        .map(|w| (w.label.as_str(), w.percent))
        .collect();
    assert_eq!(shown, [("A", 100.0), ("B", 100.0), ("C", 0.0)]);
}

#[test]
fn a_reply_without_quota_is_unavailable() {
    for reply in [
        json!({}),
        json!({ "models": {} }),
        json!({ "models": { "m": { "displayName": "M" } } }),
        json!({ "models": [] }),
    ] {
        let usage = usage_from_models("acct", Some("Pro".to_string()), &reply, NOW);
        assert_eq!(usage.status, AntigravityUsageStatus::Unavailable);
        assert!(usage.windows.is_empty());
        assert_eq!(
            usage.message.as_deref(),
            Some("Antigravity did not report plan limits")
        );
    }
}

#[test]
fn verdicts_map_to_statuses() {
    let signed_out = usage_from_verdict("a", ProbeVerdict::SignedOut, NOW);
    assert_eq!(signed_out.status, AntigravityUsageStatus::SignedOut);
    assert_eq!(signed_out.message, None);
    let keychain = usage_from_verdict("a", ProbeVerdict::Unavailable(KEYCHAIN_UNAVAILABLE), NOW);
    assert_eq!(keychain.status, AntigravityUsageStatus::Unavailable);
    assert_eq!(
        keychain.message.as_deref(),
        Some("Antigravity usage is not available for a keychain login")
    );
    let failed = usage_from_verdict(
        "a",
        ProbeVerdict::Failed("Antigravity usage timed out"),
        NOW,
    );
    assert_eq!(failed.status, AntigravityUsageStatus::Error);
    let ok = usage_from_verdict(
        "a",
        ProbeVerdict::Models {
            plan_type: None,
            reply: json!({ "models": { "m": { "quotaInfo": { "remainingFraction": 0.5 } } } }),
        },
        NOW,
    );
    assert_eq!(ok.status, AntigravityUsageStatus::Ok);
    assert_eq!(ok.windows[0].percent, 50.0);
}

#[test]
fn usage_serializes_with_the_renderer_field_names() {
    let usage = usage_from_models(
        "acct",
        Some("Pro".to_string()),
        &json!({ "models": { "m": { "displayName": "M", "quotaInfo": { "remainingFraction": 0.5, "resetTime": "r" } } } }),
        NOW,
    );
    assert_eq!(
        serde_json::to_value(&usage).unwrap(),
        json!({
            "accountId": "acct",
            "status": "ok",
            "planType": "Pro",
            "windows": [{ "kind": "model", "label": "M", "modelId": "m", "percent": 50.0, "resetsAt": "r" }],
            "checkedAt": NOW,
            "message": null
        })
    );
    let signed_out = usage_from_verdict("acct", ProbeVerdict::SignedOut, NOW);
    assert_eq!(
        serde_json::to_value(&signed_out).unwrap()["status"],
        "signed_out"
    );
}

#[test]
fn load_code_assist_fields_are_read() {
    let load = json!({
        "cloudaicompanionProject": { "id": "proj-1" },
        "currentTier": { "id": "free-tier", "name": "Free" },
        "paidTier": { "id": "g1-pro-tier", "name": "Google AI Pro", "usesGcpTos": true }
    });
    assert_eq!(code_assist_project(&load).as_deref(), Some("proj-1"));
    assert_eq!(plan_name(&load).as_deref(), Some("Google AI Pro"));
    assert_eq!(quota_endpoint(&load), GCP_TOS_ENDPOINT);

    let load = json!({
        "cloudaicompanionProject": "proj-2",
        "currentTier": { "id": "free-tier" }
    });
    assert_eq!(code_assist_project(&load).as_deref(), Some("proj-2"));
    assert_eq!(plan_name(&load).as_deref(), Some("free-tier"));
    assert_eq!(quota_endpoint(&load), DAILY_ENDPOINT);

    let load = json!({ "paidTier": { "id": "", "name": " " }, "cloudaicompanionProject": "" });
    assert_eq!(code_assist_project(&load), None);
    assert_eq!(plan_name(&load), None);
    assert_eq!(quota_endpoint(&load), DAILY_ENDPOINT);
}

#[test]
fn invalid_grant_is_recognized() {
    assert!(is_invalid_grant(&json!({ "error": "invalid_grant" })));
    assert!(!is_invalid_grant(&json!({ "error": "invalid_client" })));
    assert!(!is_invalid_grant(&json!({})));
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
