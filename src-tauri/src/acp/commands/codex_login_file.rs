//! Codex login state from the account's `auth.json`, for a host without the
//! `codex` CLI on PATH (the Codex agent can run as `npx codex-acp`, which
//! brings its own Codex). Only the presence of a ChatGPT login is read: token
//! values are never logged, copied or returned.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::CodexLoginProbe;

const AUTH_FILE: &str = "auth.json";

/// One info log per process when the probe first falls back to the file.
static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

/// The Codex home the CLI would use: the account home, else `CODEX_HOME`,
/// else `~/.codex`.
fn codex_home_dir(codex_home: Option<&str>) -> Option<PathBuf> {
    let explicit = codex_home
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    explicit
        .or_else(|| {
            std::env::var("CODEX_HOME")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".codex")))
}

/// Login state from the auth file contents: a ChatGPT login has a non-empty
/// access or refresh token. An API key alone is not a ChatGPT login, so it
/// reads as signed out (Troupe signs Codex accounts in with ChatGPT only).
pub(crate) fn login_from_auth_json(contents: &str) -> CodexLoginProbe {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(contents) else {
        log::warn!("[acp] codex auth file is not valid JSON");
        return CodexLoginProbe::Unavailable;
    };
    let has_token = |key: &str| {
        value
            .get("tokens")
            .and_then(|tokens| tokens.get(key))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|token| !token.trim().is_empty())
    };
    if has_token("refresh_token") || has_token("access_token") {
        CodexLoginProbe::SignedIn
    } else {
        CodexLoginProbe::SignedOut
    }
}

fn login_from_dir(dir: &Path) -> CodexLoginProbe {
    match std::fs::read_to_string(dir.join(AUTH_FILE)) {
        Ok(contents) => login_from_auth_json(&contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CodexLoginProbe::SignedOut,
        Err(error) => {
            log::warn!(
                "[acp] codex auth file could not be read: {:?}",
                error.kind()
            );
            CodexLoginProbe::Unavailable
        }
    }
}

/// The login state when `codex login status` cannot run because the CLI is
/// not installed.
pub(crate) fn codex_login_from_file(codex_home: Option<&str>) -> CodexLoginProbe {
    if !FALLBACK_LOGGED.swap(true, Ordering::Relaxed) {
        log::info!("[acp] codex CLI not found; reading the Codex login file instead");
    }
    match codex_home_dir(codex_home) {
        Some(dir) => login_from_dir(&dir),
        None => CodexLoginProbe::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chatgpt_login_reads_as_signed_in() {
        let json = r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":"a","access_token":"b","refresh_token":"c","account_id":"d"}}"#;
        assert_eq!(login_from_auth_json(json), CodexLoginProbe::SignedIn);
    }

    #[test]
    fn an_api_key_or_empty_tokens_read_as_signed_out() {
        assert_eq!(
            login_from_auth_json(r#"{"OPENAI_API_KEY":"sk-x","tokens":null}"#),
            CodexLoginProbe::SignedOut
        );
        assert_eq!(
            login_from_auth_json(r#"{"tokens":{"access_token":"","refresh_token":"  "}}"#),
            CodexLoginProbe::SignedOut
        );
    }

    #[test]
    fn a_broken_file_is_unavailable() {
        assert_eq!(
            login_from_auth_json("{not json"),
            CodexLoginProbe::Unavailable
        );
    }

    #[test]
    fn a_home_without_the_file_is_signed_out_and_with_it_signed_in() {
        let dir = tempfile::tempdir().expect("temp dir");
        let home = dir.path().to_str().expect("utf-8 path");
        assert_eq!(
            codex_login_from_file(Some(home)),
            CodexLoginProbe::SignedOut
        );
        std::fs::write(
            dir.path().join(AUTH_FILE),
            r#"{"tokens":{"refresh_token":"r"}}"#,
        )
        .expect("write auth file");
        assert_eq!(codex_login_from_file(Some(home)), CodexLoginProbe::SignedIn);
    }
}
