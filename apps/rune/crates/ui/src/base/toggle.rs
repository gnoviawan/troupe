//! `Toggle` (`components/ui/toggle.tsx`): a two-state button, transparent at rest, `accent` when
//! on. `outline` adds a 1 px `input` border. Its glyph keeps the icon's own 24 px size.

use rune::{El, Icon, icon, row, text};
use theme::Tokens;

use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleVariant {
    #[default]
    Default,
    Outline,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleSize {
    #[default]
    Default,
    Sm,
    Lg,
}

impl ToggleSize {
    pub const ALL: [ToggleSize; 3] = [ToggleSize::Sm, ToggleSize::Default, ToggleSize::Lg];

    pub fn name(self) -> &'static str {
        match self {
            ToggleSize::Default => "default",
            ToggleSize::Sm => "sm",
            ToggleSize::Lg => "lg",
        }
    }

    /// Height and horizontal padding.
    fn metrics(self) -> (f32, f32) {
        match self {
            ToggleSize::Default => (40.0, 12.0),
            ToggleSize::Sm => (36.0, 10.0),
            ToggleSize::Lg => (44.0, 20.0),
        }
    }
}

enum Face {
    Text(String),
    Icon(&'static Icon),
}

pub struct Toggle {
    label: String,
    face: Face,
    pressed: bool,
    variant: ToggleVariant,
    size: ToggleSize,
    disabled: bool,
}

impl Toggle {
    pub fn new(label: impl Into<String>) -> Toggle {
        let label = label.into();
        Toggle {
            face: Face::Text(label.clone()),
            label,
            pressed: false,
            variant: ToggleVariant::Default,
            size: ToggleSize::Default,
            disabled: false,
        }
    }
    /// A toggle that shows `glyph`; `label` is its accessible name.
    pub fn icon(glyph: &'static Icon, label: impl Into<String>) -> Toggle {
        Toggle {
            face: Face::Icon(glyph),
            ..Toggle::new(label)
        }
    }
    pub fn pressed(mut self, p: bool) -> Self {
        self.pressed = p;
        self
    }
    pub fn variant(mut self, v: ToggleVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn size(mut self, s: ToggleSize) -> Self {
        self.size = s;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let (h, px) = self.size.metrics();
        let outline = self.variant == ToggleVariant::Outline;
        let ink = if self.pressed { c.accent_foreground } else { c.foreground };
        let mut e = row()
            .role("button")
            .checked(self.pressed)
            .label(self.label)
            .h(h)
            .px(if outline { px + 1.0 } else { px })
            .center()
            .radius(6.0)
            .focusable(!self.disabled)
            .disabled(self.disabled)
            .focus_ring(2.0, 2.0, c.focus_ring);
        if outline {
            e = e.border(1.0, c.input);
        }
        if self.pressed {
            e = e.fill(c.accent);
        }
        if !self.disabled {
            e = e.hover(if outline { c.accent } else { c.muted });
        }
        if self.disabled {
            e = e.opacity(0.5);
        }
        e.child(match self.face {
            Face::Text(s) => text(s).font(sans(14.0, 500.0, 20.0)).color(ink),
            Face::Icon(i) => icon(i, 24.0).color(ink),
        })
    }
}
