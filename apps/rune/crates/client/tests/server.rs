#[test]
fn projects_against_local_test_server() {
    let Ok(url) = std::env::var("TROUPE_TEST_SERVER") else {
        return;
    };
    let port = url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|p| p.trim_end_matches('/').parse::<u16>().ok());
    assert!(
        port.is_some_and(|p| (8190..=8199).contains(&p)),
        "test server must be http://127.0.0.1 on a reserved test port (8190-8199): {url}"
    );
    let config = client::Config {
        base_url: url,
        token: None,
        device_id: "test".into(),
        client_instance_id: "test".into(),
        seed: 1,
        host_local: None,
    };
    let data =
        client::native::http_send(&config, &client::http::HttpRequest::get("/projects")).unwrap();
    assert!(
        data.get("projects")
            .is_some_and(serde_json::Value::is_array)
    );
}
