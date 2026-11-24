//! The preview scaffolding of `design-system/src/previews/kit.tsx`: the padded body, the labelled
//! row and the caption every scene is built from.

use rune::{El, col, row, text};
use theme::Tokens;
use ui::type_::{MONO_XS, sans};

/// `Pad`: `p-5` with `gap` between children (the preview default is 20).
pub fn pad(gap: f32, children: Vec<El>) -> El {
    col().w_full().padding(20.0).gap(gap).children(children)
}

/// `Row`: a 96 px mono label, then the specimens wrapping 12 px apart.
pub fn labelled(t: &Tokens, label: &str, children: Vec<El>) -> El {
    row()
        .w_full()
        .items_start()
        .gap(16.0)
        .child(
            text(label.to_owned())
                .font(MONO_XS)
                .color(t.colors.muted_foreground)
                .w(96.0)
                .pt(6.0),
        )
        .child(
            row()
                .grow(1.0)
                .min_w(0.0)
                .flex_wrap()
                .items_center()
                .gap(12.0)
                .children(children),
        )
}

/// `Caption`: 12 px muted text on a 1.625 line.
pub fn caption(t: &Tokens, s: &str) -> El {
    text(s.to_owned())
        .font(sans(12.0, 400.0, 19.5))
        .color(t.colors.muted_foreground)
        .wrap()
}

/// The note a scene shows until the unit that owns its parts lands.
pub fn pending(t: &Tokens, name: &str, owner: &str, why: &str) -> El {
    col()
        .w_full()
        .h_full()
        .center()
        .gap(6.0)
        .padding(20.0)
        .child(text(owner.to_owned()).font(sans(16.0, 600.0, 24.0)).color(t.colors.foreground))
        .child(text(name.to_owned()).font(sans(12.0, 500.0, 16.0)).color(t.colors.muted_foreground))
        .child(caption(t, why))
}
