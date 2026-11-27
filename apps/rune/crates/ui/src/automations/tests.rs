use model::automations::format::State;
use rune::{Cx, El, Env, Runtime};

use super::*;

struct S;

fn view(_: &S, _: &mut Cx<S>) -> El {
    let t = theme::tokens("termul", true);
    let face = Face {
        icon: kind_icon(NodeKind::Agent),
        title: "Extract".into(),
        subtitle: "Agent · claude".into(),
        problem: false,
        issues: 2,
        status: Some(StepView::Failed),
        selected: true,
        loop_: Some(LoopCount {
            iterations: 3,
            failed: 1,
        }),
        slots: vec![("Model", "Default".into()), ("Tools", "None".into())],
        dimmed: false,
    };
    col()
        .child(state_pill(t, State::NeedsApproval).key("pill"))
        .child(card(t, face).key("card"))
}

#[test]
fn a_card_and_a_pill_draw() {
    let mut rt = Runtime::new(S, view, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert!(rt.find("pill").is_some());
    let (x, y) = rt.find("card").expect("card");
    assert_eq!((x, y), (CARD_W / 2.0, 20.0 + CARD_H / 2.0));
}
