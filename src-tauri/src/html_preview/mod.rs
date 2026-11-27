//! Live preview of an `.html` file open in the editor.
//!
//! The preview document must come from a real http origin: the desktop CSP is
//! inherited by `srcdoc`, `data:` and `blob:` frames, which then refuse inline
//! and `data:` scripts. So the host serves each preview session at
//! `/preview/<token>/<entry>`, next to the files beside the entry, on the
//! desktop's loopback canvas listener and on the web router (same-origin).
//!
//! The token in the path is the only credential: browser iframes cannot send
//! `Authorization`, and the desktop loopback iframe is cross-site, so cookies
//! do not reach it. Tokens are never logged. Every response carries a CSP
//! `sandbox` without `allow-same-origin`, so a previewed page stays opaque even
//! when opened top-level on the web origin that holds the auth token.
//!
//! - [`registry`] owns the sessions; the transports are thin adapters over
//!   [`Registry::open`], [`Registry::update`], [`Registry::close`] and
//!   [`Registry::serve`].

mod serve;

#[cfg(test)]
mod serve_tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use axum::extract::Path as AxumPath;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use serde::Serialize;

use crate::canvas::static_server::generate_token;
use crate::media_bytes::log_name;

/// Live sessions kept at once; opening one more drops the oldest.
pub const MAX_SESSIONS: usize = 32;
/// Largest unsaved buffer a session holds.
pub const MAX_BUFFER_BYTES: usize = 5 * 1024 * 1024;
/// Largest file the preview serves from disk.
pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewErrorCode {
    InvalidPath,
    PathTraversal,
    NotFound,
    NotAFile,
    NotHtml,
    ReadError,
    BufferTooLarge,
    SessionNotFound,
    TokenGenerationFailed,
}

impl PreviewErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPath => "INVALID_PATH",
            Self::PathTraversal => "PATH_TRAVERSAL",
            Self::NotFound => "NOT_FOUND",
            Self::NotAFile => "NOT_A_FILE",
            Self::NotHtml => "UNSUPPORTED_FILE",
            Self::ReadError => "READ_ERROR",
            Self::BufferTooLarge => "BUFFER_TOO_LARGE",
            Self::SessionNotFound => "SESSION_NOT_FOUND",
            Self::TokenGenerationFailed => "TOKEN_GENERATION_FAILED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewError {
    pub code: PreviewErrorCode,
    pub message: String,
}

impl PreviewError {
    fn new(code: PreviewErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

// No `Debug`: the token must never reach a log line.
/// Where the renderer points the preview iframe:
/// `<baseUrl>/preview/<token>/<entry>`.
#[derive(Clone, Serialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewOpenInfo {
    /// The session token. It authorizes the preview URLs.
    pub token: String,
    /// The HTML file's name, the page to load under the session.
    pub entry: String,
    /// The origin that serves the preview. Empty on the web, where it is the
    /// page's own origin.
    pub base_url: String,
}

struct PreviewSession {
    /// Canonical directory of the HTML file; nothing outside it is served.
    root: PathBuf,
    entry: String,
    /// The editor's unsaved text, served in place of the entry file.
    buffer: Option<String>,
    opened: u64,
}

#[derive(Default)]
struct Sessions {
    by_token: HashMap<String, PreviewSession>,
    opened: u64,
}

#[derive(Default)]
pub struct Registry {
    sessions: Mutex<Sessions>,
}

/// The process-wide registry: the desktop loopback listener, the shared-live
/// server and the Tauri commands all see the same sessions.
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}

impl Registry {
    /// Start a session for the HTML file at `path`. `base_url` is the origin
    /// the transport serves previews from.
    ///
    /// # Errors
    /// The path policy of the media reads (`INVALID_PATH`, `PATH_TRAVERSAL`,
    /// `NOT_FOUND`, `NOT_A_FILE`), `UNSUPPORTED_FILE` for anything but
    /// `.html`/`.htm`, and `TOKEN_GENERATION_FAILED`.
    pub fn open(&self, path: &str, base_url: String) -> Result<PreviewOpenInfo, PreviewError> {
        let file = resolve_html_file(path)?;
        let (Some(root), Some(entry)) = (file.parent(), file.file_name()) else {
            return Err(PreviewError::new(
                PreviewErrorCode::InvalidPath,
                "the file has no parent directory",
            ));
        };
        let entry = entry.to_string_lossy().into_owned();
        let token = generate_token().map_err(|reason| {
            PreviewError::new(
                PreviewErrorCode::TokenGenerationFailed,
                format!("failed to mint the preview token: {reason}"),
            )
        })?;
        let mut sessions = self.sessions.lock();
        sessions.opened += 1;
        let opened = sessions.opened;
        if sessions.by_token.len() >= MAX_SESSIONS {
            evict_oldest(&mut sessions.by_token);
        }
        sessions.by_token.insert(
            token.clone(),
            PreviewSession {
                root: root.to_path_buf(),
                entry: entry.clone(),
                buffer: None,
                opened,
            },
        );
        Ok(PreviewOpenInfo {
            token,
            entry,
            base_url,
        })
    }

