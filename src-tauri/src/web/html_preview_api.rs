//! HTML preview web routes: `POST /html-preview/open|update|close` (`IpcBody`
//! envelope, the twins of the `html_preview_*` Tauri commands) and the
//! same-origin `/preview/<token>/*` pages.
//!
//! The control routes sit behind the bearer gate (`"/html-preview/"` in
//! `GATED_PREFIXES`). They are not write routes: a session is an in-memory
//! view of a file the peer can already read through `/fs/read`, so a
//! view-only shared-live peer and a phone without `--allow-remote-writes`
//! preview too. `/preview/<token>/*` is outside the gated prefixes because
//! an iframe cannot send `Authorization`; the session token in the path
//! authorizes it.

use axum::extract::DefaultBodyLimit;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::html_preview::{
    log_file_name, registry, serve_session_file, serve_session_root, PreviewOpenInfo,
    MAX_BUFFER_BYTES,
};
use crate::web::fs_api::IpcBody;

/// A buffer at the cap, JSON-escaped, still fits: every byte of it may become
/// a two-byte escape.
const MAX_UPDATE_BODY_BYTES: usize = 2 * MAX_BUFFER_BYTES + 64 * 1024;

/// `POST /html-preview/open` body.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HtmlPreviewOpenRequest {
    /// Absolute path of the `.html` or `.htm` file.
    path: String,
}

/// `POST /html-preview/update` body.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HtmlPreviewUpdateRequest {
    /// The token `open` returned.
    token: String,
    /// The editor's text, served in place of the file until the session closes.
    content: String,
}

/// `POST /html-preview/close` body.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HtmlPreviewCloseRequest {
    /// The token `open` returned.
    token: String,
}

pub fn html_preview_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/html-preview/open", post(open))
        .route(
            "/html-preview/update",
            post(update).layer(DefaultBodyLimit::max(MAX_UPDATE_BODY_BYTES)),
        )
        .route("/html-preview/close", post(close))
        .route("/preview/{token}/", get(serve_session_root))
        .route("/preview/{token}/{*rest}", get(serve_session_file))
}

/// `POST /html-preview/open`: a session for the file, served same-origin
/// (`baseUrl` is empty).
async fn open(Json(req): Json<HtmlPreviewOpenRequest>) -> Json<IpcBody<PreviewOpenInfo>> {
    let file = log_file_name(&req.path);
    match registry().open(&req.path, String::new()) {
        Ok(info) => {
            tracing::info!(target: "termul::web::html_preview", file = %file, "preview opened");
            Json(IpcBody::ok(info))
        }
        Err(err) => {
            tracing::warn!(
                target: "termul::web::html_preview",
                file = %file,
                code = err.code.as_str(),
                "preview open refused"
            );
            Json(IpcBody::err(err.message, err.code.as_str()))
        }
    }
}

/// `POST /html-preview/update`: serve the editor's unsaved text as the entry.
async fn update(Json(req): Json<HtmlPreviewUpdateRequest>) -> Json<IpcBody<()>> {
    match registry().update(&req.token, req.content) {
        Ok(()) => Json(IpcBody::ok(())),
        Err(err) => {
            tracing::warn!(
                target: "termul::web::html_preview",
                code = err.code.as_str(),
                "preview update refused"
            );
            Json(IpcBody::err(err.message, err.code.as_str()))
        }
    }
}

/// `POST /html-preview/close`: end the session. Idempotent.
async fn close(Json(req): Json<HtmlPreviewCloseRequest>) -> Json<IpcBody<()>> {
    let closed = registry().close(&req.token);
    tracing::info!(target: "termul::web::html_preview", closed, "preview closed");
    Json(IpcBody::ok(()))
}
