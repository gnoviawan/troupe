use super::*;
#[test]
fn extract_quoted() {
    let h = r#"Bearer resource_metadata="https://x.test/.well-known/oauth-protected-resource""#;
    assert_eq!(
        extract_resource_metadata_url(h),
        Some("https://x.test/.well-known/oauth-protected-resource".into())
    );
}
#[test]
fn extract_missing() {
    assert!(extract_resource_metadata_url("Bearer realm=\"t\"").is_none());
}
#[test]
fn auth_required() {
    assert!(is_auth_required(r#"Bearer resource_metadata="https://x""#));
    assert!(is_auth_required(r#"Bearer error="invalid_token""#));
    assert!(is_auth_required("Bearer"));
    assert!(is_auth_required(r#"Basic realm="t", Bearer"#));
    assert!(!is_auth_required("Basic realm=\"t\""));
    assert!(!is_auth_required(r#"Basic realm="bearer""#));
    assert!(!is_auth_required(""));
}

#[test]
fn extract_www_authenticate_prefers_bearer_across_multiple_headers() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.append(
        reqwest::header::WWW_AUTHENTICATE,
        reqwest::header::HeaderValue::from_static(r#"Basic realm="fallback""#),
    );
    headers.append(
        reqwest::header::WWW_AUTHENTICATE,
        reqwest::header::HeaderValue::from_static(
            r#"Bearer resource_metadata="https://x.test/.well-known/oauth-protected-resource""#,
        ),
    );
    assert_eq!(
        extract_www_authenticate(&headers).as_deref(),
        Some(r#"Bearer resource_metadata="https://x.test/.well-known/oauth-protected-resource""#)
    );
}
#[test]
fn key_normalizes() {
    assert_eq!(
        keychain_key("https://x/mcp"),
        keychain_key("https://x/mcp/")
    );
}
#[test]
fn expiry() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(is_token_expired(&StoredToken {
        access_token: "x".into(),
        refresh_token: None,
        expires_at: Some(now - 1),
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into()
    }));
    assert!(!is_token_expired(&StoredToken {
        access_token: "x".into(),
        refresh_token: None,
        expires_at: Some(now + 3600),
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into()
    }));
    assert!(!is_token_expired(&StoredToken {
        access_token: "x".into(),
        refresh_token: None,
        expires_at: None,
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into()
    }));
}

#[test]
fn token_file_path_normalizes_trailing_slash() {
    // Equivalent URLs (differing only by a trailing slash) MUST produce
    // the same token file path so a probe and a connect that differ only
    // by the trailing slash reuse the same stored token.
    let a = token_file_path("https://x/mcp").unwrap();
    let b = token_file_path("https://x/mcp/").unwrap();
    assert_eq!(a, b, "trailing slash must normalize to the same file");
}

#[test]
fn expired_token_with_refresh_is_treatable_as_expired() {
    // A token that is already expired and has a refresh_token must be
    // reported as expired so `get_valid_token` attempts a refresh. This
    // is the precondition for the refresh path being reachable at all.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expired = StoredToken {
        access_token: "x".into(),
        refresh_token: Some("r".into()),
        expires_at: Some(now - 1),
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into(),
    };
    assert!(is_token_expired(&expired));
}

thread_local! {
    /// Test-only token directory for this thread. `token_file_path` uses it
    /// instead of the real store, so a test that sets it (through
    /// [`TokenDirGuard`]) never touches the user's tokens. Thread-local, not
    /// an env var, so parallel tests that read the env stay unaffected; a
    /// `#[tokio::test]` runtime and the tasks it spawns share one thread.
    pub(crate) static TOKEN_DIR_OVERRIDE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Points this thread's token store at a fresh temp dir until dropped.
pub(crate) struct TokenDirGuard(std::path::PathBuf);

impl TokenDirGuard {
    pub(crate) fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "termul-mcp-oauth-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        TOKEN_DIR_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(dir.clone()));
        Self(dir)
    }
}

impl Drop for TokenDirGuard {
    fn drop(&mut self) {
        TOKEN_DIR_OVERRIDE.with(|slot| *slot.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An MCP server that publishes no OAuth metadata, so a client falls back to
/// its legacy `/token` and `/register` endpoints. Answers every token request
/// with `body` and records the form bodies it received; each registration
/// gets the next client id (`client-1`, `client-2`, ...).
pub(crate) async fn serve_token_endpoint(
    body: serde_json::Value,
) -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
    serve_token_endpoint_with_status(body, axum::http::StatusCode::OK).await
}

/// Like [`serve_token_endpoint`] but answers with `status`, so a test can drive
/// the error path (400 + `invalid_grant` is a rejected refresh).
pub(crate) async fn serve_token_endpoint_with_status(
    body: serde_json::Value,
    status: axum::http::StatusCode,
) -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
    let requests = std::sync::Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let registrations = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = axum::Router::new()
        .route(
            "/token",
            axum::routing::post(move |form: String| {
                let seen = seen.clone();
                let body = body.clone();
                async move {
                    seen.lock().push(form);
                    (status, axum::Json(body))
                }
            }),
        )
        .route(
            "/register",
            axum::routing::post(move |axum::Json(request): axum::Json<serde_json::Value>| {
                let n = registrations.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                async move {
                    axum::Json(serde_json::json!({
                        "client_id": format!("client-{n}"),
                        "redirect_uris": request["redirect_uris"],
                    }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (url, requests)
}

pub(crate) fn expired_token_with_refresh(url: &str) -> StoredToken {
    StoredToken {
        access_token: "stale-access".into(),
        refresh_token: Some("stored-refresh".into()),
        expires_at: Some(1),
        client_id: "termul-client".into(),
        issuer: url.into(),
        server_url: url.into(),
    }
}

/// Issue #1345: an expired token that has a refresh token is refreshed at the
/// token endpoint and stored, not deleted.
#[tokio::test]
async fn expired_token_with_refresh_token_is_refreshed() {
    let (url, requests) = serve_token_endpoint(serde_json::json!({
        "access_token": "fresh-access",
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
    .await;
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();
    let generation = token_generation(&url);

    let token = get_valid_token(&url).await;
    let stored = load_stored_token(&url).unwrap();
    let refreshed_generation = token_generation(&url);
    delete_stored_token(&url).unwrap();

    assert_eq!(token.unwrap().as_deref(), Some("fresh-access"));
    assert_eq!(refreshed_generation, generation + 1);
    let stored = stored.expect("the refreshed token is stored");
    assert_eq!(stored.access_token, "fresh-access");
    assert_eq!(stored.refresh_token.as_deref(), Some("stored-refresh"));
    let requests = requests.lock();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].contains("grant_type=refresh_token")
            && requests[0].contains("refresh_token=stored-refresh"),
        "{}",
        requests[0]
    );
}

/// Issue #1364: two concurrent `get_valid_token` calls share one refresh
/// against a server that rotates refresh tokens. One `/token` POST goes out and
/// both callers get the new access token. A second POST would send the rotated
/// refresh token, draw `invalid_grant` on a real server, and delete the fresh
/// token.
#[tokio::test]
async fn concurrent_get_valid_token_calls_share_one_refresh() {
    let _tokens = TokenDirGuard::new("single-flight");
    let (url, requests) = serve_token_endpoint(serde_json::json!({
        "access_token": "fresh-access",
        "token_type": "Bearer",
        "refresh_token": "rotated-refresh",
        "expires_in": 3600,
    }))
    .await;
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();

    let (first, second) = tokio::join!(get_valid_token(&url), get_valid_token(&url));

    assert_eq!(first.unwrap().as_deref(), Some("fresh-access"));
    assert_eq!(second.unwrap().as_deref(), Some("fresh-access"));
    assert_eq!(requests.lock().len(), 1);
    let stored = load_stored_token(&url).unwrap().expect("token kept");
    assert_eq!(stored.refresh_token.as_deref(), Some("rotated-refresh"));
}

/// Issue #1364: a refresh that fails on a connection error keeps the stored
/// token so the next call can retry. The chat continues without it.
#[tokio::test]
async fn refresh_connection_error_keeps_stored_token() {
    let _tokens = TokenDirGuard::new("conn-error");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    drop(listener);
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();

    let token = get_valid_token(&url).await;

    assert_eq!(token.unwrap(), None);
    assert!(
        load_stored_token(&url).unwrap().is_some(),
        "a connection error must keep the token"
    );
}

/// Issue #1364: a refresh that outlives [`REFRESH_TIMEOUT`] keeps the stored
/// token and returns no token instead of pinning the caller.
#[tokio::test(start_paused = true)]
async fn refresh_timeout_keeps_stored_token() {
    let _tokens = TokenDirGuard::new("timeout");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();

    let token = get_valid_token(&url).await;

    assert_eq!(token.unwrap(), None);
    assert!(
        load_stored_token(&url).unwrap().is_some(),
        "a timeout must keep the token"
    );
}

/// Issue #1364: the one failure that still deletes. The authorization server
/// rejects the refresh token outright (`invalid_grant`).
#[tokio::test]
async fn rejected_refresh_deletes_stored_token() {
    let _tokens = TokenDirGuard::new("rejected");
    let (url, _requests) = serve_token_endpoint_with_status(
        serde_json::json!({
            "error": "invalid_grant",
            "error_description": "refresh token revoked",
        }),
        axum::http::StatusCode::BAD_REQUEST,
    )
    .await;
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();

    let token = get_valid_token(&url).await;

    assert_eq!(token.unwrap(), None);
    assert!(load_stored_token(&url).unwrap().is_none());
}

#[tokio::test]
async fn refresh_without_expires_in_clears_expired_timestamp() {
    let _tokens = TokenDirGuard::new("refresh-no-exp");
    let (url, _requests) = serve_token_endpoint(serde_json::json!({
        "access_token": "fresh-access",
        "token_type": "Bearer",
    }))
    .await;
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();

    let token = get_valid_token(&url).await.unwrap();
    assert_eq!(token.as_deref(), Some("fresh-access"));
    let stored = load_stored_token(&url).unwrap().expect("stored token");
    assert_eq!(
        stored.expires_at, None,
        "omitted expires_in must not retain the old expired timestamp"
    );
}

#[tokio::test]
async fn discover_follows_401_challenge_resource_metadata_and_scope() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let prm_url = format!("{base}/custom/prm");
    let issuer = format!("{base}/auth");
    let resource_url = format!("{base}/api/mcp");

    let prm_header = format!(r#"Bearer resource_metadata="{prm_url}", scope="custom.read""#);
    let prm_body = serde_json::json!({
        "resource": resource_url,
        "authorization_servers": [issuer],
    });
    let as_meta = serde_json::json!({
        "issuer": format!("{base}/auth"),
        "authorization_endpoint": format!("{base}/auth/authorize"),
        "token_endpoint": format!("{base}/auth/token"),
        "registration_endpoint": format!("{base}/auth/register"),
        "response_types_supported": ["code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    });

    let app = axum::Router::new()
        .route(
            "/api/mcp",
            axum::routing::post(move |headers: axum::http::HeaderMap| {
                let header = prm_header.clone();
                async move {
                    let has_tenant =
                        headers.get("x-tenant-id").and_then(|v| v.to_str().ok()) == Some("acme");
                    if has_tenant {
                        (
                            axum::http::StatusCode::UNAUTHORIZED,
                            [(axum::http::header::WWW_AUTHENTICATE, header)],
                            "unauthorized",
                        )
                    } else {
                        (
                            axum::http::StatusCode::UNAUTHORIZED,
                            [(
                                axum::http::header::WWW_AUTHENTICATE,
                                "Bearer realm=\"wrong-tenant\"".to_string(),
                            )],
                            "unauthorized",
                        )
                    }
                }
            }),
        )
        .route(
            "/custom/prm",
            axum::routing::get(move || {
                let body = prm_body.clone();
                async move { axum::Json(body) }
            }),
        )
        .route(
            "/.well-known/oauth-authorization-server/auth",
            axum::routing::get(move || {
                let body = as_meta.clone();
                async move { axum::Json(body) }
            }),
        )
        .route(
            "/auth/register",
            axum::routing::post(
                |axum::Json(req): axum::Json<serde_json::Value>| async move {
                    axum::Json(serde_json::json!({
                        "client_id": "challenge-client",
                        "redirect_uris": req["redirect_uris"],
                    }))
                },
            ),
        );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let configured_headers = [crate::acp::mcp_probe::McpNameValuePair {
        name: "X-Tenant-Id".to_string(),
        value: "acme".to_string(),
    }];
    let session = start_authorization_session(
        &resource_url,
        "http://127.0.0.1:9999/callback",
        &configured_headers,
    )
    .await
    .expect("challenge discovery should succeed");
    let auth_url = url::Url::parse(session.get_authorization_url()).unwrap();
    assert_eq!(auth_url.path(), "/auth/authorize");
    let scope = auth_url
        .query_pairs()
        .find_map(|(k, v)| (k == "scope").then(|| v.into_owned()))
        .unwrap_or_default();
    assert!(
        scope.split_whitespace().any(|s| s == "custom.read"),
        "expected challenge scope in auth url: {auth_url}"
    );
}

/// Start a local mock MCP + OAuth 2.1 AS that advertises RFC 9207
/// `authorization_response_iss_parameter_supported: true` (matching Craftwork
/// and BetterAuth/Auth0/Keycloak servers).
async fn serve_rfc9207_server(
    token_status: axum::http::StatusCode,
    token_body: serde_json::Value,
) -> (String, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let issuer = format!("{base}/api/auth");
    let resource_url = format!("{base}/api/mcp");
    let prm_url = format!("{base}/.well-known/oauth-protected-resource/api/mcp");

    let prm_header = format!(r#"Bearer resource_metadata="{prm_url}""#);
    let prm_body = serde_json::json!({
        "resource": resource_url,
        "authorization_servers": [issuer],
    });
    let as_meta = serde_json::json!({
        "issuer": format!("{base}/api/auth"),
        "authorization_endpoint": format!("{base}/api/auth/oauth2/authorize"),
        "token_endpoint": format!("{base}/api/auth/oauth2/token"),
        "registration_endpoint": format!("{base}/api/auth/oauth2/register"),
        "response_types_supported": ["code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "authorization_response_iss_parameter_supported": true,
    });

    let app = axum::Router::new()
        .route(
            "/api/mcp",
            axum::routing::post(move || {
                let header = prm_header.clone();
                async move {
                    (
                        axum::http::StatusCode::UNAUTHORIZED,
                        [(axum::http::header::WWW_AUTHENTICATE, header)],
                        "unauthorized",
                    )
                }
            }),
        )
        .route(
            "/.well-known/oauth-protected-resource/api/mcp",
            axum::routing::get(move || {
                let body = prm_body.clone();
                async move { axum::Json(body) }
            }),
        )
        .route(
            "/.well-known/oauth-authorization-server/api/auth",
            axum::routing::get(move || {
                let body = as_meta.clone();
                async move { axum::Json(body) }
            }),
        )
        .route(
            "/api/auth/oauth2/register",
            axum::routing::post(
                |axum::Json(req): axum::Json<serde_json::Value>| async move {
                    axum::Json(serde_json::json!({
                        "client_id": "rfc9207-client",
                        "redirect_uris": req["redirect_uris"],
                    }))
                },
            ),
        )
        .route(
            "/api/auth/oauth2/token",
            axum::routing::post(move || {
                let body = token_body.clone();
                async move { (token_status, axum::Json(body)) }
            }),
        );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (resource_url, format!("{base}/api/auth"))
}

async fn send_raw_http(port: u16, request: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).await.unwrap();
    buf
}

#[tokio::test]
async fn desktop_flow_validates_rfc9207_iss_and_ignores_preconnect_and_wrong_state() {
    let _tokens = TokenDirGuard::new("desktop-rfc9207");
    let (server_url, issuer) = serve_rfc9207_server(
        axum::http::StatusCode::OK,
        serde_json::json!({
            "access_token": "rfc9207-access",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "rfc9207-refresh",
        }),
    )
    .await;

    let cb_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = cb_listener.local_addr().unwrap().port();
    let redirect_uri = format!("http://127.0.0.1:{port}{OAUTH_REDIRECT_PATH}");

    let session = start_authorization_session(&server_url, &redirect_uri, &[])
        .await
        .unwrap();
    let auth_url = url::Url::parse(session.get_authorization_url()).unwrap();
    let state = auth_url
        .query_pairs()
        .find_map(|(k, v)| (k == "state").then(|| v.into_owned()))
        .unwrap();

    // Open an idle speculative preconnect socket that never sends bytes.
    let _preconnect = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();

    let server_url_clone = server_url.clone();
    let state_clone = state.clone();
    let flow_task = tokio::spawn(async move {
        complete_desktop_flow(
            cb_listener,
            port,
            &state_clone,
            &server_url_clone,
            session,
            std::time::Duration::from_secs(5),
        )
        .await
    });

    // Stray wrong-state callback should return 404 and keep the flow alive.
    let wrong_resp = send_raw_http(
        port,
        &format!("GET /callback?code=bad&state=wrong HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    )
    .await;
    assert!(wrong_resp.starts_with("HTTP/1.1 404"));

    // Real callback carrying RFC 9207 `iss` succeeds even while `_preconnect` is open.
    let encoded_iss = url::form_urlencoded::byte_serialize(issuer.as_bytes()).collect::<String>();
    let ok_resp = send_raw_http(
        port,
        &format!(
            "GET /callback?code=good-code&state={state}&iss={encoded_iss} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        ),
    )
    .await;
    assert!(ok_resp.starts_with("HTTP/1.1 200"));
    assert!(ok_resp.contains("Authorization complete"));

    let stored = flow_task.await.unwrap().expect("desktop flow succeeds");
    assert_eq!(stored.access_token, "rfc9207-access");
    assert_eq!(stored.refresh_token.as_deref(), Some("rfc9207-refresh"));
}

#[tokio::test]
async fn desktop_flow_handles_error_redirect_immediately() {
    let _tokens = TokenDirGuard::new("desktop-denied");
    let (server_url, _issuer) = serve_rfc9207_server(
        axum::http::StatusCode::OK,
        serde_json::json!({ "access_token": "x", "token_type": "Bearer" }),
    )
    .await;

    let cb_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = cb_listener.local_addr().unwrap().port();
    let redirect_uri = format!("http://127.0.0.1:{port}{OAUTH_REDIRECT_PATH}");
    let session = start_authorization_session(&server_url, &redirect_uri, &[])
        .await
        .unwrap();
    let auth_url = url::Url::parse(session.get_authorization_url()).unwrap();
    let state = auth_url
        .query_pairs()
        .find_map(|(k, v)| (k == "state").then(|| v.into_owned()))
        .unwrap();

    let state_clone = state.clone();
    let server_url_clone = server_url.clone();
    let flow_task = tokio::spawn(async move {
        complete_desktop_flow(
            cb_listener,
            port,
            &state_clone,
            &server_url_clone,
            session,
            std::time::Duration::from_secs(30),
        )
        .await
    });

    let resp = send_raw_http(
        port,
        &format!(
            "GET /callback?error=access_denied&error_description=User+denied&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        ),
    )
    .await;
    assert!(resp.starts_with("HTTP/1.1 400"));

    let err = flow_task.await.unwrap().unwrap_err();
    assert_eq!(err.code(), "OAUTH_DENIED");
    assert!(err.to_string().contains("User denied"));
}

#[tokio::test]
async fn desktop_flow_returns_502_html_when_token_exchange_fails() {
    let _tokens = TokenDirGuard::new("desktop-exchange-fail");
    let (server_url, issuer) = serve_rfc9207_server(
        axum::http::StatusCode::BAD_REQUEST,
        serde_json::json!({ "error": "invalid_grant" }),
    )
    .await;

    let cb_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = cb_listener.local_addr().unwrap().port();
    let redirect_uri = format!("http://127.0.0.1:{port}{OAUTH_REDIRECT_PATH}");
    let session = start_authorization_session(&server_url, &redirect_uri, &[])
        .await
        .unwrap();
    let auth_url = url::Url::parse(session.get_authorization_url()).unwrap();
    let state = auth_url
        .query_pairs()
        .find_map(|(k, v)| (k == "state").then(|| v.into_owned()))
        .unwrap();

    let state_clone = state.clone();
    let server_url_clone = server_url.clone();
    let flow_task = tokio::spawn(async move {
        complete_desktop_flow(
            cb_listener,
            port,
            &state_clone,
            &server_url_clone,
            session,
            std::time::Duration::from_secs(5),
        )
        .await
    });

    let encoded_iss = url::form_urlencoded::byte_serialize(issuer.as_bytes()).collect::<String>();
    let resp = send_raw_http(
        port,
        &format!(
            "GET /callback?code=bad-code&state={state}&iss={encoded_iss} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        ),
    )
    .await;
    assert!(resp.starts_with("HTTP/1.1 502"));
    assert!(resp.contains("Authorization failed"));

    let err = flow_task.await.unwrap().unwrap_err();
    assert_eq!(err.code(), "OAUTH_EXCHANGE_FAILED");
}
