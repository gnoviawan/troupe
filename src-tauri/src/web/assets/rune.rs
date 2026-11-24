//! The experimental Rune web client (plan R5): a wasm bundle the operator
//! builds and names with `--rune-web-dir`, served at `/rune/` while the
//! `--experimental-rune-client` setting is on. `/?ui=rune` redirects there.
//!
//! Off (the default), `/rune/*` answers 404 and `/?ui=rune` gets the React
//! client unchanged. No Rune bundle is embedded in the binary yet.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Router;

use super::{bundle_response, read_dir_file};

/// Where the client is mounted. The trailing `/` matters: the bundle's
/// `index.html` loads its script and wasm by relative URL.
pub const MOUNT: &str = "/rune/";

/// Put the Rune routes in front of `router`, so the React SPA fallback never
/// answers a `/rune/*` path. `bundle_dir` is the resolved setting
/// ([`crate::web::config::ServerConfig::rune_web_dir`]); `None` is off.
pub fn layer<S>(router: Router<S>, bundle_dir: Option<PathBuf>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let bundle_dir: Option<Arc<Path>> = bundle_dir.map(Arc::from);
    router.layer(middleware::from_fn_with_state(bundle_dir, entry))
}

async fn entry(
    State(bundle_dir): State<Option<Arc<Path>>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if path == "/rune" || path.starts_with(MOUNT) {
        let Some(dir) = bundle_dir else {
            return not_found();
        };
        let Some(file) = path.strip_prefix(MOUNT) else {
            return Redirect::temporary(MOUNT).into_response();
        };
        let response = bundle_response(request.method(), file, request.headers(), |file| {
            read_dir_file(&dir, file)
        });
        if response.status() == StatusCode::NOT_FOUND {
            tracing::warn!(
                target: "termul::web::rune",
                path,
                "Rune web bundle has no such file"
            );
        }
        return response;
    }
    if bundle_dir.is_some() && path == "/" && asks_for_rune(request.uri().query()) {
        return Redirect::temporary(MOUNT).into_response();
    }
    next.run(request).await
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

/// Whether the query string carries the `ui=rune` pair.
fn asks_for_rune(query: Option<&str>) -> bool {
    query.is_some_and(|query| query.split('&').any(|pair| pair == "ui=rune"))
}

#[cfg(test)]
mod tests;
