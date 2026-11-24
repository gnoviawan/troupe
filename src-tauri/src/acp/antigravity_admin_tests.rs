use std::process::ExitCode;

use super::{login_hint, run_account};
use crate::acp::AntigravityAgentService;

fn names(service: &AntigravityAgentService) -> Vec<String> {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(service.profile_list())
        .unwrap()
        .profiles
        .into_iter()
        .map(|row| row.name)
        .collect()
}

fn run(service: &AntigravityAgentService, args: &[&str]) -> ExitCode {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    run_account(service, &args)
}

#[test]
fn the_login_hint_points_to_the_troupe_ui() {
    let hint = login_hint();
    assert!(hint.contains("Settings → Provider → Antigravity"), "{hint}");
    assert!(
        !hint.contains("troupe-server antigravity account login"),
        "{hint}"
    );
}

#[test]
fn login_of_an_existing_account_fails_with_the_ui_hint() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    assert_eq!(run(&service, &["login", "default"]), ExitCode::from(1));
}

#[test]
fn help_prints_usage_and_adds_nothing() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let before = names(&service);
    for args in [
        &["add", "--help"][..],
        &["add", "-h"],
        &["--help"],
        &["rename", "x", "--help"],
    ] {
        assert_eq!(run(&service, args), ExitCode::SUCCESS, "{args:?}");
    }
    assert_eq!(names(&service), before);
}

#[test]
fn add_rename_default_project_and_remove() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    assert_eq!(run(&service, &["add", "Work"]), ExitCode::SUCCESS);
    let snapshot = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(service.profile_list())
        .unwrap();
    let work = snapshot
        .profiles
        .iter()
        .find(|row| row.name == "Work")
        .unwrap()
        .id
        .clone();
    assert_eq!(
        run(&service, &["rename", &work, "Office"]),
        ExitCode::SUCCESS
    );
    assert_eq!(run(&service, &["default", &work]), ExitCode::SUCCESS);
    assert_eq!(run(&service, &["project", "lab", &work]), ExitCode::SUCCESS);
    assert_eq!(
        run(&service, &["project", "lab", "default"]),
        ExitCode::SUCCESS
    );
    assert_eq!(run(&service, &["remove", &work]), ExitCode::SUCCESS);
    assert_eq!(run(&service, &["login", "missing"]), ExitCode::from(1));
    assert_eq!(run(&service, &["nope"]), ExitCode::from(2));
    let left = names(&service);
    assert!(left.contains(&"Default".to_string()));
    assert!(!left.iter().any(|name| name == "Office" || name == "Work"));
}
