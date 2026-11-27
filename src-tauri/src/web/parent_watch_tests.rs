#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::web::parent_watch::parent_exited;

const SETTLE: Duration = Duration::from_millis(1500);

#[tokio::test]
async fn a_running_process_is_not_reported_as_exited() {
    let watched = tokio::time::timeout(SETTLE, parent_exited(std::process::id())).await;
    assert!(watched.is_err(), "this test process is still running");
}

#[tokio::test]
async fn an_exited_and_reaped_process_is_reported_at_once() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--list")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    tokio::time::timeout(SETTLE, parent_exited(pid))
        .await
        .expect("an exited pid resolves within the poll interval");
}

#[cfg(unix)]
#[tokio::test]
async fn a_killed_process_is_reported_within_one_poll() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let pid = child.id();
    let watcher = tokio::spawn(parent_exited(pid));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!watcher.is_finished(), "the child is still running");
    child.kill().unwrap();
    child.wait().unwrap();
    tokio::time::timeout(SETTLE, watcher)
        .await
        .expect("the kill is seen within one poll")
        .unwrap();
}
