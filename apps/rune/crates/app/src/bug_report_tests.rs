use client::http::HttpError;
use serde_json::json;

use crate::State;
use crate::bug_report::{Collect, KEY, Step};

fn context() -> serde_json::Value {
    json!({ "appVersion": "0.6.0", "os": "linux", "arch": "x86_64", "channel": "release", "runId": "r", "collectedAt": "now", "logAvailable": false, "logTruncated": false, "logTail": "" })
}

#[test]
fn open_collects_and_submit_needs_a_title_then_sends_the_previewed_markdown() {
    let mut s = State::default();
    s.open_bug_report();
    assert!(s.overlays.is_open(KEY));
    assert_eq!(s.bug.sent.last().map(|(step, r)| (step.clone(), r.path.clone())), Some((Step::Collect(1), "/log/diagnostics".to_owned())));
    s.submit_bug_report();
    assert_eq!(s.bug.field_error, Some("Give the report a short title."));
    s.bug_report_fetched(Step::Collect(1), Ok(context()));
    assert!(matches!(s.bug.collect, Collect::Ready(_)));
    s.bug.title = rune::TextEdit::new(" Crash ");
    s.submit_bug_report();
    let (step, request) = s.bug.sent.last().cloned().unwrap();
    assert_eq!((step, request.path.as_str()), (Step::Submit, "/log/report-bug"));
    let body = request.body.unwrap();
    assert_eq!(body["title"], json!("Crash"));
    assert_eq!(body["body"].as_str(), s.bug_markdown().as_deref());
    assert!(s.close_top(), "Escape is taken while submitting");
    assert!(s.overlays.is_open(KEY));
}

#[test]
fn an_unavailable_gh_keeps_the_dialog_open_and_waits_for_the_clipboard_fallback() {
    let mut s = State::default();
    s.open_bug_report();
    s.bug_report_fetched(Step::Collect(1), Err(HttpError::Network("down".into())));
    assert_eq!(s.bug.collect, Collect::Unavailable);
    assert!(s.bug_markdown().unwrap().contains("- **Troupe:** unavailable (unavailable)"));
    s.bug.title = rune::TextEdit::new("Crash");
    s.submit_bug_report();
    s.bug_report_fetched(Step::Submit, Ok(json!({ "kind": "unavailable", "reason": "gh CLI is not available" })));
    assert!(s.overlays.is_open(KEY), "the dialog stays for the screenshots");
    assert!(!s.bug.submitting);
}

#[test]
fn a_late_collect_of_an_earlier_open_is_dropped() {
    let mut s = State::default();
    s.open_bug_report();
    s.close(KEY);
    s.open_bug_report();
    s.bug_report_fetched(Step::Collect(1), Ok(context()));
    assert_eq!(s.bug.collect, Collect::Collecting);
}