    /// Serve `content` in place of the session's entry file.
    ///
    /// # Errors
    /// `BUFFER_TOO_LARGE` past [`MAX_BUFFER_BYTES`]; `SESSION_NOT_FOUND` for a
    /// closed or evicted session.
    pub fn update(&self, token: &str, content: String) -> Result<(), PreviewError> {
        if content.len() > MAX_BUFFER_BYTES {
            return Err(PreviewError::new(
                PreviewErrorCode::BufferTooLarge,
                format!(
                    "the buffer is too large to preview ({} bytes, max {MAX_BUFFER_BYTES})",
                    content.len()
                ),
            ));
        }
        let mut sessions = self.sessions.lock();
        let Some(session) = sessions.by_token.get_mut(token) else {
            return Err(PreviewError::new(
                PreviewErrorCode::SessionNotFound,
                "the preview session is closed",
            ));
        };
        session.buffer = Some(content);
        Ok(())
    }

    /// End a session. Idempotent; `false` when there was none.
    pub fn close(&self, token: &str) -> bool {
        self.sessions.lock().by_token.remove(token).is_some()
    }

    /// End every session, so no `/preview/<token>/` URL a remote device
    /// learned keeps serving files once its access is revoked. Returns how
    /// many were open.
    pub fn close_all(&self) -> usize {
        let mut sessions = self.sessions.lock();
        let open = sessions.by_token.len();
        sessions.by_token.clear();
        open
    }

    /// The session's root, entry name and buffer, copied out of the lock.
    fn lookup(&self, token: &str) -> Option<(PathBuf, String, Option<String>)> {
        let sessions = self.sessions.lock();
        let session = sessions.by_token.get(token)?;
        Some((
            session.root.clone(),
            session.entry.clone(),
            session.buffer.clone(),
        ))
    }
}

fn evict_oldest(sessions: &mut HashMap<String, PreviewSession>) {
    let oldest = sessions
        .iter()
        .min_by_key(|(_, session)| session.opened)
        .map(|(token, _)| token.clone());
    if let Some(token) = oldest {
        let evicted = sessions.remove(&token);
        if let Some(session) = evicted {
            log::info!(
                "[html-preview] evicted the oldest session file={} (max {MAX_SESSIONS})",
                crate::commands::sanitize_log_field(&session.entry)
            );
        }
    }
}

fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"))
}

/// The canonical HTML file `raw` names, under the policy `read_media_bytes`
/// and `/fs/raw` apply: an absolute path (verbatim prefix stripped), no `..`
/// component, symlinks resolved.
fn resolve_html_file(raw: &str) -> Result<PathBuf, PreviewError> {
    let candidate = PathBuf::from(crate::path_validation::strip_verbatim_prefix(raw).as_ref());
    if !candidate.is_absolute() {
        return Err(PreviewError::new(
            PreviewErrorCode::InvalidPath,
            "preview path must be absolute",
        ));
    }
    let resolved =
        crate::web::fs_api::resolve_request_path(&candidate).map_err(|(message, code)| {
            let code = if code == "PATH_TRAVERSAL" {
                PreviewErrorCode::PathTraversal
            } else {
                PreviewErrorCode::ReadError
            };
            PreviewError::new(code, message)
        })?;
    if !is_html(&candidate) || !is_html(&resolved) {
        return Err(PreviewError::new(
            PreviewErrorCode::NotHtml,
            "only .html and .htm files can be previewed",
        ));
    }
    let metadata = std::fs::metadata(&resolved).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            PreviewError::new(PreviewErrorCode::NotFound, "file not found")
        } else {
            PreviewError::new(
                PreviewErrorCode::ReadError,
                format!("failed to read file: {err}"),
            )
        }
    })?;
    if !metadata.is_file() {
        return Err(PreviewError::new(
            PreviewErrorCode::NotAFile,
            "path is not a regular file",
        ));
    }
    Ok(resolved)
}

/// The file name of `path` for a log line, control characters escaped.
pub fn log_file_name(path: &str) -> String {
    crate::commands::sanitize_log_field(&log_name(Path::new(path)))
}

/// `GET /preview/<token>/`: redirects to the session's entry.
pub async fn serve_session_root(method: Method, AxumPath(token): AxumPath<String>) -> Response {
    serve_off_runtime(token, String::new(), method).await
}

/// `GET /preview/<token>/<rest>`: the entry (or its buffer) and its siblings.
pub async fn serve_session_file(
    method: Method,
    AxumPath((token, rest)): AxumPath<(String, String)>,
) -> Response {
    serve_off_runtime(token, rest, method).await
}

/// [`Registry::serve`] on the blocking pool: it reads files up to
/// [`MAX_FILE_BYTES`].
async fn serve_off_runtime(token: String, rest: String, method: Method) -> Response {
    tokio::task::spawn_blocking(move || registry().serve(&token, &rest, &method))
        .await
        .unwrap_or_else(|err| {
            log::error!("[html-preview] serve task failed: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
}
