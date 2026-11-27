//! Host files the HTTP file routes never serve: credentials that grant more
//! than the bearer token a request already carries.
//!
//! The `/fs/*` routes resolve any absolute path (ADR-007), so without this
//! list a bearer-token holder could read `<state dir>/host-local.token` and
//! present it at `authenticate` to become host-local. The writer of such a
//! file registers it here ([`protect`]); `fs_api::resolve_request_path`, the
//! content search and the git path routes (diff, stage, unstage, discard)
//! consult it.
//!
//! This guards honest clients and the cheap path. It is not a boundary
//! against a token holder: a bearer token opens a terminal running as the
//! server's user, and that shell reads the file directly.

use std::path::{Path, PathBuf};

use parking_lot::RwLock;

/// Every protected file, as registered. Append-only: a boot registers its
/// files once and tests register their own temp dirs, so entries never
/// conflict.
static PROTECTED: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Register `path` as a host secret. Idempotent.
pub fn protect(path: &Path) {
    let mut protected = PROTECTED.write();
    if !protected.iter().any(|known| known == path) {
        protected.push(path.to_path_buf());
    }
}

/// The protected paths resolved against the filesystem now, so a symlinked
/// state dir, or a file created after registration, still matches the
/// canonical paths the file routes compare.
#[must_use]
pub fn snapshot() -> HostSecrets {
    HostSecrets(
        PROTECTED
            .read()
            .iter()
            .map(|path| canonical(path))
            .collect(),
    )
}

/// `path` (any form) is a protected file, or a directory that holds one.
#[must_use]
pub fn exposed(path: &Path) -> bool {
    snapshot().exposes(&canonical(path))
}

/// The canonical form of `path`, or of its parent joined with its name when
/// `path` does not exist yet, or `path` itself.
fn canonical(path: &Path) -> PathBuf {
    if let Ok(resolved) = path.canonicalize() {
        return resolved;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map_or_else(|_| path.to_path_buf(), |parent| parent.join(name)),
        _ => path.to_path_buf(),
    }
}

/// A point-in-time view of the protected paths, all canonical.
#[derive(Debug, Default)]
pub struct HostSecrets(Vec<PathBuf>);

impl HostSecrets {
    /// `path` (canonical) is a protected file or lies under one.
    #[must_use]
    pub fn covers(&self, path: &Path) -> bool {
        self.0.iter().any(|secret| path.starts_with(secret))
    }

    /// `path` (canonical) is covered, or is a directory that holds a
    /// protected file, so moving it would carry the secret somewhere
    /// unprotected.
    #[must_use]
    pub fn exposes(&self, path: &Path) -> bool {
        self.covers(path) || self.0.iter().any(|secret| secret.starts_with(path))
    }
}
