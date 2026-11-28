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
fn each_ssh_event_becomes_a_typed_ssh_watch_delta() {
    use crate::ssh::{session, sftp};
    use crate::web::desktop_mode::ssh_delta;
    use serde_json::json;
    use troupe_protocol::ws::SshWatchDelta;

    let events = [
        (
            session::EVENT_PROGRESS,
            json!({ "sessionId": "s1", "stage": "authenticating" }),
            "progress",
        ),
        (
            session::EVENT_HOST_KEY,
            json!({
                "sessionId": "s1", "status": "changed", "host": "h", "port": 22,
                "keyType": "ssh-ed25519", "fingerprint": "SHA256:new",
                "previousFingerprints": ["SHA256:old"],
            }),
            "host_key",
        ),
        (
            session::EVENT_HOST_KEY_RESOLVED,
            json!({ "sessionId": "s1" }),
            "host_key_resolved",
        ),
        (
            session::EVENT_ENDED,
            json!({ "sessionId": "s1", "terminalId": null, "reason": "lost",
                    "exitCode": null, "detail": "reset" }),
            "ended",
        ),
        (
            sftp::EVENT_TRANSFER_PROGRESS,
            json!({ "connectionId": "s1", "remotePath": "/r", "localPath": "/l",
                    "bytesTransferred": 1, "totalBytes": 2, "direction": "download",
                    "status": "in-progress", "error": null }),
            "transfer",
        ),
    ];
    for (event, payload, tag) in events {
        let delta = ssh_delta(event, payload).expect(event);
        assert_eq!(delta["event"], tag);
        let typed: SshWatchDelta = serde_json::from_value(delta.clone()).expect(event);
        assert_eq!(serde_json::to_value(typed).unwrap(), delta, "{event}");
    }
    assert_eq!(
        ssh_delta("ssh-port-forward-status-changed", json!(["s1", {}])),
        None
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
