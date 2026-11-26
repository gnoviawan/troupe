//! The HTML preview sub-router through tower oneshot: the control routes and a
//! page fetched by the URL `open` describes.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::Router;
use tower::ServiceExt;

use super::html_preview_api::html_preview_router;

fn app() -> Router {
    html_preview_router::<()>()
}

async fn send(request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = app().oneshot(request).await.expect("router response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, headers, body.to_vec())
}

async fn post(path: &str, body: serde_json::Value) -> serde_json::Value {
    let request = Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request");
    let (status, _, body) = send(request).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    serde_json::from_slice(&body).expect("json body")
}

#[tokio::test]
async fn an_opened_page_serves_the_updated_buffer_until_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let page = dir.path().join("my page.html");
    std::fs::write(&page, "<p>disk</p>").expect("write page");

    let opened = post(
        "/html-preview/open",
        serde_json::json!({ "path": page.to_string_lossy() }),
    )
    .await;
    assert_eq!(opened["success"], true);
    assert_eq!(opened["data"]["entry"], "my page.html");
    assert_eq!(opened["data"]["baseUrl"], "");
    let token = opened["data"]["token"].as_str().expect("token").to_string();

    let updated = post(
        "/html-preview/update",
        serde_json::json!({ "token": token, "content": "<p>buffer</p>" }),
    )
    .await;
    assert_eq!(updated["success"], true);

    let page_url = format!("/preview/{token}/my%20page.html");
    let (status, headers, body) = send(
        Request::get(&page_url)
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"<p>buffer</p>");
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY],
        "sandbox allow-scripts allow-forms allow-modals allow-popups"
    );

    let (status, headers, _) = send(
        Request::get(format!("/preview/{token}/"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(headers[header::LOCATION], "./my%20page.html");

    let closed = post("/html-preview/close", serde_json::json!({ "token": token })).await;
    assert_eq!(closed["success"], true);
    let (status, _, _) = send(
        Request::get(&page_url)
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn open_refuses_a_file_that_is_not_html() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("app.js");
    std::fs::write(&script, "1").expect("write script");

    let refused = post(
        "/html-preview/open",
        serde_json::json!({ "path": script.to_string_lossy() }),
    )
    .await;

    assert_eq!(refused["success"], false);
    assert_eq!(refused["code"], "UNSUPPORTED_FILE");
}
