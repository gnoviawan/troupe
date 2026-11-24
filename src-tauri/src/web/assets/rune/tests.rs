use std::path::Path;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use tower::ServiceExt as _;

use super::layer;
use crate::web::assets::static_service_from;

const REACT_INDEX: &str = "<!doctype html><title>react</title>";
const RUNE_INDEX: &str = "<!doctype html><title>rune</title>";
const WASM: &[u8] = b"\0asm\x01\0\0\0";

/// A served root laid out like a real host: the React bundle, the Rune bundle
/// beside it, and a secret one level above the Rune bundle.
struct Fixture {
    _root: tempfile::TempDir,
    react: std::path::PathBuf,
    rune: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("tempdir");
    let react = root.path().join("react");
    let rune = root.path().join("rune");
    std::fs::create_dir_all(&react).expect("react dir");
    std::fs::create_dir_all(rune.join("fonts")).expect("rune dir");
    std::fs::write(react.join("index.html"), REACT_INDEX).expect("react index");
    std::fs::write(rune.join("index.html"), RUNE_INDEX).expect("rune index");
    std::fs::write(rune.join("app.js"), "export default 1").expect("rune js");
    std::fs::write(rune.join("app_bg.wasm"), WASM).expect("rune wasm");
    std::fs::write(rune.join("fonts/inter.woff2"), b"wOF2").expect("rune font");
    std::fs::write(root.path().join("secret.txt"), "top secret").expect("secret");
    Fixture {
        _root: root,
        react,
        rune,
    }
}

fn app(react: &Path, rune: Option<&Path>) -> Router {
    layer(
        Router::new().fallback_service(static_service_from(react)),
        rune.map(Path::to_path_buf),
    )
}

struct Reply {
    status: StatusCode,
    content_type: Option<String>,
    location: Option<String>,
    body: Vec<u8>,
}

async fn get(app: Router, uri: &str) -> Reply {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let header_value = |name: header::HeaderName| {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().expect("ascii").to_string())
    };
    let content_type = header_value(header::CONTENT_TYPE);
    let location = header_value(header::LOCATION);
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    Reply {
        status,
        content_type,
        location,
        body,
    }
}

#[tokio::test]
async fn serves_the_wasm_as_application_wasm_when_on() {
    let f = fixture();
    let reply = get(app(&f.react, Some(&f.rune)), "/rune/app_bg.wasm").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.content_type.as_deref(), Some("application/wasm"));
    assert_eq!(reply.body, WASM);
}

#[tokio::test]
async fn serves_the_bundle_files_when_on() {
    let f = fixture();
    let cases = [
        ("/rune/", "text/html; charset=utf-8", RUNE_INDEX.as_bytes()),
        (
            "/rune/index.html",
            "text/html; charset=utf-8",
            RUNE_INDEX.as_bytes(),
        ),
        (
            "/rune/app.js",
            "text/javascript; charset=utf-8",
            b"export default 1".as_slice(),
        ),
        ("/rune/fonts/inter.woff2", "font/woff2", b"wOF2".as_slice()),
    ];
    for (uri, content_type, body) in cases {
        let reply = get(app(&f.react, Some(&f.rune)), uri).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}");
        assert_eq!(reply.content_type.as_deref(), Some(content_type), "{uri}");
        assert_eq!(reply.body, body, "{uri}");
    }
}

#[tokio::test]
async fn bare_mount_redirects_to_the_slash_form_when_on() {
    let f = fixture();
    let reply = get(app(&f.react, Some(&f.rune)), "/rune").await;
    assert_eq!(reply.status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(reply.location.as_deref(), Some("/rune/"));
}

#[tokio::test]
async fn every_rune_path_is_404_when_off() {
    let f = fixture();
    for uri in [
        "/rune",
        "/rune/",
        "/rune/index.html",
        "/rune/app_bg.wasm",
        "/rune/route",
    ] {
        let reply = get(app(&f.react, None), uri).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(reply.body, b"404 Not Found", "{uri}");
    }
}

#[tokio::test]
async fn a_missing_bundle_file_is_404_not_the_react_shell() {
    let f = fixture();
    let reply = get(app(&f.react, Some(&f.rune)), "/rune/missing.js").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.body, b"404 Not Found");
}

#[tokio::test]
async fn refuses_paths_that_leave_the_bundle_dir() {
    let f = fixture();
    for uri in [
        "/rune/../secret.txt",
        "/rune/fonts/../../secret.txt",
        "/rune/./app.js",
        "/rune//app.js",
        "/rune/..%2fsecret.txt",
        "/rune/%2e%2e/secret.txt",
        "/rune/..%5csecret.txt",
        "/rune/C:/secret.txt",
    ] {
        let reply = get(app(&f.react, Some(&f.rune)), uri).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(reply.body, b"404 Not Found", "{uri}");
    }
}

#[tokio::test]
async fn ui_rune_redirects_to_the_mount_when_on() {
    let f = fixture();
    for uri in ["/?ui=rune", "/?ui=rune&debug=1", "/?debug=1&ui=rune"] {
        let reply = get(app(&f.react, Some(&f.rune)), uri).await;
        assert_eq!(reply.status, StatusCode::TEMPORARY_REDIRECT, "{uri}");
        assert_eq!(reply.location.as_deref(), Some("/rune/"), "{uri}");
    }
}

#[tokio::test]
async fn ui_rune_falls_through_to_react_when_off() {
    let f = fixture();
    let reply = get(app(&f.react, None), "/?ui=rune").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body, REACT_INDEX.as_bytes());
}

#[tokio::test]
async fn other_paths_and_queries_stay_on_react_when_on() {
    let f = fixture();
    for uri in ["/", "/?ui=react", "/?ui=runes", "/runes", "/projects/rune"] {
        let reply = get(app(&f.react, Some(&f.rune)), uri).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}");
        assert_eq!(reply.body, REACT_INDEX.as_bytes(), "{uri}");
    }
}
