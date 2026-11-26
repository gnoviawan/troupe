use client::terminal::{Command, Output};
use rune::{Key, KeyEvent, Mods};
use serde_json::{Value, json};
use troupe_protocol::terminal::{WebTerminalEventPayload, WebTerminalRequestType};

use crate::tests::present;

/// A desktop runtime showing live terminal `pty-1`, focused.
fn shown() -> rune::Runtime<crate::State> {
    sized((1280.0, 800.0), false)
}

fn sized(size: (f32, f32), touch: bool) -> rune::Runtime<crate::State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    let pane = s.workspace.tree().active_pane_id.clone();
    s.terms.sessions.spawn(41, "p-a", Some(pane));
    s.terminal_output(Output::Reply {
        call: 41,
        result: Ok(json!({ "id": "pty-1", "shell": "bash", "cwd": "/a", "pid": 10, "cols": 60, "rows": 20, "claim": "c" })),
    });
    s.terms.views.insert(
        "pty-1".into(),
        crate::terminal::TermView::new(std::sync::Arc::new(std::sync::Mutex::new(troupe_term::Term::new(60, 20)))),
    );
    s.terms.focused = Some("pty-1".into());
    let mut rt = rune::Runtime::new(s, crate::view, size, rune::Env::new(touch, 1.0));
    step(&mut rt);
    rt
}

fn lifecycle(kind: &str, generation: Option<u64>) -> Output {
    Output::Event(WebTerminalEventPayload::SessionLifecycle {
        terminal_id: "pty-1".into(),
        session_id: "s1".into(),
        kind: kind.into(),
        nonce: Some("n1".into()),
        prompt_suppressed: true,
        line_empty: true,
        exit_code: None,
        prompt_generation: generation,
        cwd: Some("/a".into()),
    })
}

/// The shell reached a trusted prompt: the editor mounts.
fn at_prompt(rt: &mut rune::Runtime<crate::State>, generation: u64) {
    let at = rt.state.now + 0.05;
    if generation == 1 {
        rt.state.terminal_output(lifecycle("InitShell", None));
    }
    rt.state.terminal_output(lifecycle("PromptReady", Some(generation)));
    rt.invalidate();
    rt.frame(at);
    if present(rt, "command-editor-pty-1") {
        focus(rt, at);
    }
}

/// A press on the editor gives it the keys.
fn focus(rt: &mut rune::Runtime<crate::State>, at: f64) {
    let (x, y) = rt.find("command-editor-pty-1").expect("the editor");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for i in 1..=3 {
        rt.frame(at + 0.02 * f64::from(i));
    }
}

/// Keys are queued and the next frame hands them to the focused element. The host's frames
/// are applied straight to the state here, so the frame also rebuilds the view.
fn step(rt: &mut rune::Runtime<crate::State>) {
    rt.invalidate();
    let at = rt.state.now + 0.01;
    rt.frame(at);
}

fn typed(rt: &mut rune::Runtime<crate::State>, text: &str) {
    for ch in text.chars() {
        rt.key(KeyEvent { key: Key::Text(ch.to_string()), mods: Mods::default() });
    }
    step(rt);
}

fn press(rt: &mut rune::Runtime<crate::State>, key: Key, shift: bool) {
    rt.key(KeyEvent { key, mods: Mods { shift, ..Mods::default() } });
    step(rt);
}

