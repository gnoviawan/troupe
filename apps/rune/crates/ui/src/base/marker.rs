//! `Marker` (`components/ui/marker.tsx`): a 12 px muted line in a transcript (a branch switch,
//! a test result, a date). `border` adds a bottom hairline; `separator` centres the text between
//! two hairlines.

use rune::{El, Icon, el, icon, row, text};
use theme::Tokens;

use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkerVariant {
    #[default]
    Default,
    Border,
    Separator,
}

pub struct Marker {
    label: String,
    glyph: Option<&'static Icon>,
    variant: MarkerVariant,
}

impl Marker {
    pub fn new(label: impl Into<String>) -> Marker {
        Marker {
            label: label.into(),
            glyph: None,
            variant: MarkerVariant::Default,
        }
    }
    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.glyph = Some(i);
        self
    }
    pub fn variant(mut self, v: MarkerVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let ink = c.muted_foreground;
        let line = || el().h(1.0).grow(1.0).fill(c.border);
        let mut e = row().w_full().min_w(0.0).items_center().gap(8.0);
        if self.variant == MarkerVariant::Separator {
            e = e.child(line());
        }
        if let Some(i) = self.glyph {
            e = e.child(icon(i, 14.0).color(ink).a11y_hidden());
        }
        e = e.child(text(self.label).font(sans(12.0, 400.0, 16.0)).color(ink));
        match self.variant {
            MarkerVariant::Default => e,
            MarkerVariant::Border => e.pb(9.0).border_sides([0.0, 0.0, 1.0, 0.0], c.border),
            MarkerVariant::Separator => e.child(line()),
        }
    }
}
