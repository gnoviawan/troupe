//! `ChatMarkdownTable`: a GFM table in a hairline card whose toolbar holds Copy, Download and
//! View fullscreen, and the full-window view that opens from it.

use rune::{El, col, row};
use theme::Tokens;
use theme::icons as i;

use super::{Hooks, copies, markdown};
use crate::overlay::Close;
use crate::{IconActionButton, IconActionSize, icon_action_group};

/// The overlay key of the full-window table.
pub const TABLE_ZOOM: &str = "table-zoom";

/// The table's card: `my-1 rounded-md border border-border/40`, the toolbar band, then the
/// table on `background`.
pub(super) fn framed<S: 'static>(t: &'static Tokens, src: &str, streaming: bool, hooks: &Hooks<S>) -> El {
    let c = &t.colors;
    let edge = c.border.alpha(0.4);
    let zoom = hooks.on_table_zoom.clone();
    let zoomed = src.to_owned();
    let buttons = vec![
        IconActionButton::new(&i::COPY, "Copy table")
            .size(IconActionSize::Sm)
            .build(t)
            .on_press_tx(copies::<S>(src.to_owned())),
        IconActionButton::new(&i::DOWNLOAD, "Download table")
            .size(IconActionSize::Sm)
            .build(t),
        IconActionButton::new(&i::MAXIMIZE2, "View fullscreen")
            .size(IconActionSize::Sm)
            .disabled(streaming)
            .build(t)
            .key("table-fullscreen")
            .on_press(move |s: &mut S| zoom(s, zoomed.clone())),
    ];
    col()
        .w_full()
        .my(4.0)
        .radius(6.0)
        .clip_rounded(6.0)
        .border(1.0, edge)
        .child(
            row()
                .key("table-toolbar")
                .w_full()
                .justify_end()
                .px(4.0)
                .py(2.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], edge)
                .child(icon_action_group(t, true, buttons)),
        )
        .child(
            col()
                .role("region")
                .label("Markdown table")
                .w_full()
                .fill(c.background)
                .child(markdown(t, src.to_owned(), streaming, hooks).render(&super::markdown_style(t))),
        )
}

/// The full-window table (`TableFullscreen`) for `src`, pushed into `layer` when `open`.
/// `close` runs on Escape and on Exit fullscreen.
pub fn table_zoom<S: 'static>(
    t: &'static Tokens,
    src: &str,
    open: bool,
    hooks: &Hooks<S>,
    close: impl Fn(&mut S) + Clone + 'static,
    layer: &mut Vec<El>,
) {
    if !open {
        return;
    }
    let c = &t.colors;
    let shut = Close::new(close);
    let buttons = vec![
        IconActionButton::new(&i::COPY, "Copy table")
            .build(t)
            .on_press_tx(copies::<S>(src.to_owned())),
        IconActionButton::new(&i::DOWNLOAD, "Download table").build(t),
        shut.press(IconActionButton::new(&i::X, "Exit fullscreen").build(t).key("table-zoom-exit")),
    ];
    let body = col()
        .key(format!("{TABLE_ZOOM}-dialog"))
        .role("dialog")
        .label("Table")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(c.background)
        .child(row().w_full().justify_end().padding(16.0).child(icon_action_group(t, false, buttons)))
        .child(
            col()
                .key("table-zoom-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(16.0)
                .pb(16.0)
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(
                    col()
                        .w_full()
                        .border(1.0, c.border)
                        .child(markdown(t, src.to_owned(), false, hooks).render(&super::markdown_style(t))),
                ),
        );
    layer.push(shut.modal(body));
}
