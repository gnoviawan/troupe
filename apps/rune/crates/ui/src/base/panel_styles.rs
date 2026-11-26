//! Panel chrome from `components/ui/panel-styles.ts`: the segmented track, the pressed pill, the
//! panel field, the 40 px panel header and the quiet icon buttons. As on the web, these hold only
//! the look; heights and widths stay at the call site unless the constant fixes them.

use rune::{El, Icon, TextEdit, icon, row, text, text_input};
use theme::Tokens;

use crate::paint::shadows;
use crate::type_::sans;

/// A segmented track: a bordered `card` strip whose active segment is a keycap.
pub struct Segmented<'a> {
    label: String,
    items: &'a [&'a str],
    active: usize,
}

impl<'a> Segmented<'a> {
    pub fn new(label: impl Into<String>, items: &'a [&'a str], active: usize) -> Segmented<'a> {
        Segmented {
            label: label.into(),
            items,
            active,
        }
    }

    /// The track; segment `i` is keyed `"{key}/{i}"`, and pressing it calls `on_pick(state, i)`.
    pub fn build<S: 'static>(
        self,
        t: &Tokens,
        key: &str,
        capitalize: bool,
        on_pick: impl Fn(&mut S, usize) + Clone + 'static,
    ) -> El {
        let c = &t.colors;
        let mut track = row()
            .role("tablist")
            .label(self.label)
            .gap(2.0)
            .padding(3.0)
            .radius(8.0)
            .fill(c.card)
            .border(1.0, c.border);
        for (i, item) in self.items.iter().enumerate() {
            let active = i == self.active;
            let name = if capitalize { capitalized(item) } else { (*item).to_owned() };
            let mut seg = row()
                .key(Self::segment_key(key, i))
                .role("tab")
                .label(name.clone())
                .checked(active)
                .h(28.0)
                .px(8.0)
                .center()
                .radius(6.0)
                .focusable(true)
                .focus_ring(2.0, 0.0, c.focus_ring)
                .child(
                    text(name)
                        .font(sans(12.0, 500.0, 16.0))
                        .color(if active { c.foreground } else { c.muted_foreground }),
                );
            let pick = on_pick.clone();
            seg = seg.on_press(move |s: &mut S| pick(s, i));
            if active {
                seg = shadows(seg.fill(c.keycap_fill), &t.shadows.keycap_shadow);
            }
            track = track.child(seg);
        }
        track
    }

    pub fn segment_key(key: &str, i: usize) -> String {
        format!("{key}/{i}")
    }
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A bordered on/off pill (option pills, multi-select answers): a foreground wash when pressed.
pub fn pressed_toggle(t: &Tokens, label: impl Into<String>, pressed: bool) -> El {
    let c = &t.colors;
    let label = label.into();
    let mut e = row()
        .role("button")
        .checked(pressed)
        .label(label.clone())
        .h(28.0)
        .px(11.0)
        .center()
        .radius(6.0)
        .border(1.0, c.border)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus_ring)
        .child(
            text(label)
                .font(sans(12.0, 400.0, 16.0))
                .color(if pressed { c.foreground } else { c.muted_foreground }),
        );
    e = if pressed {
        e.fill(c.foreground.alpha(0.1))
    } else {
        e.hover(c.foreground.alpha(0.03))
    };
    e
}

/// The panel text field: the field surface at 12 px (16 px on a coarse pointer).
pub fn panel_field(t: &Tokens, edit: &TextEdit, placeholder: &str, coarse: bool) -> El {
    let c = &t.colors;
    let font = if coarse { sans(16.0, 400.0, 24.0) } else { sans(12.0, 400.0, 16.0) };
    text_input(edit, font)
        .single_line()
        .label(placeholder.to_owned())
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .px(11.0)
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border.alpha(0.6))
        .focus_ring(2.0, -1.0, c.focus_ring)
}

/// A quiet icon button: a muted glyph and a faint foreground wash on hover. Size stays at the
/// call site ([`panel_icon_button`] is the 28 px one).
pub fn quiet_icon_button(t: &Tokens, glyph: &'static Icon, label: impl Into<String>, side: f32, glyph_size: f32) -> El {
    let c = &t.colors;
    row()
        .role("button")
        .label(label)
        .w(side)
        .h(side)
        .center()
        .radius(6.0)
        .hover(c.foreground.alpha(0.03))
        .hover_group()
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus_ring)
        .child(icon(glyph, glyph_size).color(c.muted_foreground).group_color(c.foreground))
}

/// The 28 px panel-header icon button with a 14 px glyph.
pub fn panel_icon_button(t: &Tokens, glyph: &'static Icon, label: impl Into<String>) -> El {
    quiet_icon_button(t, glyph, label, 28.0, 14.0)
}

/// The 40 px panel header: a `.label-section` title at the start, `actions` at the end.
pub fn panel_header(t: &Tokens, title: impl Into<String>, actions: Vec<El>) -> El {
    row()
        .h(40.0)
        .items_center()
        .justify_between()
        .pl(16.0)
        .pr(6.0)
        .child(
            text(title)
                .font(sans(12.0, 500.0, 16.8))
                .color(t.colors.foreground)
                .role("heading"),
        )
        .child(row().children(actions))
}
