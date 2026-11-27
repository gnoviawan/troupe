#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::Request;
use futures_util::StreamExt as _;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::web::stream_cutoff::end_streams_on_stop;

fn app(stop: CancellationToken) -> axum::Router {
    axum::Router::new()
        .route(
            "/stalled",
            axum::routing::get(|| async {
                let first = futures_util::stream::once(async {
                    Ok::<_, std::io::Error>(Bytes::from_static(b"first"))
                });
                Body::from_stream(first.chain(futures_util::stream::pending()))
            }),
        )
        .route("/sized", axum::routing::get(|| async { "sized" }))
        .layer(axum::middleware::from_fn_with_state(
            stop,
            end_streams_on_stop,
        ))
}

#[tokio::test]
async fn a_stalled_stream_ends_when_the_host_stops() {
    let stop = CancellationToken::new();
    let response = app(stop.clone())
        .oneshot(Request::get("/stalled").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let mut body = response.into_body().into_data_stream();
    assert_eq!(&body.next().await.unwrap().unwrap()[..], b"first");
    assert!(
        tokio::time::timeout(Duration::from_millis(200), body.next())
            .await
            .is_err(),
        "the stream stalls while the host runs"
    );
    stop.cancel();
    let ended = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("the stream ends once the host stops");
    assert!(ended.is_none());
}

#[tokio::test]
async fn a_sized_response_is_untouched() {
    let stop = CancellationToken::new();
    stop.cancel();
    let response = app(stop)
        .oneshot(Request::get("/sized").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"sized");
}
