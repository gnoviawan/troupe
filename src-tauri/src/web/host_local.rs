//! The host-local credential: proof that a `/ws` client runs as the user who
//! started the server, not merely that its socket comes from loopback.
//!
//! A reverse proxy or a tunnel (cloudflared) on the same host makes every
//! remote client a loopback peer, so neither the peer address nor the bearer
//! token may grant the host's own credentials (SSH keys and sessions). A
//! `--desktop` server writes a fresh random credential to
//! `<state dir>/host-local.token` at start, readable by the owner only. A
//! client that presents it at `authenticate` (the Rune desktop app reads the
//! file) on a token-gated loopback connection whose upgrade names no proxy
//! is host-local (`capabilities::Capabilities::host_local`). Browser tabs
//! never present it.
//!
//! This is not a boundary against a bearer-token holder. The token opens a
//! terminal running as the server's user, and that shell can read the file.
//! The HTTP file routes refuse it (`web::host_secrets`), which closes only
//! the cheap path.

use std::path::{Path, PathBuf};

use crate::web::auth::{generate_token, persist_token, WebAuthToken};

/// The credential file under the server's state dir.
pub const HOST_LOCAL_TOKEN_FILE: &str = "host-local.token";

/// The file `issue` writes before renaming it over the credential.
#[must_use]
pub fn staging_path(state_dir: &Path) -> PathBuf {
    state_dir.join(format!(
        "{HOST_LOCAL_TOKEN_FILE}.{}.tmp",
        std::process::id()
    ))
}

/// Mint a credential and write it to `<state_dir>/host-local.token` (owner
/// only, replacing the previous boot's). Both the file and its staging copy
/// are registered as host secrets before any byte is written. Returns the
/// credential and the path.
///
/// # Errors
/// The CSPRNG failed or the file could not be written; the server then runs
/// without host-local clients.
pub fn issue(state_dir: &Path) -> Result<(WebAuthToken, PathBuf), String> {
    let path = state_dir.join(HOST_LOCAL_TOKEN_FILE);
    let staging = staging_path(state_dir);
    crate::web::host_secrets::protect(&path);
    crate::web::host_secrets::protect(&staging);
    let credential = generate_token()?;
    let _ = std::fs::remove_file(&staging);
    persist_token(&staging, &credential)
        .and_then(|()| std::fs::rename(&staging, &path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&staging);
            format!(
                "cannot write the host-local credential to {}: {error}",
                path.display()
            )
        })?;
    Ok((credential, path))
}
