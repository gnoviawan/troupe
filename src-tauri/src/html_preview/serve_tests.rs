use std::fs;

use axum::http::{header, Method, StatusCode};
use axum::response::Response;

use super::serve::PREVIEW_CSP;
use super::{PreviewErrorCode, Registry, MAX_BUFFER_BYTES, MAX_SESSIONS};

struct Site {
    dir: tempfile::TempDir,
    registry: Registry,
}

impl Site {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let site = dir.path().join("site");
        fs::create_dir(&site).expect("site dir");
        fs::write(site.join("index.html"), "<p>disk</p>").expect("index");
        fs::write(site.join("style.css"), "body{color:red}").expect("css");
        fs::write(site.join("key.pem"), "secret").expect("pem");
        fs::write(dir.path().join("outside.css"), "outside").expect("outside");
        Self {
            dir,
            registry: Registry::default(),
        }
    }

    fn path(&self, relative: &str) -> String {
        self.dir
            .path()
            .join(relative)
            .to_string_lossy()
            .into_owned()
    }

    fn open(&self) -> String {
        self.registry
            .open(&self.path("site/index.html"), String::new())
            .expect("open")
            .token
    }
}

async fn text(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf-8 body")
}

fn header_of<'a>(response: &'a Response, name: header::HeaderName) -> &'a str {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

#[test]
fn open_returns_the_entry_name_and_the_transport_base() {
    let site = Site::new();
    let info = site
        .registry
        .open(
            &site.path("site/index.html"),
            "http://127.0.0.1:4100".into(),
        )
        .expect("open");
    assert_eq!(info.entry, "index.html");
    assert_eq!(info.base_url, "http://127.0.0.1:4100");
    assert_eq!(info.token.len(), 32);
}

#[test]
fn open_refuses_paths_the_media_policy_refuses() {
    let site = Site::new();
    let refused = |path: &str| {
        site.registry
            .open(path, String::new())
            .map(|_| ())
            .unwrap_err()
            .code
    };
    assert_eq!(refused("site/index.html"), PreviewErrorCode::InvalidPath);
    assert_eq!(
        refused(&site.path("site/../site/index.html")),
        PreviewErrorCode::PathTraversal
    );
    assert_eq!(
        refused(&site.path("site/style.css")),
        PreviewErrorCode::NotHtml
    );
    assert_eq!(
        refused(&site.path("site/missing.html")),
        PreviewErrorCode::NotFound
    );
}

#[tokio::test]
async fn the_buffer_is_served_for_the_entry_with_the_sandbox_headers() {
    let site = Site::new();
    let token = site.open();
    site.registry
        .update(&token, "<p>unsaved</p>".into())
        .expect("update");

    let response = site.registry.serve(&token, "index.html", &Method::GET);

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_of(&response, header::CONTENT_TYPE),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        header_of(&response, header::CONTENT_SECURITY_POLICY),
        PREVIEW_CSP
    );
    assert_eq!(
        PREVIEW_CSP,
        "sandbox allow-scripts allow-forms allow-modals allow-popups"
    );
    assert_eq!(
        header_of(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "*"
    );
    assert_eq!(
        header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
        "nosniff"
    );
    assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-store");
    assert_eq!(header_of(&response, header::REFERRER_POLICY), "no-referrer");
    assert_eq!(text(response).await, "<p>unsaved</p>");
}

#[tokio::test]
async fn the_entry_comes_from_disk_until_a_buffer_is_set() {
    let site = Site::new();
    let token = site.open();

    let response = site.registry.serve(&token, "index.html", &Method::GET);

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(text(response).await, "<p>disk</p>");
}

#[tokio::test]
async fn a_sibling_file_is_served_with_its_mime_type() {
    let site = Site::new();
    let token = site.open();

    let response = site.registry.serve(&token, "style.css", &Method::GET);

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_of(&response, header::CONTENT_TYPE),
        "text/css; charset=utf-8"
    );
    assert_eq!(
        header_of(&response, header::CONTENT_SECURITY_POLICY),
        PREVIEW_CSP
    );
    assert_eq!(text(response).await, "body{color:red}");
}

#[test]
fn parent_components_are_refused() {
    let site = Site::new();
    let token = site.open();

    let response = site.registry.serve(&token, "../outside.css", &Method::GET);

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        header_of(&response, header::CONTENT_SECURITY_POLICY),
        PREVIEW_CSP
    );
}

#[test]
fn an_absolute_rest_is_refused() {
    let site = Site::new();
    let token = site.open();
    let absolute = site.path("outside.css");

    let response = site.registry.serve(&token, &absolute, &Method::GET);

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_root_is_refused() {
    let site = Site::new();
    std::os::unix::fs::symlink(site.path("outside.css"), site.path("site/link.css"))
        .expect("symlink");
    let token = site.open();

    let response = site.registry.serve(&token, "link.css", &Method::GET);

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn a_type_off_the_allowlist_is_refused() {
    let site = Site::new();
    let token = site.open();

    let response = site.registry.serve(&token, "key.pem", &Method::GET);

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[test]
fn an_unknown_token_is_not_found() {
    let site = Site::new();
    site.open();

    let response = site.registry.serve(
        "0123456789abcdef0123456789abcdef",
        "index.html",
        &Method::GET,
    );

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        header_of(&response, header::CONTENT_SECURITY_POLICY),
        PREVIEW_CSP
    );
}

#[test]
fn the_session_root_redirects_to_the_entry() {
    let site = Site::new();
    let token = site.open();

    let response = site.registry.serve(&token, "", &Method::GET);

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(header_of(&response, header::LOCATION), "./index.html");
}

#[test]
fn only_get_and_head_are_served() {
    let site = Site::new();
    let token = site.open();

    assert_eq!(
        site.registry
            .serve(&token, "style.css", &Method::HEAD)
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        site.registry
            .serve(&token, "style.css", &Method::POST)
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn a_closed_session_stops_serving_and_refuses_updates() {
    let site = Site::new();
    let token = site.open();

    assert!(site.registry.close(&token));
    assert!(!site.registry.close(&token));
    assert_eq!(
        site.registry
            .serve(&token, "index.html", &Method::GET)
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        site.registry
            .update(&token, String::new())
            .unwrap_err()
            .code,
        PreviewErrorCode::SessionNotFound
    );
}

#[test]
fn an_oversized_buffer_is_refused() {
    let site = Site::new();
    let token = site.open();

    let error = site
        .registry
        .update(&token, "x".repeat(MAX_BUFFER_BYTES + 1))
        .unwrap_err();

    assert_eq!(error.code, PreviewErrorCode::BufferTooLarge);
}

#[test]
fn opening_past_the_cap_evicts_the_oldest_session() {
    let site = Site::new();
    let first = site.open();
    let tokens: Vec<String> = (0..MAX_SESSIONS).map(|_| site.open()).collect();

    assert_eq!(
        site.registry
            .serve(&first, "index.html", &Method::GET)
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        site.registry
            .serve(&tokens[0], "index.html", &Method::GET)
            .status(),
        StatusCode::OK
    );
}
