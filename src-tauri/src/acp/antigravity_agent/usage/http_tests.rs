use std::fs;

use super::*;

fn home_with(file: &str, body: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("antigravity-acp");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(file), body).unwrap();
    root
}

#[test]
fn a_login_file_is_read_with_its_defaults() {
    let home = home_with(
        "acp_token.json",
        r#"{"refresh_token":"r","client_id":"c","client_secret":"s","project_id":"p"}"#,
    );
    let LoginFile::Login(login) = read_login(home.path()) else {
        panic!("expected a login");
    };
    assert_eq!(login.token_uri(), "https://oauth2.googleapis.com/token");
    assert_eq!(login.project_id(), Some("p"));
    assert_eq!(login.client_id.as_deref(), Some("c"));
    assert_eq!(login.client_secret.as_deref(), Some("s"));

    let home = home_with(
        "acp_business_token.json",
        r#"{"refresh_token":"r","token_uri":"https://example.test/token"}"#,
    );
    let LoginFile::Login(login) = read_login(home.path()) else {
        panic!("expected the business login");
    };
    assert_eq!(login.token_uri(), "https://example.test/token");
    assert_eq!(login.project_id(), None);
}

#[test]
fn a_missing_or_empty_login_is_not_a_login() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(read_login(root.path()), LoginFile::Missing));
    let home = home_with("acp_token.json", r#"{"access_token":"a"}"#);
    assert!(matches!(read_login(home.path()), LoginFile::Invalid));
    let home = home_with("acp_token.json", "not json");
    assert!(matches!(read_login(home.path()), LoginFile::Invalid));
}

#[tokio::test]
async fn a_home_without_a_login_file_is_signed_out() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(
        probe_home(root.path(), false).await,
        ProbeVerdict::SignedOut
    ));
    let home = home_with("acp_token.json", "{}");
    assert!(matches!(
        probe_home(home.path(), false).await,
        ProbeVerdict::SignedOut
    ));
}

#[tokio::test]
async fn a_token_endpoint_that_is_not_https_is_never_called() {
    let home = home_with(
        "acp_token.json",
        r#"{"refresh_token":"r","token_uri":"http://127.0.0.1:9/token"}"#,
    );
    assert!(matches!(
        probe_home(home.path(), false).await,
        ProbeVerdict::Unavailable(USAGE_UNAVAILABLE)
    ));
}

#[test]
fn statuses_map_to_verdicts() {
    assert!(verdict_for_status(200).is_none());
    assert!(matches!(
        verdict_for_status(401),
        Some(ProbeVerdict::SignedOut)
    ));
    assert!(matches!(
        verdict_for_status(403),
        Some(ProbeVerdict::Unavailable(_))
    ));
    assert!(matches!(
        verdict_for_status(404),
        Some(ProbeVerdict::Unavailable(_))
    ));
    assert!(matches!(
        verdict_for_status(503),
        Some(ProbeVerdict::Failed(_))
    ));
}
