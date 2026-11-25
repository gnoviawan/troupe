use client::terminal::{Command, Output};
use serde_json::json;
use troupe_protocol::terminal::WebTerminalRequestType;

use super::KEYS;
use crate::tests::present;

/// A phone-sized runtime showing live terminal `pty-1`.
fn phone() -> rune::Runtime<crate::State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    let pane = s.workspace.tree().active_pane_id.clone();
    s.terms.sessions.spawn(41, "p-a", Some(pane));
    s.terminal_output(Output::Reply {
        call: 41,
        result: Ok(json!({ "id": "pty-1", "shell": "bash", "cwd": "/a", "pid": 10, "cols": 40, "rows": 20, "claim": "c" })),
    });
    s.terms.views.insert(
        "pty-1".into(),
        crate::terminal::TermView::new(std::sync::Arc::new(std::sync::Mutex::new(troupe_term::Term::new(40, 20)))),
    );
    let mut rt = rune::Runtime::new(s, crate::view, (390.0, 844.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

fn writes(rt: &rune::Runtime<crate::State>) -> Vec<String> {
    rt.state
        .terms
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: WebTerminalRequestType::Write, payload, .. } => payload["data"].as_str().map(str::to_owned),
            _ => None,
        })
        .collect()
}

#[test]
fn the_phone_shows_the_key_bar_under_a_terminal_and_its_keys_write_their_bytes() {
    let mut rt = phone();
    assert!(present(&rt, "terminal-pty-1"), "the phone main area shows the terminal");
    assert!(present(&rt, "terminal-key-bar-pty-1"), "with the key bar under it");
    for (label, bytes, _) in [KEYS[2], KEYS[4]] {
        let (x, y) = rt.find(&format!("terminal-key-{label}")).expect("the key");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.05);
        assert_eq!(writes(&rt).last().map(String::as_str), Some(bytes), "{label}");
    }
    let (x, y) = rt.find("terminal-keys-toggle").expect("the toggle");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.1);
    assert!(!present(&rt, "terminal-key-Esc"), "the toggle hides the keys");
    assert!(present(&rt, "terminal-keys-paste"), "and keeps Paste");
}