/// The session calls sent so far, as `(type, payload)`.
fn calls(rt: &rune::Runtime<crate::State>) -> Vec<(WebTerminalRequestType, Value, u64)> {
    rt.state
        .terms
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { call, kind, payload, ledgered } => {
                let session = matches!(
                    kind,
                    WebTerminalRequestType::CommandEditorSubmit
                        | WebTerminalRequestType::CommandEditorRequestCompletion
                        | WebTerminalRequestType::CommandEditorCancelCompletion
                        | WebTerminalRequestType::Write
                );
                assert!(!ledgered || *kind == WebTerminalRequestType::Write, "session calls are not ledgered");
                session.then(|| (*kind, payload.clone(), *call))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_trusted_prompt_mounts_the_editor_and_enter_sends_a_multi_line_draft_as_one_submit() {
    let mut rt = shown();
    assert!(!present(&rt, "command-editor-pty-1"), "no editor before the shell reports a prompt");
    at_prompt(&mut rt, 1);
    assert!(present(&rt, "command-editor-pty-1"), "a suppressed prompt on an empty line mounts it");
    step(&mut rt);
    typed(&mut rt, "echo \"a");
    press(&mut rt, Key::Enter, true);
    typed(&mut rt, "b\"");
    assert!(calls(&rt).iter().all(|(k, ..)| *k != WebTerminalRequestType::Write), "no key reached the PTY");
    press(&mut rt, Key::Enter, false);
    step(&mut rt);
    let sent = calls(&rt);
    let (kind, payload, call) = sent.last().cloned().expect("a submit");
    assert_eq!(kind, WebTerminalRequestType::CommandEditorSubmit);
    assert_eq!(payload, json!({ "terminalId": "pty-1", "draft": "echo \"a\nb\"" }));
    assert!(!present(&rt, "command-editor-pty-1"), "the editor gives way while the command runs");
    assert_eq!(rt.state.terms.editor.sessions.draft("pty-1"), "echo \"a\nb\"", "the draft stays until the host takes it");

    rt.state.terminal_output(Output::Reply { call, result: Ok(json!("Accepted")) });
    rt.state.terminal_output(lifecycle("CommandStarted", None));
    rt.state.terminal_output(lifecycle("CommandFinished", None));
    at_prompt(&mut rt, 2);
    assert!(present(&rt, "command-editor-pty-1"), "the next prompt mounts it again");
    assert_eq!(rt.state.terms.editor.sessions.draft("pty-1"), "", "an accepted submit drops the draft");
    assert_eq!(rt.state.terms.editor.history.recall("p-a"), vec!["echo \"a\nb\"".to_owned()], "and joins the history");

    press(&mut rt, Key::Up, false);
    step(&mut rt);
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "echo \"a\nb\"", "Arrow Up recalls it");
}

#[test]
fn a_refused_submit_brings_the_draft_back_and_keys_never_reach_the_pty_meanwhile() {
    let mut rt = shown();
    at_prompt(&mut rt, 1);
    typed(&mut rt, "ls");
    press(&mut rt, Key::Enter, false);
    let (_, _, call) = calls(&rt).last().cloned().expect("a submit");
    rt.state.terminal_write("pty-1", "x");
    assert!(calls(&rt).iter().all(|(k, ..)| *k != WebTerminalRequestType::Write), "typing is held while the submit is on the wire");
    rt.state.terminal_write("pty-1", "\u{3}");
    assert!(calls(&rt).iter().any(|(k, p, _)| *k == WebTerminalRequestType::Write && p["data"] == "\u{3}"), "Ctrl+C still interrupts");
    rt.state.terminal_output(Output::Reply { call, result: Ok(json!("PreWrite")) });
    step(&mut rt);
    let at = rt.state.now + 0.01;
    focus(&mut rt, at);
    assert!(present(&rt, "command-editor-pty-1"), "nothing was written: the editor comes back");
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "ls", "with the draft");
}

#[test]
fn tab_asks_for_completions_and_accepting_one_spells_it_for_the_shell() {
    let mut rt = shown();
    at_prompt(&mut rt, 1);
    typed(&mut rt, "cat di");
    press(&mut rt, Key::Tab, false);
    let (kind, payload, call) = calls(&rt).into_iter().rfind(|(k, ..)| *k == WebTerminalRequestType::CommandEditorRequestCompletion).expect("a request");
    assert_eq!(kind, WebTerminalRequestType::CommandEditorRequestCompletion);
    assert_eq!((payload["draft"].as_str(), payload["cursor"].as_u64(), payload["cwd"].as_str()), (Some("cat di"), Some(6), Some("/a")));
    let output = "dir with space/".bytes().map(|b| format!("{b:02x}")).collect::<String>();
    let completion = json!({
        "result": "Accepted",
        "outcome": "Completed",
        "completion": {
            "request_id": payload["requestId"], "buffer_revision": payload["bufferRevision"], "cursor": 6,
            "prompt_generation": 1, "cwd": "/a", "output": output,
        }
    });
    rt.state.terminal_output(Output::Reply { call, result: Ok(completion) });
    step(&mut rt);
    assert!(present(&rt, "command-completions-pty-1"), "the list opens over the caret");
    assert_eq!(rt.state.ghost("pty-1").as_deref(), Some("r\\ with\\ space/"), "the lit row's rest shows as a ghost");
    press(&mut rt, Key::Tab, false);
    step(&mut rt);
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "cat dir\\ with\\ space/");
    assert!(!present(&rt, "command-completions-pty-1"));
}

#[test]
fn the_alternate_screen_takes_the_editor_away_and_gives_it_back() {
    let mut rt = shown();
    at_prompt(&mut rt, 1);
    typed(&mut rt, "vim");
    if let Ok(mut term) = rt.state.terms.views["pty-1"].cells.lock() {
        term.advance(b"\x1b[?1049h");
    }
    rt.state.command_editor_screen("pty-1");
    step(&mut rt);
    assert!(!present(&rt, "command-editor-pty-1"), "a full-screen program gets the keys");
    if let Ok(mut term) = rt.state.terms.views["pty-1"].cells.lock() {
        term.advance(b"\x1b[?1049l");
    }
    rt.state.command_editor_screen("pty-1");
    step(&mut rt);
    assert!(present(&rt, "command-editor-pty-1"), "leaving it brings the prompt's editor back");
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "vim", "with the draft");
}

/// The editor's height to a tenth of a point (layout keeps 1/64 pt).
fn field_height(rt: &rune::Runtime<crate::State>) -> f32 {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("command-editor-pty-1")).map(|e| (e.rect[3] * 10.0).round() / 10.0).expect("the editor")
}

