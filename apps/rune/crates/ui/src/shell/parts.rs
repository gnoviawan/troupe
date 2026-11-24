//! The main area's top row, the side panel's header and resize edge, the tool strip's buttons
//! and the launcher heading the main area shows until a tab is open.

use rune::{Color, Cursor, El, Icon, col, el, icon, row, text};
use theme::Tokens;

use super::TOP_ROW;
use crate::CountBadge;
use crate::skeleton::skeleton;
use crate::type_::sans;

/// The Troupe brand mark (`TermulMark`), its 569x507 artwork centred in a square box.
pub static TROUPE_MARK: Icon = Icon {
    name: "TroupeMark",
    view: 569.0,
    body: r##"<g transform="translate(0,31)"><g transform="matrix(1,0,0,1,-446.039965,-826.589138)" fill="currentColor"><g transform="matrix(2.924821,0,0,2.924821,37.282287,448.301549)"><g transform="matrix(1,0,0,1,-26.278515,0)"><path fill-rule="evenodd" d="M272.622,302.62L251.873,302.62C254.474,302.62 256.699,300.753 257.148,298.192L288.639,157.389L248.668,157.389L216.975,296.321C216.918,296.642 216.89,296.96 216.89,297.273C216.89,296.96 216.918,296.642 216.975,296.321L248.668,157.389L354.887,157.389C358.216,157.389 360.739,160.394 360.163,163.673L353.504,201.53C353.053,204.091 350.829,205.959 348.228,205.959L300.845,205.959L277.898,298.192C277.448,300.753 275.223,302.62 272.622,302.62Z"/></g><path fill-rule="evenodd" d="M146.496,133.765L139.837,171.622C139.261,174.901 141.784,177.907 145.113,177.907L190.062,177.907L167.442,268.284C166.865,271.563 169.388,274.568 172.717,274.568L223.11,274.568C225.71,274.568 227.935,272.701 228.385,270.14L258.769,135.621C259.346,132.342 256.824,129.337 253.494,129.337L151.772,129.337C149.171,129.337 146.947,131.204 146.496,133.765Z"/></g></g></g>"##,
};

/// A 44 px top row with a bottom hairline: `start` (the expand toggle while the rail is
/// collapsed), drag space, then `end` (the pane's actions).
pub fn top_row(t: &Tokens, start: Vec<El>, end: Vec<El>) -> El {
    row()
        .w_full()
        .h(TOP_ROW)
        .items_center()
        .pl(8.0)
        .pr(6.0)
        .gap(4.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .children(start)
        .child(el().grow(1.0).h_full().window_drag())
        .children(end)
}

/// The side panel's header (`SidePanelHeader`): the view's title in 12 px medium, its actions,
/// then `close`.
pub fn panel_header(t: &Tokens, title: &str, actions: Vec<El>) -> El {
    row()
        .w_full()
        .h(TOP_ROW)
        .items_center()
        .gap(6.0)
        .pl(12.0)
        .pr(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .child(
            text(title.to_owned())
                .font(sans(12.0, 500.0, 18.0))
                .color(t.colors.foreground)
                .role("heading")
                .single_line()
                .truncate()
                .min_w(0.0)
                .grow(1.0)
                .window_drag(),
        )
        .children(actions)
}

/// What the side panel shows while its view loads (`ShellSkeleton`).
pub fn panel_skeleton(t: &Tokens) -> El {
    let bar = |w: f32| skeleton(t).w(w).h(12.0).radius(4.0);
    col()
        .w_full()
        .gap(14.0)
        .padding(16.0)
        .child(skeleton(t).w_full().h(32.0).radius(8.0))
        .child(bar(140.0))
        .child(bar(96.0))
        .child(bar(180.0))
        .child(bar(120.0))
}

/// The side panel's left edge: a 4 px hit strip that shows the border colour on hover and
/// resizes with `col-resize`.
pub fn resize_handle(t: &Tokens, label: &str) -> El {
    el().w(4.0)
        .h_full()
        .role("separator")
        .label(label.to_owned())
        .focusable(true)
        .cursor(Cursor::ResizeColumn)
        .hover(t.colors.border)
}

/// A tool strip button (`ToolStrip.Button`): 32 px, a 14 px glyph, the keycap wash when its
/// view is open, and an optional count pinned to its top-right corner.
pub fn tool_button(
    t: &Tokens,
    glyph: &'static Icon,
    label: &str,
    active: bool,
    disabled: bool,
    badge: Option<u64>,
) -> El {
    let c = &t.colors;
    let ink = if active {
        c.foreground
    } else {
        c.muted_foreground
    };
    let mut glyph = icon(glyph, 14.0).color(ink).a11y_hidden();
    if !disabled {
        glyph = glyph.group_color(c.foreground);
    }
    let mut e = row()
        .role("button")
        .label(label.to_owned())
        .w(32.0)
        .h(32.0)
        .center()
        .radius(8.0)
        .fill(if active {
            c.row_hover
        } else {
            Color::TRANSPARENT
        })
        .focusable(!disabled)
        .disabled(disabled)
        .focus_ring(2.0, 0.0, c.focus_ring)
        .child(glyph);
    if disabled {
        e = e.opacity(0.4);
    } else {
        e = e.hover(c.row_hover).hover_group().help(label.to_owned());
    }
    match badge {
        Some(n) if n > 0 => e.child(
            CountBadge::new(n)
                .max(99)
                .text_size(12.0)
                .build(t)
                .h(16.0)
                .min_w(16.0)
                .pin(None, Some(-3.0), Some(-4.0), None),
        ),
        _ => e,
    }
}

/// The web client's connection lamp in the tool strip: the connection colour while live,
/// muted while it reconnects.
pub fn connection_lamp(t: &Tokens, live: bool) -> El {
    let ink = if live {
        t.colors.connection
    } else {
        t.colors.muted_foreground
    };
    row()
        .w(32.0)
        .h(32.0)
        .center()
        .role("status")
        .label(if live { "Connected" } else { "Reconnecting" })
        .child(el().w(8.0).h(8.0).radius(4.0).fill(ink))
}

/// The launcher's greeting in the main area: the mark and "What should we do in <project>?".
/// The space below holds the composer's place until the composer is ported.
pub fn launcher_heading(t: &Tokens, project: &str) -> El {
    col()
        .w_full()
        .items_center()
        .gap(20.0)
        .child(
            icon(&TROUPE_MARK, 56.0)
                .color(t.colors.secondary_foreground)
                .label("Troupe"),
        )
        .child(
            text(format!("What should we do in {project}?"))
                .font(sans(22.0, 400.0, 28.0))
                .color(t.colors.foreground)
                .role("heading"),
        )
        .child(el().w(1.0).h(188.0))
}
