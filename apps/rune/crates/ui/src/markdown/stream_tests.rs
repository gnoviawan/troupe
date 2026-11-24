//! A reply streamed one byte at a time ends on the same element tree as the whole reply
//! drawn at once: no segment, block or card is left behind by a shape it passed through.

use rune::{Cx, El, Env, Runtime};

use super::{Hooks, prose};

/// The scripted agent's first two answers (prose, a TypeScript fence, a list, a table, a
/// quote, a diagram, a link, a bash fence, a task list), a plan fence, a file path and an
/// image.
const ANSWER: &str = include_str!("fixtures/answer.md");

#[derive(Default)]
struct S {
    text: String,
    streaming: bool,
}

fn view(s: &S, _cx: &mut Cx<S>) -> El {
    let t = theme::tokens("termul", true);
    prose(t, "reply", &s.text, s.streaming, &Hooks::<S>::default())
}

/// Every live element: its path, key, role, text and rect to a hundredth of a point.
fn tree(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .map(|e| {
            let [x, y, w, h] = e.rect.map(|v| (v * 100.0).round() / 100.0);
            format!("{} {:?} {} {:?} {x} {y} {w} {h}", e.path, e.key, e.role, e.text)
        })
        .collect()
}

/// Eight seconds of frames: every growth and fade has ended.
fn settle(rt: &mut Runtime<S>) {
    for _ in 0..240 {
        rt.frame(1.0 / 30.0);
    }
}

#[test]
fn a_byte_at_a_time_stream_ends_on_the_one_shot_tree() {
    let env = || Env::new(true, 1.0);
    let mut whole = Runtime::new(S { text: ANSWER.to_owned(), streaming: false }, view, (760.0, 4000.0), env());
    settle(&mut whole);

    let mut streamed = Runtime::new(S { text: String::new(), streaming: true }, view, (760.0, 4000.0), env());
    for end in (1..=ANSWER.len()).filter(|&i| ANSWER.is_char_boundary(i)) {
        streamed.update(|s, _| s.text = ANSWER[..end].to_owned());
        streamed.frame(1.0 / 120.0);
    }
    streamed.update(|s, _| s.streaming = false);
    settle(&mut streamed);

    let (want, got) = (tree(&whole), tree(&streamed));
    assert!(want.len() > 50, "the fixture draws a real tree ({} elements)", want.len());
    let first_diff = want.iter().zip(&got).position(|(a, b)| a != b);
    assert_eq!(
        (want.len(), first_diff),
        (got.len(), None),
        "first difference: want {:?}, got {:?}",
        first_diff.map(|i| &want[i]),
        first_diff.map(|i| &got[i]),
    );
}

#[test]
fn the_plan_fence_and_its_text_never_draw() {
    let mut rt = Runtime::new(S { text: ANSWER.to_owned(), streaming: false }, view, (760.0, 4000.0), Env::new(true, 1.0));
    settle(&mut rt);
    let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(texts.iter().any(|t| t.contains("Plan saved.")));
    assert!(!texts.iter().any(|t| t.contains("entries")), "the plan snapshot is not prose");
    assert!(texts.iter().any(|t| t.contains("./src/lib/x_y.ts:12")), "the path keeps its text as a link");
}
