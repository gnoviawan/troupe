use serde_json::json;

use super::*;

fn profile(id: &str, auth: &str) -> Profile {
    Profile {
        id: id.into(),
        name: format!("host {id}"),
        host: "prod.example".into(),
        port: 2222,
        username: "deploy".into(),
        auth_method: auth.into(),
        ..Profile::default()
    }
}

fn prompt(attempt: &str, status: &str) -> HostKeyPrompt {
    HostKeyPrompt {
        session_id: attempt.into(),
        status: status.into(),
        host: "prod.example".into(),
        port: 2222,
        key_type: "ssh-ed25519".into(),
        fingerprint: "SHA256:new".into(),
        previous_fingerprints: if status == "changed" { vec!["SHA256:old".into()] } else { Vec::new() },
    }
}

fn request(effect: &Effect) -> (&'static str, Value) {
    match effect {
        Effect::Request { kind, payload, .. } => (*kind, payload.clone()),
        other => panic!("not a request: {other:?}"),
    }
}

#[test]
fn an_open_connects_a_focused_tab_and_its_answer_attaches_the_shell() {
    let mut s = State::default();
    s.set_profiles(vec![profile("p1", "key")]);
    let effects = s.open_session("t1", "p1");
    assert_eq!(s.active_tab, "t1");
    assert!(s.workspace_open);
    assert_eq!(
        request(&effects[0]),
        ("ssh_session_open", json!({ "request": { "sessionId": "t1", "profileId": "p1", "cols": 120, "rows": 32 } }))
    );
    s.progressed("t1", "authenticating");
    assert_eq!(s.session("t1").unwrap().stage.as_deref(), Some("authenticating"));

    let effects = s.opened(
        "t1",
        Opened::Ok { pty_id: "pty-1".into(), claim: "c".into(), connected_at: Some("now".into()), secret_saved: true },
    );
    assert_eq!(effects, [Effect::Attach { tab: "t1".into(), pty_id: "pty-1".into(), claim: "c".into() }, Effect::ReloadProfiles]);
    let session = s.session("t1").unwrap();
    assert_eq!((session.status, session.stage.as_deref()), (Status::Connected, None));
}

#[test]
fn a_secret_goes_into_the_open_request_only() {
    let mut s = State::default();
    s.set_profiles(vec![profile("p1", "password")]);
    s.open_session("t1", "p1");
    let effects = s.opened("t1", Opened::Err(SessionError { code: "SSH_PASSWORD_REQUIRED".into(), detail: String::new() }));
    assert!(matches!(effects[0], Effect::Log { warn: true, .. }));
    let session = s.session("t1").unwrap();
    assert_eq!(session.status, Status::AwaitingInput);
    assert_eq!(
        session.credential_prompt,
        Some(CredentialPrompt { kind: prompt::SecretKind::Password, reason: prompt::PromptReason::Required })
    );

    let effects = s.retry("t1", "a2", Some("hunter2"), true);
    assert_eq!(request(&effects[0]).1["request"]["secret"], "hunter2");
    assert_eq!(request(&effects[0]).1["request"]["saveSecret"], true);
    assert_eq!(s.session("t1").unwrap().attempt_id, "a2");
    assert!(!serde_json::to_string(&s).unwrap().contains("hunter2"), "the store never keeps a secret");
}

#[test]
fn a_changed_key_waits_for_explicit_trust_naming_the_shown_fingerprint() {
    let mut s = State::default();
    s.set_profiles(vec![profile("p1", "agent")]);
    s.open_session("t1", "p1");
    assert!(s.host_key_asked(prompt("t1", "changed")).is_empty());
    let session = s.session("t1").unwrap();
    assert_eq!(session.status, Status::AwaitingInput);
    let shown = session.host_key_prompt.as_ref().unwrap();
    assert_eq!(shown.title(), "Server identity changed");
    assert_eq!(shown.trust_label(), "Replace key and connect");
    assert_eq!(shown.previous_fingerprints, ["SHA256:old"]);

    let effects = s.respond_host_key("t1", true);
    assert_eq!(
        request(&effects[0]),
        ("ssh_session_respond_host_key", json!({ "sessionId": "t1", "trust": true, "fingerprint": "SHA256:new" }))
    );
    assert!(s.session("t1").unwrap().host_key_prompt.is_none());
    assert!(s.respond_host_key("t1", true).is_empty(), "one answer per question");
}

