use client::terminal::{Command, Output};
use rune::{Key, KeyEvent, Mods};
use serde_json::{Value, json};
use troupe_protocol::terminal::{WebTerminalEventPayload, WebTerminalRequestType};

use crate::tests::present;

/// A runtime showing live terminal `pty-1`, focused.
fn shown() -> rune::Runtime<crate::State> {
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
    let mut rt = rune::Runtime::new(s, crate::view, (1280.0, 800.0), rune::Env::new(false, 1.0));
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
