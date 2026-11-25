use client::terminal::{Command, Output as TermOutput};
use client::ws::{Command as WsCommand, Output};
use serde_json::{Value, json};
use troupe_protocol::terminal::WebTerminalRequestType;
use troupe_protocol::ws::{WsError, WsRequestType};

use super::{CONFIRM, KEY, ago};
use crate::State;

/// Project `p-a` shown, its workspace and terminal layout read (calls 7 and 8).
fn showing() -> State {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    s
}

const DAY: f64 = 86_400_000.0;

#[test]
fn times_read_like_the_react_modal() {
    let now = 1_764_000_000_000.0;
    assert_eq!(ago(now - 30_000.0, now), "Just now");
    assert_eq!(ago(now - 5.0 * 60_000.0, now), "5m ago");
    assert_eq!(ago(now - 3.0 * 3_600_000.0, now), "3h ago");
    assert_eq!(ago(now - 2.0 * DAY, now), "2d ago");
    // 2025-11-24T16:00:00Z is 1764000000000.
    assert_eq!(ago(now, now + 8.0 * DAY), "11/24/2025");
}

/// The last store call the app sent, its key and its call id.
fn last_store(s: &State, kind: WsRequestType) -> (u64, Value) {
    s.sync
        .sent
        .iter()
        .rev()
        .find_map(|c| match c {
            WsCommand::Call { call, kind: k, payload, .. } if *k == kind => Some((*call, payload.clone())),
            _ => None,
        })
        .expect("a store call")
}

fn entry(command: &str, project: &str, at: f64) -> Value {
    json!({ "id": command, "command": command, "terminalName": "Terminal 1", "terminalId": "t-1", "projectId": project, "timestamp": at })
}

fn opened() -> State {
    let mut s = showing();
    s.open_command_history();
    let (call, payload) = last_store(&s, WsRequestType::StoreRead);
    assert!(payload["key"].as_str().unwrap().ends_with("/projects/p-a/command-history"), "{payload}");
    let list = json!([entry("npm test", "p-a", 3.0), entry("git status", "p-a", 2.0), entry("npm run build", "p-a", 1.0)]);
    s.apply(crate::tests::read_reply(call, json!({ "_version": 1, "data": list })));
    s
}

fn shown_commands(s: &State) -> Vec<String> {
    s.history_entries().iter().map(|e| e.command.clone()).collect()
}

#[test]
fn the_modal_lists_this_projects_commands_and_filters_them() {
    let mut s = opened();
    assert!(s.overlays.is_open(KEY));
    assert_eq!(shown_commands(&s), ["npm test", "git status", "npm run build"]);
    s.terms.history.query = rune::TextEdit::new("NPM");
    assert_eq!(shown_commands(&s), ["npm test", "npm run build"], "case-insensitive");
    s.terms.history.loaded.insert("p-b".into(), vec![super::Entry::parse(&entry("npm ci", "p-b", 2.5)).unwrap()]);
    s.terms.history.all_projects = true;
    assert_eq!(shown_commands(&s), ["npm test", "npm ci", "npm run build"], "every read project, newest first");
}

#[test]
fn return_inserts_the_highlighted_command_at_the_active_terminal_unsent() {
    let mut s = opened();
    let pane = s.workspace.tree().active_pane_id.clone();
    s.terms.sessions.spawn(41, "p-a", Some(pane));
    s.terminal_output(TermOutput::Reply {
        call: 41,
        result: Ok(json!({ "id": "pty-1", "shell": "bash", "cwd": "/a", "pid": 10, "cols": 80, "rows": 24, "claim": "c" })),
    });
    s.terms.views.insert("pty-1".into(), crate::terminal::TermView::new(std::sync::Arc::new(std::sync::Mutex::new(troupe_term::Term::new(20, 4)))));
    s.terms.sent.clear();
    s.history_key(Some(1));
    s.history_key(None);
    assert!(!s.overlays.is_open(KEY), "picking closes the modal");
    let writes: Vec<_> = s
        .terms
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: WebTerminalRequestType::Write, payload, .. } => Some(payload.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(writes, [json!({ "terminalId": "pty-1", "data": "git status" })]);
}

#[test]
fn clear_asks_first_then_empties_the_list_once_the_host_saved_it() {
    let mut s = opened();
    s.overlays.open(CONFIRM);
    s.clear_command_history();
    let (call, payload) = last_store(&s, WsRequestType::StoreWrite);
    assert_eq!(payload["value"]["data"], json!([]));
    assert!(s.terms.history.clearing);
    s.clear_command_history();
    assert_eq!(last_store(&s, WsRequestType::StoreWrite).0, call, "a second press while clearing sends nothing");
    s.apply(Output::Reply { call, result: Err(WsError { code: "STORE_FULL".into(), message: "disk full".into(), details: None }) });
    assert!(s.overlays.is_open(CONFIRM), "a failed clear keeps the confirm open");
    assert_eq!(shown_commands(&s).len(), 3, "and the list");
    s.clear_command_history();
    let (call, _) = last_store(&s, WsRequestType::StoreWrite);
    s.apply(Output::Reply { call, result: Ok(None) });
    assert!(!s.overlays.is_open(CONFIRM));
    assert!(shown_commands(&s).is_empty());
}
