//! The Antigravity login probe. Antigravity has no `login status` command, so
//! a home is signed in when its Google token file holds a refresh token. The
//! built-in Account on macOS may keep its login in the keychain instead; that
//! entry is looked up without reading the secret. Nothing here logs a path,
//! a token or an account id.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::AntigravityLoginStatus;
use crate::acp::antigravity_accounts::AntigravityAccountStore;

/// The agent's own directory under a home.
const AGENT_DIR: &str = crate::acp::antigravity_accounts::links::AGENT_DIR;
/// The consumer Google login.
pub(crate) const TOKEN_FILE: &str = "acp_token.json";
/// The Gemini Enterprise login.
pub(crate) const BUSINESS_TOKEN_FILE: &str = "acp_business_token.json";
/// Keychain service and accounts of the agent's macOS login.
const KEYCHAIN_SERVICE: &str = "gemini";
const KEYCHAIN_ACCOUNTS: [&str; 2] = ["antigravity-acp", "antigravity-acp-business"];
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// The two token files of `home`, consumer first.
pub(crate) fn token_files(home: &Path) -> [PathBuf; 2] {
    let dir = home.join(AGENT_DIR);
    [dir.join(TOKEN_FILE), dir.join(BUSINESS_TOKEN_FILE)]
}

/// Whether `bytes` are a JSON object with a non-empty `refresh_token` string.
pub(crate) fn has_refresh_token(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .as_object()?
                .get("refresh_token")?
                .as_str()
                .map(|token| !token.trim().is_empty())
        })
        .unwrap_or(false)
}

/// The login state of `home` from its token files: `Some(true)` when one holds
/// a refresh token, `Some(false)` when none does, `None` when one could not be
/// read and the other did not confirm a login.
pub(crate) fn token_file_login(home: &Path) -> Option<bool> {
    let mut unreadable = false;
    for path in token_files(home) {
        match std::fs::read(&path) {
            Ok(bytes) if has_refresh_token(&bytes) => return Some(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                log::warn!(
                    "[antigravity-accounts] could not read a login file: {:?}",
                    error.kind()
                );
                unreadable = true;
            }
        }
    }
    if unreadable {
        None
    } else {
        Some(false)
    }
}

/// Whether the agent's macOS keychain entry exists. `security` without `-w`
/// neither prints the secret nor prompts. `None` when it could not run.
#[cfg(target_os = "macos")]
pub(crate) async fn keychain_login() -> Option<bool> {
    let mut failed = false;
    for account in KEYCHAIN_ACCOUNTS {
        let mut command = tokio::process::Command::new("/usr/bin/security");
        command
            .args([
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                account,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        match tokio::time::timeout(KEYCHAIN_TIMEOUT, command.status()).await {
            Ok(Ok(status)) if status.success() => return Some(true),
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                log::warn!(
                    "[antigravity-accounts] keychain lookup could not start: {:?}",
                    error.kind()
                );
                failed = true;
            }
            Err(_) => {
                log::warn!("[antigravity-accounts] keychain lookup timed out");
                failed = true;
            }
        }
    }
    if failed {
        None
    } else {
        Some(false)
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unused_async)]
pub(crate) async fn keychain_login() -> Option<bool> {
    let _ = (KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNTS, KEYCHAIN_TIMEOUT);
    Some(false)
}

pub(super) struct SystemAntigravityLoginProbe;

#[async_trait::async_trait]
impl AntigravityLoginStatus for SystemAntigravityLoginProbe {
    async fn login_status(&self, home: Option<&Path>) -> Result<Option<bool>, String> {
        let Some(dir) = home
            .map(Path::to_path_buf)
            .or_else(AntigravityAccountStore::host_shared_dir)
        else {
            return Ok(None);
        };
        let from_file = token_file_login(&dir);
        if home.is_none() && from_file == Some(false) {
            // The built-in Account runs without the file-storage flag, so on
            // macOS its login is in the keychain.
            return Ok(keychain_login().await);
        }
        Ok(from_file)
    }

    /// Delete the token files of a non-host home. The host home (`None`) is
    /// never signed out here.
    async fn logout(&self, home: Option<&Path>) -> Result<(), String> {
        let Some(dir) = home else {
            return Ok(());
        };
        let mut failed = false;
        for path in token_files(dir) {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    log::warn!(
                        "[antigravity-accounts] could not delete a login file: {:?}",
                        error.kind()
                    );
                    failed = true;
                }
            }
        }
        if failed {
            Err("Antigravity sign-out did not finish".to_string())
        } else {
            log::info!("[antigravity-accounts] login files deleted");
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
