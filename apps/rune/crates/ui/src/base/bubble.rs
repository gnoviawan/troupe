//! `Bubble`, `BubbleContent`, `BubbleGroup` and `BubbleReactions` (`components/ui/bubble.tsx`):
//! a chat message's rounded box in seven variants. `ghost` spans the row with no box. Text is
//! 14 px on a 1.625 line (`leading-relaxed`).

use rune::{El, col, row, text};
use theme::Tokens;

use crate::paint::{PILL, pad, shadows};
use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BubbleVariant {
    #[default]
    Default,
    Secondary,
    Muted,
    Tinted,
    Outline,
    Ghost,
    Destructive,
}

impl BubbleVariant {
    pub fn name(self) -> &'static str {
        match self {
            BubbleVariant::Default => "default",
            BubbleVariant::Secondary => "secondary",
            BubbleVariant::Muted => "muted",
            BubbleVariant::Tinted => "tinted",
            BubbleVariant::Outline => "outline",
            BubbleVariant::Ghost => "ghost",
            BubbleVariant::Destructive => "destructive",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BubbleAlign {
    #[default]
    Start,
    End,
}

pub const BUBBLE_TEXT: rune::TextStyle = sans(14.0, 400.0, 22.75);

pub struct Bubble {
    body: String,
    variant: BubbleVariant,
    align: BubbleAlign,
    max_w: f32,
    reactions: Option<El>,
}

impl Bubble {
    pub fn new(body: impl Into<String>) -> Bubble {
        Bubble {
            body: body.into(),
            variant: BubbleVariant::Default,
            align: BubbleAlign::Start,
            max_w: f32::INFINITY,
            reactions: None,
        }
    }
    pub fn variant(mut self, v: BubbleVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn align(mut self, a: BubbleAlign) -> Self {
        self.align = a;
        self
    }
    /// `max-w-[80%]` of the row: pass the row's width times 0.8.
    pub fn max_w(mut self, w: f32) -> Self {
        self.max_w = w;
        self
    }
    /// A [`bubble_reactions`] pill hung over the bubble's bottom edge.
    pub fn reactions(mut self, r: El) -> Self {
        self.reactions = Some(r);
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let (fill, ink) = match self.variant {
            BubbleVariant::Default => (Some(c.primary_fill), c.primary_foreground),
            BubbleVariant::Secondary => (Some(c.secondary), c.secondary_foreground),
            BubbleVariant::Muted => (Some(c.muted), c.foreground),
            BubbleVariant::Tinted => (Some(c.secondary), c.foreground),
            BubbleVariant::Outline => (Some(c.background), c.foreground),
            BubbleVariant::Ghost => (None, c.foreground),
            BubbleVariant::Destructive => (Some(c.destructive_fill), c.destructive_foreground),
        };
        let words = text(self.body).font(BUBBLE_TEXT).color(ink).wrap();
        let mut content = col().child(words);
        if self.variant == BubbleVariant::Ghost {
            content = content.w_full();
        } else {
            let edge = if self.variant == BubbleVariant::Outline { 1.0 } else { 0.0 };
            content = pad(content, [10.0 + edge, 16.0 + edge, 10.0 + edge, 16.0 + edge])
                .radius(16.0)
                .max_w(self.max_w);
            if edge > 0.0 {
                content = content.border(1.0, c.border);
            }
        }
        if let Some(f) = fill {
            content = content.fill(f);
        }
        let mut bubble = col().child(content);
        if let Some(r) = self.reactions {
            bubble = bubble.child(r.absolute(14.0, -1.0).mt(-12.0));
        }
        match self.align {
            BubbleAlign::Start => bubble,
            BubbleAlign::End => bubble.self_end().items_end(),
        }
    }
}

/// `BubbleGroup`: bubbles stacked 4 px apart.
pub fn bubble_group(bubbles: Vec<El>) -> El {
    col().min_w(0.0).gap(4.0).children(bubbles)
}

/// `BubbleReactions`: a small bordered pill (`👍 2`).
pub fn bubble_reactions(t: &Tokens, label: impl Into<String>) -> El {
    let c = &t.colors;
    let e = pad(row(), [3.0, 7.0, 3.0, 7.0])
        .gap(2.0)
        .items_center()
        .radius(PILL)
        .border(1.0, c.border)
        .fill(c.background)
        .child(text(label).font(sans(12.0, 400.0, 16.0)).color(c.foreground));
    shadows(e, &t.shadows.elevation_sm)
}
