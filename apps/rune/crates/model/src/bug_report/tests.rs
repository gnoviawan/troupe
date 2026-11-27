use super::{Cause, fallback_cause};

#[test]
fn a_fallback_line_is_the_hosts_reason_flattened_and_cut() {
    assert_eq!(fallback_cause(Cause::Unavailable, Some("  gh:\n  not\tfound ")), "gh: not found");
    assert_eq!(fallback_cause(Cause::Failed, None), "gh issue create failed.");
    assert_eq!(fallback_cause(Cause::Unavailable, Some("   ")), "GitHub CLI is unavailable.");
    let long = "x".repeat(200);
    assert_eq!(fallback_cause(Cause::Failed, Some(&long)), format!("{}…", "x".repeat(160)));
    assert!(fallback_cause(Cause::Transport, Some("ignored")).contains("renderer transport failure"));
}
