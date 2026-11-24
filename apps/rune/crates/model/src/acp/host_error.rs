//! The host's verdict on a failed command. `/ws` carries it as the reply's
//! `code` and drops the message's tag; the desktop `invoke` path keeps the
//! tag (`ACP_AGENT_UNAVAILABLE: …`) and sends no code. The classifiers read
//! both, as `tauri-verdicts.ts` and `acp-spawn-errors.ts` do.

/// A command's failure: the error's message and the host's error code.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HostError {
    pub message: String,
    pub code: Option<String>,
}

impl HostError {
    pub(crate) fn new(message: String, code: Option<String>) -> Self {
        Self { message, code }
    }

    /// The `/ws` code, or the desktop message tag, of one of `verdicts`.
    pub(crate) fn is(&self, verdicts: &[Verdict]) -> bool {
        verdicts.iter().any(|(code, tag)| {
            self.code.as_deref() == Some(*code) || self.message.starts_with(&format!("{tag}: "))
        })
    }
}

/// A `/ws` code and the desktop message tag of the same verdict.
pub(crate) type Verdict = (&'static str, &'static str);

/// A spawn that can never succeed as asked (`DEFINITE_SPAWN_REJECTIONS`).
pub(crate) const DEFINITE_SPAWN_REJECTION: [Verdict; 2] = [
    ("agent_unavailable", "ACP_AGENT_UNAVAILABLE"),
    ("invalid_agent_config", "ACP_INVALID_AGENT_CONFIG"),
];

/// A provider Account that is signed out.
pub(crate) const SIGNED_OUT: [Verdict; 3] = [
    ("claude_account_signed_out", "ACP_CLAUDE_ACCOUNT_SIGNED_OUT"),
    ("codex_account_signed_out", "ACP_CODEX_ACCOUNT_SIGNED_OUT"),
    (
        "antigravity_account_signed_out",
        "ACP_ANTIGRAVITY_ACCOUNT_SIGNED_OUT",
    ),
];

/// `AMBIGUOUS_AUTH_CODE`: several sign-in methods and none chosen.
pub(crate) const AMBIGUOUS_AUTH: &str = "ACP_MULTI_AUTH";

/// `AGENT_AUTH_REQUIRED_CODE`.
pub(crate) const AUTH_REQUIRED: &str = "agent_auth_required";

/// `PROVIDER_ERROR_CODES`: a provider's refusal, never a sign-in.
pub(crate) const PROVIDER: [&str; 3] = [
    "provider_quota_exceeded",
    "provider_session_limit",
    "provider_request_rejected",
];
