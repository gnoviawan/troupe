use super::{parse_usage_reply, QuotaReport};
use crate::acp::antigravity_accounts::usage::{AntigravityUsageStatus, AntigravityUsageWindow};

fn window(label: &str, percent: f64, resets_at: Option<&str>) -> AntigravityUsageWindow {
    AntigravityUsageWindow {
        kind: "model".to_string(),
        label: label.to_string(),
        model_id: label.to_string(),
        percent,
        resets_at: resets_at.map(str::to_string),
    }
}

fn no_meters(lines: &[&str]) -> QuotaReport {
    QuotaReport::NoMeters(lines.iter().map(|line| (*line).to_string()).collect())
}

const PERSONAL: &str = "### Session Usage
- **Session total:** data not available yet

### Quota Limits
- **Gemini Models**
  - **Weekly Limit Remaining:** 91% remaining (resets Oct 02, 2026, 14:00 UTC)
- **Claude and GPT models**
  - **Weekly Limit Remaining:** 100% remaining";

#[test]
fn each_percent_bucket_becomes_a_window_of_percent_used_most_used_first() {
    assert_eq!(
        parse_usage_reply(PERSONAL),
        QuotaReport::Meters {
            windows: vec![
                window(
                    "Gemini Models · Weekly Limit Remaining",
                    9.0,
                    Some("2026-10-02T14:00:00Z")
                ),
                window("Claude and GPT models · Weekly Limit Remaining", 0.0, None),
            ],
            notes: vec![],
        }
    );
}

#[test]
fn a_reset_time_in_another_format_keeps_the_window_without_a_reset() {
    let reply = "### Quota Limits
- **Gemini Models**
  - **Daily:** 40% remaining (resets 2026-10-02T08:22:00+05:00)";

    assert_eq!(
        parse_usage_reply(reply),
        QuotaReport::Meters {
            windows: vec![window("Gemini Models · Daily", 60.0, None)],
            notes: vec![],
        }
    );
}

#[test]
fn the_agent_error_lines_are_kept_word_for_word() {
    for line in [
        "Unable to retrieve quota status (deadline exceeded)",
        "Not applicable for current authentication method.",
        "data not available yet",
        "No quota limits reported.",
    ] {
        let reply = format!(
            "### Session Usage\n- **Session total:** data not available yet\n\n### Quota Limits\n- {line}"
        );
        assert_eq!(parse_usage_reply(&reply), no_meters(&[line]), "{line}");
    }
}

#[test]
fn unit_counts_and_disabled_buckets_are_notes_not_meters() {
    let reply = "### Quota Limits
- **Gemini Models**
  *(Shared by every Gemini model)*
  - **Requests:** 1,000 units remaining (resets Oct 02, 2026, 14:00 UTC)
  - **Burst:** Available
- **Claude and GPT models**
  - **Weekly:** Disabled: not in your plan
- **Image models**
  - No active bucket limits";

    assert_eq!(
        parse_usage_reply(reply),
        no_meters(&[
            "Gemini Models · Requests: 1,000 units remaining (resets Oct 02, 2026, 14:00 UTC)",
            "Gemini Models · Burst: Available",
            "Claude and GPT models · Weekly: Disabled: not in your plan",
            "Image models · No active bucket limits",
        ])
    );
}

#[test]
fn business_alert_and_overage_lines_ride_along_with_the_meters() {
    let reply = "### Quota Limits
- **Alert:** You have used 80% of your monthly budget.
- **Gemini Models**
  - **Monthly Budget:** 20% remaining (resets Nov 01, 2026, 00:00 UTC)
    - *Hard stop at 100%.*
- **Overage:** Disabled";

    assert_eq!(
        parse_usage_reply(reply),
        QuotaReport::Meters {
            windows: vec![window(
                "Gemini Models · Monthly Budget",
                80.0,
                Some("2026-11-01T00:00:00Z")
            )],
            notes: vec![
                "Alert: You have used 80% of your monthly budget.".to_string(),
                "Overage: Disabled".to_string(),
            ],
        }
    );
}

#[test]
fn a_reply_with_no_quota_section_is_its_own_first_line() {
    assert_eq!(
        parse_usage_reply("\n/usage takes no arguments, got 'now'\n"),
        no_meters(&["/usage takes no arguments, got 'now'"])
    );
    assert_eq!(
        parse_usage_reply("   "),
        no_meters(&["Antigravity sent an empty /usage reply."])
    );
}

#[test]
fn a_percent_outside_0_to_100_is_not_a_meter() {
    let reply = "### Quota Limits
- **Gemini Models**
  - **Weekly:** 140% remaining";

    assert_eq!(
        parse_usage_reply(reply),
        no_meters(&["Gemini Models · Weekly: 140% remaining"])
    );
}

#[test]
fn meters_become_an_ok_record_and_agent_lines_an_unavailable_one() {
    let ok = parse_usage_reply(PERSONAL).into_usage("work", 1_700_000_000_000);
    assert_eq!(ok.status, AntigravityUsageStatus::Ok);
    assert_eq!(ok.windows.len(), 2);
    assert_eq!(ok.checked_at, 1_700_000_000_000);
    assert_eq!(ok.message, None);

    let none = parse_usage_reply("### Quota Limits\n- data not available yet")
        .into_usage("work", 1_700_000_000_000);
    assert_eq!(none.account_id, "work");
    assert_eq!(none.status, AntigravityUsageStatus::Unavailable);
    assert!(none.windows.is_empty());
    assert_eq!(none.message.as_deref(), Some("data not available yet"));
}

#[test]
fn a_bucket_indented_with_four_spaces_or_a_tab_is_still_a_meter() {
    let reply = "### Quota Limits\n- **Gemini Models**\n    - **Weekly:** 75% remaining\n- **Claude and GPT models**\n\t- **Weekly:** 50% remaining";

    assert_eq!(
        parse_usage_reply(reply),
        QuotaReport::Meters {
            windows: vec![
                window("Claude and GPT models · Weekly", 50.0, None),
                window("Gemini Models · Weekly", 25.0, None),
            ],
            notes: vec![],
        }
    );
}

#[test]
fn a_long_top_level_line_is_clipped_to_300_characters() {
    let long = "x".repeat(400);
    let reply = format!("### Quota Limits\n- **Overage:** {long}\n- {long}");

    let QuotaReport::NoMeters(notes) = parse_usage_reply(&reply) else {
        panic!("no meter in the reply");
    };
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().all(|note| note.chars().count() == 300));
    assert!(notes[0].starts_with("Overage: xxx"));
}
