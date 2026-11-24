//! `CountBadge` (`components/ui/count-badge.tsx`): the blue count pill. Height, min width and
//! type size stay at the call site; the defaults are the preview's 16 px semibold.

use rune::{El, row, text};
use theme::Tokens;

use crate::paint::PILL;
use crate::type_::sans;

pub struct CountBadge {
    count: u64,
    max: Option<u64>,
    size: f32,
}

impl CountBadge {
    pub fn new(count: u64) -> CountBadge {
        CountBadge {
            count,
            max: None,
            size: 16.0,
        }
    }

    /// Show `{max}+` above `max`.
    pub fn max(mut self, max: u64) -> Self {
        self.max = Some(max);
        self
    }

    /// Font size; the line is one font size tall (`leading-none`).
    pub fn text_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The text the pill shows.
    pub fn text(&self) -> String {
        match self.max {
            Some(max) if self.count > max => format!("{max}+"),
            _ => self.count.to_string(),
        }
    }

    pub fn build(self, t: &Tokens) -> El {
        let label = self.text();
        row()
            .center()
            .px(4.0)
            .radius(PILL)
            .fill(t.colors.primary_fill)
            .child(
                text(label)
                    .font(sans(self.size, 600.0, self.size).features("'cv02' 1, 'cv11' 1, 'ss01' 1, 'ss03' 1, 'tnum' 1"))
                    .color(t.colors.primary_foreground),
            )
    }
}
