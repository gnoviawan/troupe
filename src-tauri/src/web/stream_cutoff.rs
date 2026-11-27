//! Ends streaming response bodies when a host stops.
//!
//! Axum's graceful shutdown waits for every in-flight HTTP response to
//! finish. A streamed body (a media file to a phone that stopped reading)
//! can hold that wait open forever, and with it a listener stop, a token
//! rotation or the server's exit. Every `serve_router` wraps responses of
//! unknown length so they end once the host begins stopping; responses of a
//! known size pass through untouched.

use axum::body::{Body, HttpBody as _};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use futures_util::StreamExt as _;
use tokio_util::sync::CancellationToken;

/// The middleware. `stop` is the host's `HostShutdown::stop_token`.
pub async fn end_streams_on_stop(
    State(stop): State<CancellationToken>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    if response.body().size_hint().exact().is_some() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let stream = body.into_data_stream().take_until(stop.cancelled_owned());
    Response::from_parts(parts, Body::from_stream(stream))
}
