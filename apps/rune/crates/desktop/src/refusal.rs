//! The window a launch that cannot start shows instead of the app: why, and a Quit button.
//! The Tauri app shows a native error dialog for the same refusals (`refuse_startup`).

use std::sync::OnceLock;

use rune::{AppSpec, Cx, El, Tx, col, row, text};
use ui::base::button::{Button, ButtonVariant};
use ui::type_::sans;

static WHY: OnceLock<(String, String)> = OnceLock::new();

#[derive(Default)]
struct Refusal;

fn view(_: &Refusal, _: &mut Cx<Refusal>) -> El {
    let t = theme::tokens(theme::DEFAULT_FAMILY, true);
    let c = &t.colors;
    let (title, message) = WHY.get().cloned().unwrap_or_default();
    col()
        .key("refusal")
        .w_full()
        .h_full()
        .fill(c.background)
        .padding(24.0)
        .gap(12.0)
        .child(text(title).font(sans(16.0, 600.0, 22.0)).color(c.foreground))
        .child(text(message).font(sans(13.0, 400.0, 19.0)).color(c.muted_foreground).wrap().w_full())
        .child(el_spacer())
        .child(
            row().w_full().justify_end().child(
                Button::new("Quit")
                    .variant(ButtonVariant::Default)
                    .build(t)
                    .key("refusal-quit")
                    .on_press_tx(|_: &mut Refusal, tx: &mut Tx| tx.quit()),
            ),
        )
}

fn el_spacer() -> El {
    rune::el().grow(1.0)
}

/// Shows `message` under `title` until the user quits.
pub fn show(title: &str, message: &str) {
    log::error!("[startup] refusing to start: {message}");
    eprintln!("{title}: {message}");
    let _ = WHY.set((title.to_owned(), message.to_owned()));
    let spec = AppSpec { fonts: theme::FONTS, families: theme::FAMILIES, ..AppSpec::new("Troupe cannot start", (560.0, 220.0), Refusal::default, view) };
    if let Err(e) = rune::App::new(spec).try_run() {
        log::error!("[startup] no window for the refusal: {e}");
    }
}
