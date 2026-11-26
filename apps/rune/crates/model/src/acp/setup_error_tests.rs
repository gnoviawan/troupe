//! `classifySetupError` against the TypeScript classifier: each expected
//! value is what the TypeScript returned for the same error, captured under
//! bun.

use serde_json::Value;

use super::host_error::HostError;
use super::setup_error::classify_setup_error;

#[test]
fn classify_setup_error_matches_the_typescript_classifier() {
    let cases: Vec<(&str, Option<String>, Option<&str>, &str)> = vec![
        (
            "ACP_AUTH_REQUIRED: sign in first",
            None,
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "ACP_AUTH_REQUIRED: sign in first"}"#,
        ),
        (
            "needs a sign-in",
            Some("agent_auth_required".into()),
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "needs a sign-in"}"#,
        ),
        (
            "ACP_CLAUDE_ACCOUNT_SIGNED_OUT: Sign in to Claude again.",
            None,
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "Sign in to Claude again."}"#,
        ),
        (
            "Sign in to Codex again.",
            Some("codex_account_signed_out".into()),
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "Sign in to Codex again."}"#,
        ),
        (
            "ACP_ANTIGRAVITY_ACCOUNT_SIGNED_OUT: Sign in to Antigravity.",
            None,
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "Sign in to Antigravity."}"#,
        ),
        (
            "ACP_AGENT_UNAVAILABLE: binary missing",
            None,
            Some("claude-acp"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "binary missing", "definite": true}"#,
        ),
        (
            "spawn failed: ENOENT",
            Some("agent_unavailable".into()),
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not run npx. Install Node.js and ensure npx is on your PATH, then try again.", "definite": true}"#,
        ),
        (
            "ACP_INVALID_AGENT_CONFIG: bad env",
            None,
            None,
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "bad env", "definite": true}"#,
        ),
        (
            "quota hit",
            Some("provider_quota_exceeded".into()),
            None,
            r#"{"category": "provider", "label": "Usage limit reached", "detail": "The provider's usage limit was reached. Wait for it to reset or check your plan, then try again. (quota hit)", "providerCode": "provider_quota_exceeded"}"#,
        ),
        (
            "provider_session_limit",
            Some("provider_session_limit".into()),
            None,
            r#"{"category": "provider", "label": "Session limit reached", "detail": "The provider's session limit was reached. Wait for it to reset, then try again.", "providerCode": "provider_session_limit"}"#,
        ),
        (
            "   ",
            Some("provider_request_rejected".into()),
            None,
            r#"{"category": "provider", "label": "Provider rejected the request", "detail": "The provider rejected the request. Check the model and account settings, then try again.", "providerCode": "provider_request_rejected"}"#,
        ),
        (
            "spawn codex ENOENT",
            None,
            Some("codex"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not start \"codex\". Check that the binary exists and is on your PATH."}"#,
        ),
        (
            "spawn x ENOENT",
            None,
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not run npx. Install Node.js and ensure npx is on your PATH, then try again."}"#,
        ),
        (
            "spawn x ENOENT",
            None,
            Some("uvx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not run uvx. Install uv and ensure uvx is on your PATH, then try again."}"#,
        ),
        (
            "No such file or directory (os error 2)",
            None,
            None,
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not start the ACP agent. Check that the command exists and is on your PATH."}"#,
        ),
        (
            "program not found or not executable: C:\\npm\\claude.cmd",
            None,
            Some("node"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "program not found or not executable: C:\\npm\\claude.cmd"}"#,
        ),
        (
            "program not found on PATH: claude",
            None,
            Some("claude"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not start \"claude\". Check that the binary exists and is on your PATH."}"#,
        ),
        (
            "Command failed: exit status: 127\nsh: 1: codex-acp: not found",
            None,
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "npx started, but its package executable \"codex-acp\" was not found. The npx cache may be incomplete. Run `npm config get cache`, rename the `_npx` directory there as a backup, then retry."}"#,
        ),
        (
            "npm error enoent Could not read package.json: /home/u/.npm/_npx/1a2b3c/package.json",
            None,
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "npm could not read package.json in its npx cache entry for this agent. The npx cache may be incomplete. Run `npm config get cache`, rename the `_npx` directory there as a backup, then retry."}"#,
        ),
        (
            "Error: missing optional dependency @openai/codex-linux-x64 (stack)",
            None,
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "npx started, but the package is missing its \"@openai/codex-linux-x64\" dependency. The npx cache may be incomplete. Run `npm config get cache`, rename the `_npx` directory there as a backup, then retry."}"#,
        ),
        (
            "Error: Cannot find module \"x\"",
            None,
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "npx started, but the package failed to load a module it needs. The npx cache may be incomplete. Run `npm config get cache`, rename the `_npx` directory there as a backup, then retry."}"#,
        ),
        (
            "Error: Cannot find module \"x\"",
            None,
            Some("node"),
            r#"{"category": "unknown", "label": "Setup failed", "detail": "Error: Cannot find module \"x\""}"#,
        ),
        (
            "failed to spawn agent for config gemini",
            None,
            Some("gemini-acp"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "Could not start \"gemini-acp\". Check that the binary exists and is on your PATH."}"#,
        ),
        (
            "stream closed by peer",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "Agent connection was interrupted. Wait a moment, then retry."}"#,
        ),
        (
            "Stream closed NGHTTP2_ENHANCE_YOUR_CALM",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "Connection was rate-limited by the agent service. Wait a moment, then retry."}"#,
        ),
        (
            "connection reset by server",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "Agent connection was interrupted. Wait a moment, then retry."}"#,
        ),
        (
            "the transport broke",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "the transport broke"}"#,
        ),
        (
            "write EPIPE",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "Agent connection was interrupted. Wait a moment, then retry."}"#,
        ),
        (
            "HTTP 429 too many requests during auth",
            None,
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "HTTP 429 too many requests during auth"}"#,
        ),
        (
            "HTTP 401 unauthorized",
            None,
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "HTTP 401 unauthorized"}"#,
        ),
        (
            "invalid api-key",
            None,
            None,
            r#"{"category": "auth", "label": "Authentication required", "detail": "invalid api-key"}"#,
        ),
        (
            "session/new timed out after 30s",
            None,
            None,
            r#"{"category": "timeout", "label": "Session setup timed out", "detail": "session/new timed out after 30s"}"#,
        ),
        (
            "initialize timeout",
            None,
            None,
            r#"{"category": "timeout", "label": "Session setup timed out", "detail": "initialize timeout"}"#,
        ),
        (
            "something odd",
            None,
            None,
            r#"{"category": "unknown", "label": "Setup failed", "detail": "something odd"}"#,
        ),
        (
            "rate limit exceeded",
            None,
            None,
            r#"{"category": "unknown", "label": "Setup failed", "detail": "Connection was rate-limited by the agent service. Wait a moment, then retry."}"#,
        ),
        (
            "agent thread is no longer running",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "Agent connection was interrupted. Wait a moment, then retry."}"#,
        ),
        (
            "connection to host timed out",
            None,
            None,
            r#"{"category": "transport", "label": "Agent connection lost", "detail": "Agent connection was interrupted. Wait a moment, then retry."}"#,
        ),
        (
            "pick a method",
            Some("ACP_MULTI_AUTH".into()),
            None,
            r#"{"category": "multi-auth", "label": "Multiple sign-in methods", "detail": "pick a method"}"#,
        ),
        (
            "Command failed: exit status: 1270\nsh: 1: x: not found",
            None,
            Some("npx"),
            r#"{"category": "unknown", "label": "Setup failed", "detail": "Command failed: exit status: 1270\nsh: 1: x: not found"}"#,
        ),
        (
            "Command failed: exit status: 127\nsh: codex-acp: command not found",
            None,
            Some("npx"),
            r#"{"category": "spawn", "label": "Agent unavailable", "detail": "npx started, but its package executable \"codex-acp\" was not found. The npx cache may be incomplete. Run `npm config get cache`, rename the `_npx` directory there as a backup, then retry."}"#,
        ),
        (
            "got HTTP 429 from upstream",
            None,
            None,
            r#"{"category": "unknown", "label": "Setup failed", "detail": "Connection was rate-limited by the agent service. Wait a moment, then retry."}"#,
        ),
        (
            "NGHTTP2_PROTOCOL_ERROR",
            None,
            None,
            r#"{"category": "unknown", "label": "Setup failed", "detail": "Agent connection was interrupted. Wait a moment, then retry."}"#,
        ),
        (
            "Command failed: exit status: 128\nsh: 1: x: not found",
            None,
            Some("npx"),
            r#"{"category": "unknown", "label": "Setup failed", "detail": "Command failed: exit status: 128\nsh: 1: x: not found"}"#,
        ),
    ];
    for (message, code, command, want) in cases {
        let want: Value = serde_json::from_str(want).expect("captured JSON");
        let got = classify_setup_error(&HostError::new(message.into(), code), command);
        assert_eq!(got, want, "{message:?} {command:?}");
    }
}

#[test]
fn an_ambiguous_sign_in_reads_as_multi_auth() {
    let message = "This agent advertises multiple sign-in methods (A, B). Troupe does not choose one automatically; pick one of the methods below to sign in.";
    let got = classify_setup_error(&HostError::ambiguous(message.into()), None);
    let want = serde_json::json!({"category": "multi-auth", "label": "Multiple sign-in methods", "detail": "This agent advertises multiple sign-in methods (A, B). Troupe does not choose one automatically; pick one of the methods below to sign in."});
    assert_eq!(got, want);
}

#[test]
fn non_ascii_spawn_errors_do_not_panic() {
    for (message, command) in [
        (r"spawn failed: C:\Users\İlker\claude.exe program not found", Some("claude")),
        ("npx failed: İİİ missing optional dependency @scope/pkg-İ (x)", Some("npx")),
        ("ẞtraße İ program not found on PATH: İ", None),
    ] {
        let _ = classify_setup_error(&HostError::new(message.into(), None), command);
    }
}
