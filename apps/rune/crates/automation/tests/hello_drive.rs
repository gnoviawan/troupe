//! Drives `hello` through the driver's command handler on a headless `Runtime`.
#![cfg(unix)]

use automation::{Command, respond};
use hello::{State, view};
use rune::{Env, Runtime};
use serde_json::Value;

fn send(rt: &mut Runtime<State>, line: &str) -> Value {
    let cmd: Command = serde_json::from_str(line).unwrap();
    respond(rt, &cmd)
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
}

fn hello() -> Runtime<State> {
    let env = Env {
        system_dark: true,
        scale: 2.0,
    };
    let mut rt = Runtime::new(State::default(), view, (360.0, 220.0), env);
    rt.frame(0.0);
    rt
}

#[test]
fn click_then_find_reads_the_new_label() {
    let mut rt = hello();
    assert_eq!(
        send(&mut rt, r#"{"cmd":"find","text":"Pressed 1"}"#)["error"],
        "not found"
    );

    assert_eq!(
        send(&mut rt, r#"{"cmd":"click","key":"press"}"#)["ok"],
        true
    );
    settle(&mut rt);

    let found = send(&mut rt, r#"{"cmd":"find","text":"Pressed 1"}"#);
    assert_eq!(found["ok"], true);
    assert_eq!(found["element"]["key"], "press-label");
    assert_eq!(found["element"]["text"], "Pressed 1");
}

#[test]
fn type_fills_the_focused_field() {
    let mut rt = hello();
    assert_eq!(
        send(&mut rt, r#"{"cmd":"click","key":"field"}"#)["ok"],
        true
    );
    settle(&mut rt);
    assert_eq!(
        send(&mut rt, r#"{"cmd":"type","text":"typed via socket"}"#)["ok"],
        true
    );
    settle(&mut rt);

    assert_eq!(rt.state.draft.text, "typed via socket");
    assert_eq!(
        send(&mut rt, r#"{"cmd":"find","text":"typed via socket"}"#)["element"]["key"],
        "field"
    );
}

#[test]
fn wait_and_snap_need_the_live_window() {
    let mut rt = hello();
    let reply = send(&mut rt, r#"{"cmd":"snap","path":"/tmp/never.png"}"#);
    assert_eq!(reply["error"], "wait and snap need the live window");
}