#[test]
fn a_rejection_names_no_fingerprint_and_an_orphan_question_is_ignored() {
    let mut s = State::default();
    s.open_session("t1", "p1");
    s.host_key_asked(prompt("t1", "unknown"));
    assert_eq!(request(&s.respond_host_key("t1", false)[0]).1, json!({ "sessionId": "t1", "trust": false }));

    let effects = s.host_key_asked(prompt("nobody", "unknown"));
    assert!(effects.is_empty(), "another client owns this attempt");
}

#[test]
fn a_resolved_question_leaves_the_tab_connecting() {
    let mut s = State::default();
    s.open_session("t1", "p1");
    s.host_key_asked(prompt("t1", "unknown"));
    s.host_key_resolved("t1");
    let session = s.session("t1").unwrap();
    assert_eq!((session.status, session.host_key_prompt.is_none()), (Status::Connecting, true));
}

#[test]
fn closing_a_live_tab_asks_first_and_releases_its_terminal() {
    let mut s = State::default();
    s.open_session("t1", "p1");
    s.opened("t1", Opened::Ok { pty_id: "pty-1".into(), claim: "c".into(), connected_at: None, secret_saved: false });
    assert!(s.request_close("t1").is_empty());
    assert_eq!(s.pending_close.as_deref(), Some("t1"));
    let effects = s.close("t1");
    assert_eq!(request(&effects[0]), ("ssh_session_close", json!({ "sessionId": "t1" })));
    assert_eq!(effects[1], Effect::Release { pty_id: "pty-1".into() });
    assert_eq!((s.active_tab.as_str(), s.pending_close.as_ref()), (HOSTS_TAB, None));
}

#[test]
fn a_late_answer_for_a_closed_tab_drops_its_shell() {
    let mut s = State::default();
    s.open_session("t1", "p1");
    s.close("t1");
    let effects = s.opened("t1", Opened::Ok { pty_id: "pty-9".into(), claim: "c".into(), connected_at: None, secret_saved: false });
    assert_eq!(request(&effects[0]).0, "ssh_session_close");
    assert_eq!(effects[1], Effect::Release { pty_id: "pty-9".into() });
}

#[test]
fn transfers_merge_by_session_path_and_direction_and_clear_when_finished() {
    let mut s = State::default();
    let t = |conn: &str, path: &str, status: &str, bytes: u64| Transfer {
        connection_id: conn.into(),
        remote_path: path.into(),
        local_path: "/l".into(),
        bytes_transferred: bytes,
        total_bytes: 10,
        direction: "upload".into(),
        status: status.into(),
        error: None,
    };
    s.update_transfer(t("a", "/x", "in-progress", 1));
    s.update_transfer(t("a", "/x", "completed", 10));
    s.update_transfer(t("b", "/y", "completed", 10));
    s.update_transfer(t("a", "/z", "in-progress", 3));
    assert_eq!(s.transfers.len(), 3);
    assert_eq!(s.transfers[0].bytes_transferred, 10);
    s.clear_completed_transfers(Some("a"));
    assert_eq!(s.transfers.iter().map(|t| t.remote_path.as_str()).collect::<Vec<_>>(), ["/y", "/z"]);
    s.clear_completed_transfers(None);
    assert_eq!(s.transfers.iter().map(|t| t.remote_path.as_str()).collect::<Vec<_>>(), ["/z"]);
}

