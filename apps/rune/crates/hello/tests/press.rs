//! Drives the button through `rune::Runtime` and asserts the new label text.

use hello::{State, view};
use rune::{Env, Runtime};

fn label(rt: &Runtime<State>) -> Option<String> {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some("press-label"))
        .and_then(|e| e.text.clone())
}

#[test]
fn pressing_the_button_changes_its_label() {
    let env = Env {
        system_dark: true,
        scale: 2.0,
    };
    let mut rt = Runtime::new(State::default(), view, (360.0, 220.0), env);
    rt.frame(0.0);
    assert_eq!(label(&rt).as_deref(), Some("Press me"));

    let (x, y) = rt.find("press").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }

    assert_eq!(rt.state.presses, 1);
    assert_eq!(label(&rt).as_deref(), Some("Pressed 1"));
}
