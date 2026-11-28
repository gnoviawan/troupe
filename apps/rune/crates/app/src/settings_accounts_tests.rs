use client::ws::{Command, Output};
use rune::{Runtime, TextEdit};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use super::PAGE;
use crate::State;
use crate::tests::{present, runtime_at, tap};

fn edit(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update_if(move |s, _| {
        f(s);
        true
    });
    rt.frame(0.016);
    rt.frame(0.032);
}

fn calls(rt: &Runtime<State>, want: WsRequestType) -> Vec<(u64, Value)> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { call, kind, payload, .. } if *kind == want => Some((*call, payload.clone())),
            _ => None,
        })
        .collect()
}

fn last(rt: &Runtime<State>, want: WsRequestType) -> (u64, Value) {
    calls(rt, want).pop().unwrap_or_else(|| panic!("no {want:?} call"))
}

fn reply(rt: &mut Runtime<State>, call: u64, result: Result<Value, WsError>) {
    edit(rt, move |s| s.apply(Output::Reply { call, result: result.map(Some) }));
}

fn refused(message: &str) -> WsError {
    WsError { code: "INTERNAL".into(), message: message.into(), details: None }
}

fn claude() -> Value {
    json!({ "accounts": [
        { "id": "default", "name": "Default", "builtin": true, "signedIn": true },
        { "id": "work", "name": "Work", "builtin": false, "signedIn": true }
    ], "defaultAccountId": "default", "projectAccounts": { "p-a": "work" }, "chatAccounts": {}, "autoSwitch": false })
}

fn usage() -> Value {
    json!({ "usage": [
        { "accountId": "default", "status": "ok", "subscriptionType": "max", "checkedAt": 1.0, "message": null, "windows": [
            { "kind": "session", "group": "session", "scope": null, "percent": 30.0, "resetsAt": null, "severity": null },
            { "kind": "weekly_all", "group": "weekly", "scope": null, "percent": 12.0, "resetsAt": null, "severity": null }
        ] },
        { "accountId": "work", "status": "signed_out", "subscriptionType": null, "checkedAt": 1.0, "message": null, "windows": [] }
    ] })
}

/// The Accounts page with Claude's list and usage answered and the other two groups empty.
fn opened() -> Runtime<State> {
    let mut rt = runtime_at((1280.0, 800.0));
    edit(&mut rt, |s| s.open_settings(Some(PAGE)));
    let (list, _) = last(&rt, WsRequestType::ListClaudeAccounts);
    reply(&mut rt, list, Ok(claude()));
    let (read, payload) = last(&rt, WsRequestType::ListClaudeUsage);
    assert_eq!(payload, json!({ "force": false }));
    reply(&mut rt, read, Ok(usage()));
    for kind in [WsRequestType::ListCodexAccounts, WsRequestType::ListAntigravityAccounts] {
        let (call, _) = last(&rt, kind);
        reply(&mut rt, call, Ok(json!({ "accounts": [{ "id": "default", "name": "Default", "builtin": true, "signedIn": null }], "defaultAccountId": "default", "projectAccounts": {}, "chatAccounts": {}, "autoSwitch": false })));
    }
    rt
}

#[test]
fn every_provider_shows_its_accounts_usage_and_switching() {
    let rt = opened();
    for key in ["claude-accounts", "codex-accounts", "antigravity-accounts", "claude-account-default", "claude-account-work", "main-meter", "claude-auto-switch", "codex-second-account-hint", "per-project-note"] {
        assert!(present(&rt, key), "the page shows {key}");
    }
    assert!(!present(&rt, "default-pill"), "the built-in Default does not say Default twice");
    let (tone, meta) = model::settings::accounts::claude_status(&claude()["accounts"][1], Some(&usage()["usage"][1]), false, 1);
    assert_eq!((tone, meta.as_str()), (model::settings::accounts::AccountTone::SignedOut, "signed out"));
}

#[test]
fn automatic_switching_asks_first_and_only_then_saves() {
    let mut rt = opened();
    tap(&mut rt, "auto-switch");
    assert_eq!(rt.state.accounts_ui.risk, Some(model::settings::accounts::Provider::Claude));
    assert!(present(&rt, "risk-confirm"));
    assert!(calls(&rt, WsRequestType::SetClaudeAutoSwitch).is_empty(), "nothing is saved before the warning is accepted");
    tap(&mut rt, "risk-cancel");
    assert!(calls(&rt, WsRequestType::SetClaudeAutoSwitch).is_empty());
    tap(&mut rt, "auto-switch");
    tap(&mut rt, "risk-confirm");
    let (call, payload) = last(&rt, WsRequestType::SetClaudeAutoSwitch);
    assert_eq!(payload, json!({ "enabled": true }));
    reply(&mut rt, call, Ok(json!({})));
    assert_eq!(calls(&rt, WsRequestType::ListClaudeAccounts).len(), 2, "the list is read again after the change");
}

