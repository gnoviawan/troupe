//! The rail (`ShellRail`, `RailBody`, `RailHeader`, `ProjectSidebar`): expanded to its saved
//! width or collapsed to a 56 px icon column. Every row puts its 16 px icon box at x = 20 and
//! its label at x = 44 when expanded; collapsed, the icon stays on the x = 28 column.

use rune::{Color, El, Icon, el, icon, row, text};
use theme::Tokens;

use super::TROUPE_MARK;
use super::{COLLAPSED_RAIL, TOP_ROW};
use crate::type_::sans;

/// A rail row is 30 px tall (`h-7.5`) with a 12 px inset and an 8 px icon-to-label gap.
const ROW: f32 = 30.0;
/// The collapsed row's square fill spans x = 13..43 of the 56 px rail.
const COLLAPSED_ROW_INSET: f32 = (COLLAPSED_RAIL - ROW) / 2.0;

fn label_text(t: &Tokens, label: &str, weight: f32) -> El {
    text(label.to_owned())
        .font(sans(12.0, weight, 16.0))
        .color(t.colors.secondary_foreground)
        .single_line()
        .truncate()
        .min_w(0.0)
        .grow(1.0)
}

/// The 16 px box every rail row puts its leading glyph in.
fn icon_box(lead: El) -> El {
    row().w(16.0).h(16.0).center().child(lead)
}

fn row_shell(t: &Tokens, label: &str, collapsed: bool, disabled: bool) -> El {
    let base = row()
        .role("button")
        .label(label.to_owned())
        .h(ROW)
        .items_center()
        .radius(8.0)
        .focusable(!disabled)
        .disabled(disabled)
        .focus_ring(2.0, 0.0, t.colors.focus_ring);
    let base = if collapsed {
        base.w(ROW)
            .ml(COLLAPSED_ROW_INSET)
            .center()
            .help(label.to_owned())
    } else {
        base.w_full().px(12.0).gap(8.0)
    };
    if disabled {
        base.opacity(0.4)
    } else {
        base.hover(t.colors.row_hover).hover_group()
    }
}

/// A navigation row (New chat, Search, Canvas, Automations): a 14 px muted glyph and a 12 px
/// medium label; icon-only on the collapsed rail.
pub fn nav_row(
    t: &Tokens,
    glyph: &'static Icon,
    label: &str,
    collapsed: bool,
    disabled: bool,
) -> El {
    let mut glyph = icon(glyph, 14.0)
        .color(t.colors.muted_foreground)
        .a11y_hidden();
    if !disabled {
        glyph = glyph.group_color(t.colors.foreground);
    }
    let e = row_shell(t, label, collapsed, disabled).child(icon_box(glyph));
    if collapsed {
        e
    } else {
        e.child(label_text(t, label, 500.0))
    }
}

/// The swatch colour of a project colour name (`blue`, `green`, ...), as `getColorClasses`
/// maps it; an unknown name is gray.
pub fn project_color(t: &Tokens, name: &str) -> Color {
    let c = &t.colors;
    match name {
        "blue" => c.project_blue,
        "cyan" => c.project_cyan,
        "green" => c.project_green,
        "orange" => c.project_orange,
        "pink" => c.project_pink,
        "purple" => c.project_purple,
        "red" => c.project_red,
        "yellow" => c.project_yellow,
        _ => c.project_gray,
    }
}

/// `ProjectIcon`'s letter badge: the project's first letter on its colour, 16 px, 4 px corners.
pub fn project_badge(t: &Tokens, name: &str, color: &str) -> El {
    let letter: String = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    row()
        .w(16.0)
        .h(16.0)
        .center()
        .radius(4.0)
        .fill(project_color(t, color))
        .a11y_hidden()
        .child(
            text(letter)
                .font(sans(10.0, 600.0, 10.0))
                .color(t.colors.primary_foreground),
        )
}

/// A project row: badge and name, then `trailing` (the active project's new-chat `+`). The
/// collapsed rail shows the badge alone, on the keycap surface when `active`.
pub fn project_row(
    t: &Tokens,
    name: &str,
    color: &str,
    active: bool,
    collapsed: bool,
    trailing: Option<El>,
) -> El {
    let e = row_shell(t, &format!("Project: {name}"), collapsed, false)
        .child(project_badge(t, name, color));
    if collapsed {
        return if active {
            e.fill(t.colors.row_hover)
        } else {
            e
        };
    }
    let e = e.child(label_text(t, name, 500.0));
    match trailing {
        Some(trailing) => e.child(trailing),
        None => e,
    }
}

/// A square rail action: `side` 28 (`sm`) or 32 (`base`) with a 14 px glyph, or 20 for a
/// section's `+`. Hover changes only the ink and adds the 3 % wash; `pressed` is the keycap
/// "you are here" surface.
pub fn rail_button(
    t: &Tokens,
    glyph: &'static Icon,
    label: &str,
    side: f32,
    glyph_px: f32,
    pressed: bool,
) -> El {
    let c = &t.colors;
    let ink = if pressed {
        c.foreground
    } else {
        c.muted_foreground
    };
    row()
        .role("button")
        .label(label.to_owned())
        .w(side)
        .h(side)
        .center()
        .radius(if side > 20.0 { 8.0 } else { 4.0 })
        .fill(if pressed {
            c.row_hover
        } else {
            Color::TRANSPARENT
        })
        .hover(c.row_hover)
        .hover_group()
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus_ring)
        .help(label.to_owned())
        .child(
            icon(glyph, glyph_px)
                .color(ink)
                .group_color(c.foreground)
                .a11y_hidden(),
        )
}

/// The rail's part of the top row: the Troupe mark centred on the icon column, drag space,
/// then the collapse toggle 16 px from the edge (expanded only).
pub fn rail_header(t: &Tokens, collapsed: bool, toggle: Option<El>) -> El {
    let mark = row().w(COLLAPSED_RAIL).h_full().center().child(
        icon(&TROUPE_MARK, 18.0)
            .color(t.colors.foreground)
            .label("Troupe"),
    );
    let header = row().w_full().h(TOP_ROW).items_center().child(mark);
    match (collapsed, toggle) {
        (false, Some(toggle)) => header
            .child(el().grow(1.0).h_full().window_drag())
            .child(row().mr(16.0).child(toggle)),
        _ => header.child(el().grow(1.0).h_full().window_drag()),
    }
}

/// A rail section title (Projects, Quick chats) in 12 px medium muted ink, with its action
/// (the `+`) on the right lane of the rows' trailing marks.
pub fn section_header(t: &Tokens, title: &str, action: Option<El>) -> El {
    let header = row()
        .w_full()
        .h(28.0)
        .items_center()
        .pl(20.0)
        .pr(20.0)
        .child(
            text(title.to_owned())
                .font(sans(12.0, 500.0, 16.8))
                .color(t.colors.muted_foreground)
                .role("heading")
                .grow(1.0),
        );
    match action {
        Some(action) => header.child(action),
        None => header,
    }
}
