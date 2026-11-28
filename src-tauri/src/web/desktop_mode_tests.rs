#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::web::desktop_mode::{legacy_chat_history_dir, BrowserHostOptions, LaunchFlags};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_string()).collect()
}

#[test]
fn no_launch_flags_leaves_the_arguments_untouched() {
    let (flags, rest) =
        LaunchFlags::take(args(&["--host", "127.0.0.1", "--port", "8190"])).unwrap();
    assert_eq!(flags, LaunchFlags::default());
    assert_eq!(rest, args(&["--host", "127.0.0.1", "--port", "8190"]));
}

#[test]
fn desktop_and_parent_pid_are_taken_off_the_arguments() {
    let (flags, rest) = LaunchFlags::take(args(&[
        "--desktop",
        "--host",
        "127.0.0.1",
        "--parent-pid",
        "4242",
        "--port",
        "8190",
    ]))
    .unwrap();
    assert_eq!(
        flags,
        LaunchFlags {
            desktop: true,
            parent_pid: Some(4242),
            browser_host: None,
        }
    );
    assert_eq!(rest, args(&["--host", "127.0.0.1", "--port", "8190"]));
}

#[test]
fn parent_pid_accepts_the_equals_form() {
    let (flags, rest) = LaunchFlags::take(args(&["--parent-pid=17"])).unwrap();
    assert_eq!(flags.parent_pid, Some(17));
    assert!(!flags.desktop);
    assert!(rest.is_empty());
}

#[test]
fn a_bad_parent_pid_is_refused() {
    for bad in [
        args(&["--parent-pid"]),
        args(&["--parent-pid", "0"]),
        args(&["--parent-pid", "-3"]),
        args(&["--parent-pid=abc"]),
    ] {
        assert!(
            LaunchFlags::take(bad.clone()).is_err(),
            "{bad:?} must be refused"
        );
    }
}

#[test]
fn legacy_chat_history_is_found_only_when_the_directory_exists() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(legacy_chat_history_dir(dir.path()), None);
    std::fs::create_dir(dir.path().join("acp-chat-history")).unwrap();
    assert_eq!(
        legacy_chat_history_dir(dir.path()),
        Some(dir.path().join("acp-chat-history"))
    );
}

#[test]
fn browser_host_flags_come_off_argv_and_are_off_by_default() {
    let (flags, _) = LaunchFlags::take(args(&["--port", "8190"])).unwrap();
    assert_eq!(flags.browser_host, None);
    let (flags, rest) = LaunchFlags::take(args(&[
        "--browser-host",
        "--browser-host-allow-internal",
        "--port",
        "8190",
    ]))
    .unwrap();
    assert_eq!(
        flags.browser_host,
        Some(BrowserHostOptions {
            no_sandbox: false,
            allow_internal: true,
        })
    );
    assert_eq!(rest, args(&["--port", "8190"]));
}

#[test]
fn a_browser_host_switch_without_the_browser_host_is_refused() {
    let refused = LaunchFlags::take(args(&["--browser-host-no-sandbox"])).unwrap_err();
    assert_eq!(
        refused,
        "--browser-host-no-sandbox and --browser-host-allow-internal need --browser-host"
    );
}
