//! The phone shell's parts (`components/mobile/`), at the metrics the web client paints at
//! 390x844 with a coarse pointer: the 56 px header and its 44 px icon buttons, the drawer's
//! section headers and rows, and the 44 px rows of a bottom sheet. Builders return elements
//! without handlers; the app adds keys and presses.

use rune::{El, Icon, col, icon, row, text};
use theme::Tokens;

use crate::type_::sans;

/// Height of the phone header (`min-h-14`).
pub const HEADER: f32 = 56.0;
/// The phone's touch floor (`size-11`).
pub const TOUCH: f32 = 44.0;

/// A ghost icon button in the header or the drawer: 44 px square, a 16 px glyph.
pub fn icon_button(t: &Tokens, glyph: &'static Icon, label: &str) -> El {
    let c = &t.colors;
    row()
        .role("button")
        .label(label.to_owned())
        .size(TOUCH, TOUCH)
        .center()
        .radius(8.0)
        .hover(c.foreground.alpha(0.06))
        .child(icon(glyph, 16.0).color(c.foreground))
}

/// The header's title block: "Troupe" over the project line with its chevron, which opens
/// the project sheet.
pub fn header_title(t: &Tokens, title: &str, subtitle: El) -> El {
    col()
        .grow(1.0)
        .min_w(0.0)
        .justify_center()
        .child(
            text(title.to_owned())
                .font(sans(15.0, 500.0, 20.0))
                .color(t.colors.foreground)
                .single_line()
                .truncate(),
        )
        .child(subtitle)
}

/// The project line under the header title (`text-sm leading-[18px] text-muted-foreground`)
/// with its 12 px chevron.
pub fn header_subtitle(t: &Tokens, line: &str, label: &str) -> El {
    let c = &t.colors;
    let chevron = theme::icons::named("ChevronDown").expect("bundled icon");
    row()
        .role("button")
        .label(label.to_owned())
        .gap(4.0)
        .items_center()
        .min_w(0.0)
        .child(
            text(line.to_owned())
                .font(sans(14.0, 400.0, 18.0))
                .color(c.muted_foreground)
                .single_line()
                .truncate(),
        )
        .child(icon(chevron, 12.0).color(c.muted_foreground))
}

/// A drawer section header ("Today", "Quick chats"): 15/500 muted, with an optional action.
pub fn section(t: &Tokens, title: &str, action: Option<El>) -> El {
    let mut e = row()
        .w_full()
        .h(TOUCH)
        .pl(24.0)
        .pr(8.0)
        .items_center()
        .justify_between()
        .child(
            text(title.to_owned())
                .font(sans(16.0, 500.0, 24.0))
                .color(t.colors.muted_foreground)
                .role("heading"),
        );
    if let Some(a) = action {
        e = e.child(a);
    }
    e
}

/// A bottom sheet's row: a 16 px glyph and 16 px text, 44 px tall; destructive rows are red.
pub fn sheet_row(t: &Tokens, glyph: &'static Icon, label: &str, destructive: bool) -> El {
    let c = &t.colors;
    let ink = if destructive {
        c.destructive
    } else {
        c.foreground
    };
    row()
        .role("button")
        .label(label.to_owned())
        .w_full()
        .min_h(TOUCH)
        .px(8.0)
        .gap(12.0)
        .items_center()
        .radius(10.0)
        .hover(c.foreground.alpha(0.06))
        .child(icon(glyph, 16.0).color(ink).a11y_hidden())
        .child(
            text(label.to_owned())
                .font(sans(16.0, 400.0, 24.0))
                .color(ink)
                .grow(1.0)
                .truncate(),
        )
}

/// A sheet's centred subtitle under its title (`text-sm text-muted-foreground`).
pub fn sheet_subtitle(t: &Tokens, line: &str) -> El {
    row().w_full().justify_center().pb(8.0).child(
        text(line.to_owned())
            .font(sans(14.0, 400.0, 20.0))
            .color(t.colors.muted_foreground),
    )
}
