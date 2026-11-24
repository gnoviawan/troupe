use std::fs;

use super::*;

fn write_token(home: &Path, file: &str, body: &str) {
    let dir = home.join(AGENT_DIR);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(file), body).unwrap();
}

#[test]
fn a_refresh_token_is_a_login() {
    assert!(has_refresh_token(
        br#"{"refresh_token":"r","access_token":"a"}"#
    ));
    assert!(!has_refresh_token(br#"{"refresh_token":""}"#));
    assert!(!has_refresh_token(br#"{"refresh_token":"  "}"#));
    assert!(!has_refresh_token(br#"{"refresh_token":1}"#));
    assert!(!has_refresh_token(br#"{"access_token":"a"}"#));
    assert!(!has_refresh_token(br#"["refresh_token"]"#));
    assert!(!has_refresh_token(b"not json"));
}

#[test]
fn a_home_without_token_files_is_signed_out() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(token_file_login(root.path()), Some(false));
}

#[test]
fn the_consumer_or_the_business_token_file_signs_a_home_in() {
    let root = tempfile::tempdir().unwrap();
    write_token(root.path(), TOKEN_FILE, r#"{"refresh_token":"r"}"#);
    assert_eq!(token_file_login(root.path()), Some(true));

    let root = tempfile::tempdir().unwrap();
    write_token(root.path(), TOKEN_FILE, r#"{"refresh_token":""}"#);
    write_token(root.path(), BUSINESS_TOKEN_FILE, r#"{"refresh_token":"b"}"#);
    assert_eq!(token_file_login(root.path()), Some(true));
}

#[test]
fn a_token_file_without_a_refresh_token_is_signed_out() {
    let root = tempfile::tempdir().unwrap();
    write_token(root.path(), TOKEN_FILE, "{}");
    assert_eq!(token_file_login(root.path()), Some(false));
}

#[cfg(unix)]
#[test]
fn an_unreadable_token_file_is_unknown() {
    let root = tempfile::tempdir().unwrap();
    // A directory where the file belongs cannot be read as a file.
    fs::create_dir_all(root.path().join(AGENT_DIR).join(TOKEN_FILE)).unwrap();
    assert_eq!(token_file_login(root.path()), None);
    // A confirmed login in the other file still wins.
    write_token(root.path(), BUSINESS_TOKEN_FILE, r#"{"refresh_token":"b"}"#);
    assert_eq!(token_file_login(root.path()), Some(true));
}

#[tokio::test]
async fn logout_deletes_only_the_token_files_of_an_account_home() {
    let root = tempfile::tempdir().unwrap();
    write_token(root.path(), TOKEN_FILE, r#"{"refresh_token":"r"}"#);
    write_token(root.path(), BUSINESS_TOKEN_FILE, r#"{"refresh_token":"b"}"#);
    write_token(root.path(), "settings.json", "{}");
    let probe = SystemAntigravityLoginProbe;
    assert_eq!(probe.login_status(Some(root.path())).await, Ok(Some(true)));
    probe.logout(Some(root.path())).await.unwrap();
    assert_eq!(probe.login_status(Some(root.path())).await, Ok(Some(false)));
    assert!(root.path().join(AGENT_DIR).join("settings.json").is_file());
    // A second sign-out has nothing to delete and still succeeds.
    probe.logout(Some(root.path())).await.unwrap();
    // The host home is never signed out here.
    probe.logout(None).await.unwrap();
}
