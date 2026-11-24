//! The transcript's "Scroll to latest" button: it appears once the reader scrolls up from the
//! end and takes them back to it.

use model::acp;
use rune::{Cx, El, Env, Runtime};
use serde_json::json;

use super::{Lenses, Reader, transcript};
use crate::markdown::Hooks;

struct NoIds;

impl model::Ids for NoIds {
    fn next_id(&mut self) -> String {
        String::new()
    }
}

#[derive(Default)]
struct S {
    acp: acp::State,
    reader: Reader,
}

fn view(s: &S, _cx: &mut Cx<S>) -> El {
    let lenses = Lenses { acp: |s: &S| &s.acp, reader: |s: &S| &s.reader, reader_mut: |s: &mut S| &mut s.reader, step: |s: &mut S, e| {
        acp::step(&mut s.acp, e, &mut acp::Env { now: 0.0, ids: &mut NoIds, server_history: true });
    } };
    transcript(theme::tokens("termul", true), s, "c1", lenses, Hooks::default())
}

fn chat(turns: usize) -> Runtime<S> {
    let mut s = S::default();
    let messages = (0..turns * 2)
        .map(|n| {
            let role = if n % 2 == 0 { "user" } else { "agent" };
            serde_json::from_value(json!({
                "id": format!("m{n}"), "role": role, "streaming": false, "timestamp": n as f64 * 1000.0, "seq": n as f64,
                "blocks": [{ "type": "text", "text": format!("Message {n}, a line of prose long enough to read.") }],
            }))
            .unwrap()
        })
        .collect();
    s.acp.messages.insert("c1".into(), messages);
    let mut rt = Runtime::new(s, view, (800.0, 600.0), Env::new(true, 1.0));
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<S>) {
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
}

fn latest_button(rt: &Runtime<S>) -> Option<(f32, f32)> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.label.as_deref() == Some("Scroll to latest"))
        .map(|e| (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0))
}

#[test]
fn scrolling_up_shows_scroll_to_latest_and_pressing_it_returns_to_the_end() {
    let mut rt = chat(30);
    assert_eq!(rt.scroll_at_bottom("chat-scroll-c1"), Some(true), "a chat opens at its end");
    assert_eq!(latest_button(&rt), None, "no button at the end");

    for _ in 0..5 {
        rt.wheel(400.0, 300.0, 300.0);
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(rt.scroll_at_bottom("chat-scroll-c1"), Some(false));
    let (x, y) = latest_button(&rt).expect("the button shows once scrolled up");

    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.scroll_at_bottom("chat-scroll-c1"), Some(true), "the press scrolls to the end");
    assert_eq!(latest_button(&rt), None, "and the button goes");
}
