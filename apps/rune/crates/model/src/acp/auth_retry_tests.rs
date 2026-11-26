//! `isAuthRetriableSessionError` on error texts. The expected values are what
//! the TypeScript classifier answered for each text (`classifySetupError`).

use super::auth_retry::is_auth_retriable;
use super::host_error::HostError;

const CASES: &[(&str, bool)] = &[
    ("ACP_AUTH_REQUIRED: sign in", true),
    ("ACP_AUTH_REQUIRED", true),
    ("ACP_CLAUDE_ACCOUNT_SIGNED_OUT: x", true),
    ("ACP_CODEX_ACCOUNT_SIGNED_OUT: x", true),
    ("ACP_ANTIGRAVITY_ACCOUNT_SIGNED_OUT: x", true),
    ("ACP_CLAUDE_ACCOUNT_SIGNED_OUT", false),
    ("ACP_AGENT_UNAVAILABLE: auth", false),
    ("ACP_INVALID_AGENT_CONFIG: login", false),
    ("spawn auth fail", false),
    ("spawn\nauth fail", true),
    ("ENOENT auth", false),
    ("command not found: login", false),
    ("failed to spawn: auth", false),
    ("auth: broken pipe", false),
    ("epipe auth", false),
    ("pipeline auth", true),
    ("pipe_x auth", true),
    ("connection timed out: auth", false),
    ("connection\ntimed out: auth", true),
    ("Transport closed: auth", false),
    ("agent thread dropped login", false),
    ("stream error: auth", false),
    ("disconnected auth", false),
    ("Unauthorized", true),
    ("unauthorised", true),
    ("Please Sign-In", true),
    ("sign in", true),
    ("signin", true),
    ("sign\tin", true),
    ("sign  in", false),
    ("API key missing", true),
    ("api-key", true),
    ("apikey", true),
    ("Not logged in", true),
    ("Logged out", true),
    ("HTTP 401", true),
    ("x401", false),
    ("401x", false),
    ("4012", false),
    ("_401", false),
    ("code 403!", true),
    ("Forbidden", true),
    ("credentials expired", true),
    ("Login required", true),
    ("timed out", false),
    ("rate limited", false),
    ("something else", false),
    ("", false),
    ("AUTHENTICATION", true),
    ("Ünauthorized", true),
];

/// `/ws` errors: the code, the message without the desktop tag, and what
/// `isAuthRetriableSessionError` answers for an `AcpTransportError` of them.
const CODED: [(&str, &str, bool); 13] = [
    ("agent_auth_required", "nope", true),
    (
        "claude_account_signed_out",
        "Sign in to the Claude account first",
        true,
    ),
    ("codex_account_signed_out", "gone", true),
    ("antigravity_account_signed_out", "gone", true),
    ("agent_unavailable", "login binary missing", false),
    ("invalid_agent_config", "bad auth config", false),
    ("provider_quota_exceeded", "login quota exceeded", false),
    ("provider_session_limit", "auth limit", false),
    ("provider_request_rejected", "401 rejected", false),
    ("ACP_MULTI_AUTH", "pick a method to sign in", false),
    ("not_implemented", "not logged in", true),
    ("not_implemented", "boom", false),
    ("not_found", "unknown agent: a", false),
];

#[test]
fn auth_retriable_errors_match_the_typescript_classifier() {
    let wrong: Vec<&(&str, bool)> = CASES
        .iter()
        .filter(|(message, want)| {
            is_auth_retriable(&HostError::new((*message).into(), None)) != *want
        })
        .collect();
    assert!(wrong.is_empty(), "misclassified: {wrong:?}");
    let wrong: Vec<&(&str, &str, bool)> = CODED
        .iter()
        .filter(|(code, message, want)| {
            let error = HostError::new((*message).into(), Some((*code).into()));
            is_auth_retriable(&error) != *want
        })
        .collect();
    assert!(wrong.is_empty(), "misclassified with a code: {wrong:?}");
}