#[test]
fn messages_name_the_host_and_the_fix() {
    let p = profile("p1", "key");
    let m = messages::describe_error("SSH_CONNECTION_REFUSED", &p);
    assert_eq!(m.title, "prod.example:2222 refused the connection");
    assert!(m.suggest_edit);
    assert_eq!(messages::host_label("::1", 2222), "[::1]:2222");
    assert_eq!(messages::address_label(&p), "deploy@prod.example:2222");
    let end = SessionEnd { reason: "exited".into(), exit_code: Some(3), detail: None };
    assert_eq!(messages::describe_end(&end, &p).body, "The remote shell exited with code 3.");
}

#[test]
fn a_save_carries_a_typed_secret_and_the_stored_profile_never_does() {
    let p = profile("p1", "password");
    let payload = save_payload(&p, Some("hunter2"), None);
    assert_eq!(payload["profile"]["password"], "hunter2");
    assert!(payload["profile"].get("passphrase").is_none());
    assert_eq!(payload["profile"]["portForwards"], json!([]));
    assert!(save_payload(&p, Some(""), None)["profile"].get("password").is_none());
}

#[test]
fn remote_paths_join_and_climb_like_the_browser() {
    assert_eq!(sftp::parent_path("/home/deploy/"), "/home");
    assert_eq!(sftp::parent_path("/home"), "/");
    assert_eq!(sftp::parent_path("/"), "/");
    assert_eq!(sftp::join_path("/", "a"), "/a");
    assert_eq!(sftp::join_path("/home", "a"), "/home/a");
    assert_eq!(sftp::local_file_name("C:\\Users\\me\\a.txt"), "a.txt");
    assert_eq!(sftp::local_file_name("/tmp/b.txt"), "b.txt");
}

#[test]
fn an_upload_over_an_existing_file_is_a_conflict_and_a_transfer_keeps_its_bytes() {
    let entry = |name: &str, kind: &str| sftp::Entry {
        name: name.into(),
        path: format!("/d/{name}"),
        entry_type: kind.into(),
        size: 1,
        permissions: 0o644,
        modified_at: String::new(),
        owner: None,
    };
    let listing = [entry("a.txt", "file"), entry("b", "directory")];
    let uploads = ["/tmp/a.txt".to_owned(), "/tmp/b".to_owned(), "/tmp/c".to_owned()];
    assert_eq!(sftp::conflicts(&uploads, &listing), ["a.txt"]);

    let mut s = State::default();
    s.transfer_started("c1", "/d/a.txt", "/tmp/a.txt", "upload");
    s.update_transfer(Transfer {
        connection_id: "c1".into(),
        remote_path: "/d/a.txt".into(),
        local_path: "/tmp/a.txt".into(),
        bytes_transferred: 6,
        total_bytes: 12,
        direction: "upload".into(),
        status: "in-progress".into(),
        error: None,
    });
    s.transfer_finished("c1", "/d/a.txt", "/tmp/a.txt", "upload", Ok(()));
    assert_eq!((s.transfers[0].bytes_transferred, s.transfers[0].status.as_str()), (12, "completed"));
}

#[test]
fn a_displayed_answer_cannot_approve_a_replaced_question() {
    let mut s = State::default();
    s.open_session("t1", "p1");
    s.host_key_asked(prompt("t1", "unknown"));
    let mut replacement = prompt("t1", "changed");
    replacement.fingerprint = "SHA256:replacement".into();
    s.host_key_asked(replacement);
    assert!(s.respond_host_key_shown("t1", "t1", "SHA256:new", true).is_empty());
    assert!(s.session("t1").unwrap().host_key_prompt.is_some());
    assert!(s.respond_host_key_shown("t1", "other", "SHA256:replacement", true).is_empty());
    let answer = s.respond_host_key_shown("t1", "t1", "SHA256:replacement", true);
    assert_eq!(request(&answer[0]).1["fingerprint"], "SHA256:replacement");
}
