use client::terminal::{Command, Output};
use model::workspace::Tab;
use serde_json::json;
use troupe_protocol::terminal::WebTerminalRequestType;

use super::{ASSIST_UNAVAILABLE, AssistKind, AssistStatus, MENU, suggested_commands};
use crate::State;

/// Project `p-a` shown, its workspace and terminal layout read (calls 7 and 8).
fn showing() -> State {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    s
}
use crate::term_select::tests::shown;
use crate::tests::present;

#[test]
fn suggested_commands_are_single_line_shell_fences_only() {
    let reply = "Try this:\n```sh\nnpm run build\n```\nor\n```bash\ncd app && ls\n```\n```\nmake\n```\n\
                 ```sh\nrm -rf a\nrm -rf b\n```\n```sh\necho \u{1b}[31m\n```";
    assert_eq!(suggested_commands(reply), ["npm run build", "cd app && ls", "make"]);
    assert!(suggested_commands("no fences here").is_empty());
}

/// Terminal tab `pty-1`'s session, live in pane `pane`.
fn live(s: &mut State, id: &str, call: u64) {
    let pane = s.workspace.tree().active_pane_id.clone();
    s.terms.sessions.spawn(call, "p-a", Some(pane));
    s.terminal_output(Output::Reply {
        call,
        result: Ok(json!({ "id": id, "shell": "bash", "cwd": "/a", "pid": 10, "cols": 80, "rows": 24, "claim": "c" })),
    });
}

fn terminal_tabs(s: &State) -> Vec<String> {
    model::workspace::leaves(s.workspace.root())
        .into_iter()
        .flat_map(|leaf| leaf.tabs.iter())
        .filter_map(|tab| match tab {
            Tab::Terminal { terminal_id, .. } => Some(terminal_id.clone()),
            _ => None,
        })
        .collect()
}

fn kinds(s: &State) -> Vec<(WebTerminalRequestType, String)> {
    s.terms
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind, payload, .. } => Some((*kind, payload.get("terminalId").and_then(|v| v.as_str()).unwrap_or("").to_owned())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_secondary_press_opens_the_menu_and_the_ai_rows_need_a_selection() {
    let mut rt = shown(b"Error: EADDRINUSE");
    let (cx, cy) = rt.find("terminal-pty-1").expect("the terminal screen");
    rt.secondary_down(cx, cy);
    rt.secondary_up(cx, cy);
    rt.frame(0.048);
    assert!(rt.state.overlays.is_open(MENU), "the context menu opened");
    assert_eq!(rt.state.terms.menu.as_ref().map(|m| m.0.as_str()), Some("pty-1"));
    rt.update_if(|s, _| {
        s.close(MENU);
        s.terminal_assist("pty-1", AssistKind::Explain);
        true
    });
    rt.frame(0.064);
    let status = rt.state.terms.assist.as_ref().map(|a| a.status.clone());
    assert_eq!(status, Some(AssistStatus::Error("Select some terminal output first".into())));
    assert!(present(&rt, "terminal-assist-pty-1"), "the card shows over the pane");
    rt.update_if(|s, _| {
        s.terminal_select_all("pty-1");
        s.terminal_assist("pty-1", AssistKind::Fix);
        true
    });
    let assist = rt.state.terms.assist.clone().expect("open");
    assert_eq!((assist.kind, assist.status), (AssistKind::Fix, AssistStatus::Error(ASSIST_UNAVAILABLE.into())));
    let escape = rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() };
    rt.update_if(move |s, tx| crate::keys::on_key(s, &escape, tx));
    assert!(rt.state.terms.assist.is_none(), "Escape closed the card");
}

#[test]
fn a_done_card_offers_its_commands_and_inserts_them_unsent() {
    let mut s = showing();
    live(&mut s, "pty-1", 41);
    s.terms.assist = Some(super::Assist {
        terminal: "pty-1".into(),
        kind: AssistKind::Fix,
        status: AssistStatus::Done("Port taken.\n```sh\nkill 4242\n```".into()),
    });
    s.terms.sent.clear();
    s.insert_assist_command("pty-1", "kill 4242");
    s.insert_assist_command("pty-1", "ls\u{202e}");
    s.insert_assist_command("pty-1", "ls\u{9b}");
    let writes: Vec<String> = s
        .terms
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: WebTerminalRequestType::Write, payload, .. } => payload["data"].as_str().map(str::to_owned),
            _ => None,
        })
        .collect();
    assert_eq!(writes, ["kill 4242"], "no Return, and bidi or C1 text is refused");
}

#[test]
fn restart_puts_a_new_shell_in_the_tab_and_kills_the_old_one() {
    let mut s = showing();
    live(&mut s, "pty-1", 41);
    s.reap_closed_tabs();
    s.terms.names.insert("pty-1".into(), "Terminal 1".into());
    s.terms.sent.clear();
    s.restart_terminal("pty-1");
    let spawn = s.terms.sent.iter().find_map(|c| match c {
        Command::Call { call, kind: WebTerminalRequestType::Spawn, .. } => Some(*call),
        _ => None,
    });
    let call = spawn.expect("a new shell was asked for");
    s.terminal_output(Output::Reply {
        call,
        result: Ok(json!({ "id": "pty-2", "shell": "bash", "cwd": "/a", "pid": 11, "cols": 80, "rows": 24, "claim": "d" })),
    });
    assert_eq!(terminal_tabs(&s), ["pty-2"], "the tab moved to the new PTY");
    assert_eq!(s.terms.names.get("pty-2").map(String::as_str), Some("Terminal 1"), "and kept its name");
    s.reap_closed_tabs();
    assert!(kinds(&s).contains(&(WebTerminalRequestType::Kill, "pty-1".into())), "{:?}", kinds(&s));
}
