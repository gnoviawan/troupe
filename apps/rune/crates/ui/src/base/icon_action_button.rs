//! `IconActionButton` and `IconActionGroup` (`components/ui/icon-action-button.tsx`): compact
//! chat and code-block icon controls. Hover changes only the ink; there is no press scale. The
//! default slot is the 44 px touch target; `sm` is a 24 px slot for dense chrome. The web
//! client's tooltip is the overlay layer's (R4c); until then the label is Rune's help text.

use rune::{El, Icon, icon, row};
use theme::Tokens;

use crate::paint::pad;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconActionSize {
    #[default]
    Default,
    Sm,
}

pub struct IconActionButton {
    glyph: &'static Icon,
    label: String,
    size: IconActionSize,
    disabled: bool,
}

impl IconActionButton {
    pub fn new(glyph: &'static Icon, label: impl Into<String>) -> IconActionButton {
        IconActionButton {
            glyph,
            label: label.into(),
            size: IconActionSize::Default,
            disabled: false,
        }
    }
    pub fn size(mut self, s: IconActionSize) -> Self {
        self.size = s;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let side = match self.size {
            IconActionSize::Default => 44.0,
            IconActionSize::Sm => 24.0,
        };
        let ink = if self.disabled { c.disabled_foreground } else { c.muted_foreground };
        let mut glyph = icon(self.glyph, 14.0).color(ink);
        if !self.disabled {
            glyph = glyph.group_color(c.foreground);
        }
        let e = row()
            .role("button")
            .label(self.label.clone())
            .w(side)
            .h(side)
            .center()
            .radius(6.0)
            .focusable(!self.disabled)
            .disabled(self.disabled)
            .focus_ring(2.0, 0.0, c.focus_ring)
            .child(glyph);
        if self.disabled {
            e
        } else {
            e.hover_group().press_color(c.foreground).help(self.label)
        }
    }
}

/// The pill around a row of icon actions: a `sidebar` edge on a 70 % `sidebar` wash. `dense`
/// is the tight variant for `sm` buttons.
pub fn icon_action_group(t: &Tokens, dense: bool, buttons: Vec<El>) -> El {
    let c = &t.colors;
    let padding = if dense { [3.0, 5.0, 3.0, 5.0] } else { [5.0, 7.0, 5.0, 7.0] };
    pad(row(), padding)
        .role("group")
        .items_center()
        .gap(8.0)
        .radius(6.0)
        .fill(c.sidebar_background.alpha(0.7))
        .border(1.0, c.sidebar_background)
        .children(buttons)
}
