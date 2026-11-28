use rune::TextEdit;
use client::ws::{Command, Output};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsRequestType, WsWatchName};

use crate::State;
use crate::ssh::{DELETE_HOST, REMOTE_CONFIRM, RemoteConfirm};
use crate::tests::showing_alpha;

fn sent(s: &State, kind: WsRequestType) -> Vec<(u64, Value, bool)> {
    s.sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { call, kind: k, payload, ledgered } if *k == kind => Some((*call, payload.clone(), *ledgered)),
            _ => None,
        })
        .collect()
}

fn with_hosts() -> State {
    let mut s = showing_alpha();
    s.ssh.model.set_profiles(
        serde_json::from_value(json!([
            { "id": "p1", "name": "prod", "host": "prod.example", "port": 22, "username": "deploy", "authMethod": "key", "portForwards": [] },
            { "id": "p2", "name": "stage", "host": "stage.example", "port": 2222, "username": "deploy", "authMethod": "agent", "portForwards": [] }
        ]))
        .unwrap(),
    );
    s
}

fn delta(s: &mut State, delta: Value) {
    s.apply(Output::WatchDelta { watch: WsWatchName::Ssh, scope: String::new(), delta });
}

#[test]
fn a_changed_host_key_from_the_watch_is_trusted_only_by_its_fingerprint() {
    let mut s = with_hosts();
    s.ssh_connect("p1");
    let (_, open, ledgered) = sent(&s, WsRequestType::SshSessionOpen).pop().expect("open sent");
    assert!(ledgered, "an open carries a command id");
    let attempt = open["request"]["sessionId"].as_str().unwrap().to_owned();
    delta(
        &mut s,
        json!({ "event": "host_key", "sessionId": attempt, "status": "changed", "host": "prod.example", "port": 22,
                "keyType": "ssh-ed25519", "fingerprint": "SHA256:new", "previousFingerprints": ["SHA256:old"] }),
    );
    let tab = s.ssh.model.active_tab.clone();
    let prompt = s.ssh.model.session(&tab).unwrap().host_key_prompt.clone().expect("the question shows");
    assert_eq!(prompt.previous_fingerprints, ["SHA256:old"]);

    s.ssh_answer_host_key_shown(&tab, &attempt, &prompt.fingerprint, true);
    let (_, answer, ledgered) = sent(&s, WsRequestType::SshSessionRespondHostKey).pop().unwrap();
    assert_eq!(answer, json!({ "sessionId": attempt, "trust": true, "fingerprint": "SHA256:new" }));
    assert!(ledgered);
}

#[test]
fn the_delete_host_confirm_deletes_the_host_it_named_when_it_opened() {
    let mut s = with_hosts();
    s.ssh_request_delete("p1");
    assert!(s.overlays.is_open(DELETE_HOST));
    // The selection moves while the confirm is open; the confirm still names prod.
    s.ssh.model.select_profile(Some("p2"));
    s.ssh_confirm_delete();
    let deletes = sent(&s, WsRequestType::SshDeleteProfile);
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].1, json!({ "profileId": "p1" }));
    assert!(!s.overlays.is_open(DELETE_HOST));
    s.ssh_confirm_delete();
    assert_eq!(sent(&s, WsRequestType::SshDeleteProfile).len(), 1, "one confirm, one delete");
}

#[test]
fn a_remote_delete_acts_on_the_captured_entry_and_an_upload_over_a_file_asks_first() {
    let mut s = with_hosts();
    let entry = |name: &str| -> model::ssh::sftp::Entry {
        serde_json::from_value(json!({ "name": name, "path": format!("/home/{name}"), "entryType": "file", "size": 3,
                                      "permissions": 420, "modifiedAt": "", "owner": null }))
        .unwrap()
    };
    s.ssh.explorers.insert("c1".into(), crate::ssh::Explorer { cwd: "/home".into(), entries: vec![entry("a.txt"), entry("b.txt")], ..Default::default() });

    s.sftp_request_delete("c1", &entry("a.txt"));
    assert!(s.overlays.is_open(REMOTE_CONFIRM));
    s.sftp_confirm();
    let deletes = sent(&s, WsRequestType::SftpDelete);
    assert_eq!(deletes[0].1, json!({ "request": { "connectionId": "c1", "remotePath": "/home/a.txt" } }));
    assert!(deletes[0].2, "a delete is ledgered");

    s.sftp_upload("c1", vec!["/tmp/b.txt".into(), "/tmp/new.txt".into()]);
    assert!(matches!(&s.ssh.confirm, Some(RemoteConfirm::Replace { conflicts, .. }) if conflicts == &["b.txt"]));
    assert!(sent(&s, WsRequestType::SftpUpload).is_empty(), "nothing uploads before the confirm");
    s.sftp_confirm();
    let uploads: Vec<Value> = sent(&s, WsRequestType::SftpUpload).into_iter().map(|u| u.1["request"]["remotePath"].clone()).collect();
    assert_eq!(uploads, [json!("/home/b.txt"), json!("/home/new.txt")]);
    assert_eq!(s.ssh.model.transfers.len(), 2, "the tray shows both at once");
}

#[test]
fn a_typed_password_leaves_the_form_with_its_save() {
    let mut s = with_hosts();
    s.ssh_edit_host(Some("p1"));
    s.ssh.form.auth = "password".into();
    s.ssh.form.password = rune::TextEdit::new("hunter2");
    s.ssh_save_host(false);
    let (_, save, ledgered) = sent(&s, WsRequestType::SshSaveProfile).pop().unwrap();
    assert_eq!(save["profile"]["password"], "hunter2");
    assert!(!ledgered, "a save is idempotent and never ledgered with its secret");
    assert!(s.ssh.form.password.text.is_empty());
}

#[test]
fn closing_the_ssh_editor_clears_secrets_on_cancel_and_escape() {
    for escape in [false, true] {
        let mut s = with_hosts();
        s.ssh.form.password = TextEdit::new("secret");
        s.ssh.form.passphrase = TextEdit::new("passphrase");
        s.overlays.open(crate::ssh::EDITOR);
        if escape { s.close_top(); } else { s.close(crate::ssh::EDITOR); }
        assert!(s.ssh.form.password.text.is_empty());
        assert!(s.ssh.form.passphrase.text.is_empty());
    }
}

#[test]
fn an_upload_conflict_from_a_stale_listing_requires_explicit_replace() {
    let mut s = with_hosts();
    s.ssh.explorers.insert("c1".into(), crate::ssh::Explorer { cwd: "/home".into(), ..Default::default() });
    s.sftp_upload("c1", vec!["/tmp/new.txt".into()]);
    let (call, upload, _) = sent(&s, WsRequestType::SftpUpload).pop().unwrap();
    assert_eq!(upload["request"]["overwrite"], false);
    s.apply(Output::Reply { call, result: Err(client::ws::WsError {
        code: "SFTP_ALREADY_EXISTS".into(), message: "exists".into(), details: None,
    }) });
    assert!(s.overlays.is_open(REMOTE_CONFIRM));
    assert!(matches!(&s.ssh.confirm, Some(RemoteConfirm::Replace { conflicts, .. }) if conflicts == &["new.txt"]));
    s.sftp_confirm();
    let (_, replace, _) = sent(&s, WsRequestType::SftpUpload).pop().unwrap();
    assert_eq!(replace["request"]["overwrite"], true);
    assert_eq!(replace["request"]["remotePath"], "/home/new.txt");
}