#[test]
fn remove_acts_on_the_picked_account_only_after_the_confirm() {
    let mut rt = opened();
    let target = super::Target { provider: model::settings::accounts::Provider::Claude, id: "work".into(), name: "Work".into() };
    edit(&mut rt, |s| s.accounts_open_remove(target));
    assert!(present(&rt, "moving"), "the confirm lists the projects that move");
    assert!(calls(&rt, WsRequestType::RemoveClaudeAccount).is_empty());
    tap(&mut rt, "remove-confirm");
    tap(&mut rt, "remove-confirm");
    let removes = calls(&rt, WsRequestType::RemoveClaudeAccount);
    assert_eq!(removes.len(), 1, "a second press while busy sends nothing");
    assert_eq!(removes[0].1, json!({ "accountId": "work" }));
    reply(&mut rt, removes[0].0, Ok(json!({ "movedProjectIds": ["p-a"], "movedChatIds": [] })));
    assert!(rt.state.accounts_ui.removing.is_none());
    assert!(rt.state.toasts.visible().iter().any(|t| t.title == "Work was removed"));
}

#[test]
fn a_refused_remove_keeps_the_confirm_and_says_why() {
    let mut rt = opened();
    let target = super::Target { provider: model::settings::accounts::Provider::Codex, id: "default".into(), name: "Default".into() };
    edit(&mut rt, |s| s.accounts_open_remove(target));
    tap(&mut rt, "remove-confirm");
    let (call, _) = last(&rt, WsRequestType::RemoveCodexAccount);
    reply(&mut rt, call, Err(refused("busy")));
    assert!(rt.state.accounts_ui.removing.is_some());
    assert!(rt.state.toasts.visible().iter().any(|t| t.title == "Could not remove Default: busy"));
}

#[test]
fn add_names_the_account_then_signs_in_with_the_link_and_the_code() {
    let mut rt = opened();
    tap(&mut rt, "add-account");
    assert!(present(&rt, "account-name"));
    tap(&mut rt, "continue");
    assert_eq!(rt.state.accounts_ui.add.as_ref().unwrap().error.as_deref(), Some("Enter a name."));
    edit(&mut rt, |s| s.accounts_ui.add.as_mut().unwrap().name = TextEdit::new("Home"));
    tap(&mut rt, "continue");
    let (call, payload) = last(&rt, WsRequestType::AddClaudeAccount);
    assert_eq!(payload["name"], json!("Home"));
    reply(&mut rt, call, Ok(json!({ "id": "home", "name": "Home", "createdAt": 1 })));
    let (start, payload) = last(&rt, WsRequestType::StartClaudeLinkLogin);
    assert_eq!(payload, json!({ "accountId": "home" }));
    reply(&mut rt, start, Ok(json!({ "url": "https://claude.ai/oauth/authorize?x=1" })));
    assert!(present(&rt, "open-link") && present(&rt, "code"));
    edit(&mut rt, |s| s.accounts_ui.add.as_mut().unwrap().code = TextEdit::new(" abc\n123 "));
    tap(&mut rt, "finish");
    let (finish, payload) = last(&rt, WsRequestType::FinishClaudeLinkLogin);
    assert_eq!(payload["code"], json!("abc123"), "the pasted code loses its spaces");
    reply(&mut rt, finish, Ok(json!({})));
    assert_eq!(rt.state.accounts_ui.add.as_ref().unwrap().step, crate::settings_accounts_dialogs::Step::Done);
    assert_eq!(rt.state.accounts_ui.add.as_ref().unwrap().code.text, "", "the code is not kept");
}

#[test]
fn closing_a_waiting_sign_in_stops_it_on_the_host() {
    let mut rt = opened();
    let target = super::Target { provider: model::settings::accounts::Provider::Claude, id: "work".into(), name: "Work".into() };
    edit(&mut rt, |s| s.accounts_sign_in(target));
    assert_eq!(last(&rt, WsRequestType::StartClaudeLinkLogin).1, json!({ "accountId": "work" }));
    tap(&mut rt, "cancel");
    assert_eq!(last(&rt, WsRequestType::CancelClaudeLogin).1, json!({ "accountId": "work" }));
    assert!(rt.state.accounts_ui.add.is_none());
}

#[test]
fn rename_saves_the_trimmed_name() {
    let mut rt = opened();
    let target = super::Target { provider: model::settings::accounts::Provider::Antigravity, id: "default".into(), name: "Default".into() };
    edit(&mut rt, |s| s.accounts_open_rename(target));
    edit(&mut rt, |s| s.accounts_ui.rename = TextEdit::new("  Personal "));
    tap(&mut rt, "rename-save");
    let (call, payload) = last(&rt, WsRequestType::RenameAntigravityAccount);
    assert_eq!(payload, json!({ "accountId": "default", "name": "Personal" }));
    reply(&mut rt, call, Ok(json!({})));
    assert!(rt.state.accounts_ui.renaming.is_none());
}
