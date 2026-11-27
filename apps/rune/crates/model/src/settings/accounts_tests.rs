use serde_json::json;

use super::*;

#[test]
fn usage_words_match_the_web_client() {
    assert_eq!(usage_tone(79.9), UsageTone::Normal);
    assert_eq!(usage_tone(80.0), UsageTone::Warning);
    assert_eq!(usage_tone(100.0), UsageTone::Critical);
    assert_eq!(claude_window_label("session", None), "5-hour");
    assert_eq!(claude_window_label("weekly_scoped", Some("Opus")), "Weekly · Opus");
    assert_eq!(claude_window_label("extra_usage", None), "Extra usage");
    let now = 1_000_000_000.0;
    assert_eq!(format_reset(Some(now + 20_000.0), now).as_deref(), Some("resets now"));
    assert_eq!(format_reset(Some(now + 45.0 * 60_000.0), now).as_deref(), Some("resets in 45m"));
    assert_eq!(format_reset(Some(now + 134.0 * 60_000.0), now).as_deref(), Some("resets in 2h 14m"));
    assert_eq!(format_reset(Some(now + 120.0 * 60_000.0), now).as_deref(), Some("resets in 2h"));
    assert_eq!(format_reset(Some(now + 70.0 * 3_600_000.0), now).as_deref(), Some("resets in 3d"));
    assert_eq!(format_reset(None, now), None);
    assert_eq!(format_checked(now - 30_000.0, now), "Updated just now");
    assert_eq!(format_checked(now - 4.0 * 60_000.0, now), "Updated 4m ago");
    assert_eq!(format_checked(now - 130.0 * 60_000.0, now), "Updated 2h ago");
    assert_eq!(plan_label(Some("max")).as_deref(), Some("Max"));
    assert_eq!(count_text(1), "1 account");
    assert_eq!(count_text(3), "3 accounts");
}

#[test]
fn a_claude_row_says_its_plan_and_its_state() {
    let ready = json!({ "id": "a", "name": "Work", "builtin": false, "signedIn": true });
    let usage = json!({ "accountId": "a", "status": "ok", "subscriptionType": "max", "checkedAt": 1, "message": null, "windows": [
        { "kind": "session", "group": "session", "scope": null, "percent": 42.0, "resetsAt": null, "severity": null },
        { "kind": "weekly_scoped", "group": "weekly", "scope": "Opus", "percent": 85.0, "resetsAt": null, "severity": null }
    ] });
    assert_eq!(claude_status(&ready, Some(&usage), false, 0), (AccountTone::Warning, "Max · near the Weekly · Opus limit".into()));
    assert_eq!(claude_primary(Some(&usage)).unwrap().label, "Weekly · Opus");
    assert_eq!(claude_others(Some(&usage)).iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["5-hour"]);
    assert_eq!(claude_status(&ready, Some(&usage), true, 0), (AccountTone::Waiting, "Max · waiting for browser".into()));
    let out = json!({ "id": "b", "name": "Home", "builtin": false, "signedIn": false });
    assert_eq!(claude_status(&out, None, false, 2), (AccountTone::SignedOut, "signed out".into()));
    let unknown = json!({ "id": "c", "name": "C", "builtin": false, "signedIn": null });
    assert_eq!(claude_status(&unknown, None, false, 0), (AccountTone::Unknown, "Could not check sign-in".into()));
    assert_eq!(claude_status(&ready, None, false, 2), (AccountTone::Ok, "2 projects".into()));
}

#[test]
fn codex_and_antigravity_rows_list_their_windows() {
    let account = json!({ "id": "a", "name": "A", "builtin": true, "signedIn": true });
    let codex = json!({ "status": "ok", "planType": "plus", "windows": [{ "kind": "session", "percent": 10.0, "resetsAt": null, "windowDurationMins": 300 }, { "kind": "weekly", "percent": 50.0, "resetsAt": null, "windowDurationMins": null }] });
    assert_eq!(meters(Provider::Codex, Some(&codex)).iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["5-hour", "Weekly"]);
    assert_eq!(other_status(&account, Some(&codex), &["alpha".into()]), (AccountTone::Ok, "Plus · alpha".into()));
    let agy = json!({ "status": "ok", "planType": null, "windows": [{ "kind": "model", "label": "", "modelId": "gemini-3-pro", "percent": 3.0, "resetsAt": null }] });
    assert_eq!(meters(Provider::Antigravity, Some(&agy))[0].label, "gemini-3-pro");
    assert_eq!(other_status(&json!({ "signedIn": false }), None, &[]), (AccountTone::SignedOut, "Signed out".into()));
}

#[test]
fn remove_and_default_words() {
    assert!(!shows_default_pill(" Default "));
    assert!(shows_default_pill("Work"));
    assert_eq!(moving_text(1, Some("Work")), "1 project moves to the default account (Work)");
    assert_eq!(moving_text(2, None), "2 projects move to the default account");
}
