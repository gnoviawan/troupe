//! `Badge` (`components/ui/badge.tsx`): a 12 px semibold pill in four variants.

use rune::{Color, El, row, text};
use theme::Tokens;

use crate::paint::{PILL, pad};
use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Destructive,
    Outline,
}

impl BadgeVariant {
    pub const ALL: [BadgeVariant; 4] = [
        BadgeVariant::Default,
        BadgeVariant::Secondary,
        BadgeVariant::Outline,
        BadgeVariant::Destructive,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BadgeVariant::Default => "default",
            BadgeVariant::Secondary => "secondary",
            BadgeVariant::Destructive => "destructive",
            BadgeVariant::Outline => "outline",
        }
    }
}

pub struct Badge {
    label: String,
    variant: BadgeVariant,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Badge {
        Badge {
            label: label.into(),
            variant: BadgeVariant::Default,
        }
    }

    pub fn variant(mut self, v: BadgeVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let (fill, ink, edge) = match self.variant {
            BadgeVariant::Default => (Some(c.primary_fill), c.primary_foreground, Color::TRANSPARENT),
            BadgeVariant::Secondary => (Some(c.secondary), c.secondary_foreground, Color::TRANSPARENT),
            BadgeVariant::Destructive => {
                (Some(c.destructive_fill), c.destructive_foreground, Color::TRANSPARENT)
            }
            BadgeVariant::Outline => (None, c.foreground, c.border),
        };
        // The 1 px border takes layout space in CSS and none in Rune: it joins the padding.
        let mut e = pad(row(), [3.0, 11.0, 3.0, 11.0])
            .radius(PILL)
            .border(1.0, edge)
            .child(text(self.label).font(sans(12.0, 600.0, 16.0)).color(ink));
        if let Some(f) = fill {
            e = e.fill(f);
        }
        e
    }
}