fn key_bar(rt: &mut rune::Runtime<crate::State>, label: &str) {
    let (x, y) = rt.find(&format!("terminal-key-{label}")).expect("the key");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    step(rt);
}

#[test]
fn on_the_phone_the_editor_keeps_the_touch_font_and_the_key_bar_drives_it() {
    let mut desktop = shown();
    at_prompt(&mut desktop, 1);
    let mut rt = sized((390.0, 844.0), true);
    at_prompt(&mut rt, 1);
    assert!(present(&rt, "terminal-key-bar-pty-1"), "the key bar stays under the editor");
    assert_eq!((field_height(&desktop), field_height(&rt)), (40.8, 43.2), "one 16.8 px line on the desktop, one 19.2 px line at the 16 px floor");

    typed(&mut rt, "ls");
    key_bar(&mut rt, "Tab");
    let (kind, payload, _) = calls(&rt).last().cloned().expect("a request");
    assert_eq!((kind, payload["draft"].as_str()), (WebTerminalRequestType::CommandEditorRequestCompletion, Some("ls")), "Tab asks the host");
    key_bar(&mut rt, "←");
    typed(&mut rt, "x");
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "lxs", "the arrows move the caret");
    key_bar(&mut rt, "Ctrl+C");
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "", "Ctrl+C clears the draft");
    assert!(calls(&rt).iter().all(|(k, ..)| *k != WebTerminalRequestType::Write), "no key bar key reached the PTY");
}

/// The app's last store call of `kind`: its call id and payload.
fn store_call(rt: &rune::Runtime<crate::State>, kind: troupe_protocol::ws::WsRequestType) -> Option<(u64, Value)> {
    rt.state.sync.sent.iter().rev().find_map(|c| match c {
        client::ws::Command::Call { call, kind: k, payload, .. } if *k == kind => Some((*call, payload.clone())),
        _ => None,
    })
}

#[test]
fn a_command_accepted_before_the_history_is_read_joins_the_stored_list_instead_of_replacing_it() {
    use troupe_protocol::ws::{WsError, WsRequestType};
    let mut rt = shown();
    at_prompt(&mut rt, 1);
    let (read, payload) = store_call(&rt, WsRequestType::StoreRead).expect("the mount reads the history");
    assert!(payload["key"].as_str().unwrap().ends_with("/projects/p-a/command-history"), "{payload}");
    rt.state.apply(client::ws::Output::Reply { call: read, result: Err(WsError { code: "TIMEOUT".into(), message: "slow".into(), details: None }) });

    typed(&mut rt, "ls");
    press(&mut rt, Key::Enter, false);
    let (_, _, submit) = calls(&rt).last().cloned().expect("a submit");
    rt.state.terminal_output(Output::Reply { call: submit, result: Ok(json!("Accepted")) });
    assert!(store_call(&rt, WsRequestType::StoreWrite).is_none(), "no write while the stored list is unknown");

    rt.state.terminal_output(lifecycle("CommandStarted", None));
    rt.state.terminal_output(lifecycle("CommandFinished", None));
    at_prompt(&mut rt, 2);
    let (again, _) = store_call(&rt, WsRequestType::StoreRead).expect("a read");
    assert_ne!(again, read, "the failed read is asked again on the next mount");
    let stored = json!([{ "id": "1", "command": "npm test", "terminalName": "Terminal 1", "terminalId": "t-0", "projectId": "p-a", "timestamp": 1 }]);
    rt.state.apply(crate::tests::read_reply(again, json!({ "_version": 1, "data": stored })));
    assert_eq!(rt.state.terms.editor.history.recall("p-a"), ["npm test", "ls"], "the stored list, then the new command");
    let (_, write) = store_call(&rt, WsRequestType::StoreWrite).expect("the merged list is written");
    let written: Vec<&str> = write["value"]["data"].as_array().unwrap().iter().map(|e| e["command"].as_str().unwrap()).collect();
    assert_eq!(written, ["ls", "npm test"]);
}

#[test]
fn an_input_method_composes_at_the_caret_and_commits_into_the_draft() {
    let mut rt = shown();
    at_prompt(&mut rt, 1);
    typed(&mut rt, "echo ");
    assert!(rt.state.command_editor_ime("pty-1", &rune::Ime::Preedit("ni".into(), None, Vec::new())));
    step(&mut rt);
    assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some("ni")), "the composition shows at the caret");
    assert!(rt.state.command_editor_ime("pty-1", &rune::Ime::Commit("你".into())));
    step(&mut rt);
    assert_eq!(rt.state.terms.editor.lines["pty-1"].text(), "echo 你");
    assert!(!rt.elements().iter().any(|e| !e.exiting && e.text.as_deref() == Some("ni")), "and leaves");
    assert_eq!(crate::command_editor::field_terminal(&crate::command_editor::field_key("pty-1")), Some("pty-1"));
}
