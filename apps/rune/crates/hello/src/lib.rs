//! The scaffold's smoke app: one window holding a text field and a button whose
//! label changes on press. `cargo run -p hello` opens the window;
//! `hello --snap hello.png` renders it headless, `hello --tree` prints the
//! element tree. The same `start` is the wasm entrypoint (`wasm_bindgen(start)`).
//! With the `tooling` feature, `RUNE_DRIVE_SOCK=<path>` serves the live driver.

#![forbid(unsafe_code)]

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, TextEdit, col, el, text, text_input};

pub struct State {
    pub draft: TextEdit,
    pub presses: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            draft: TextEdit::new(""),
            presses: 0,
        }
    }
}

pub fn view(state: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let field = text_input(&state.draft, type_::BODY)
        .key("field")
        .placeholder("Type here", t.text_secondary)
        .color(t.text)
        .w_full()
        .h(44.0)
        .padding(12.0)
        .radius(10.0)
        .fill(t.card)
        .on_edit(|s: &mut State| &mut s.draft);
    let label = match state.presses {
        0 => "Press me".to_string(),
        n => format!("Pressed {n}"),
    };
    let button = el()
        .key("press")
        .h(44.0)
        .px(20.0)
        .center()
        .radius(10.0)
        .fill(t.control_off)
        .child(text(label).key("press-label").font(type_::LABEL).color(t.text))
        .on_press(|s: &mut State| s.presses += 1);
    col()
        .w_full()
        .h_full()
        .center()
        .gap(12.0)
        .padding(16.0)
        .fill(t.bg)
        .child(field)
        .child(button)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn start() {
    let app = rune::App::new(AppSpec::new("Hello", (360.0, 220.0), State::default, view));
    #[cfg(feature = "tooling")]
    let app = automation::install(app);
    app.run();
}
