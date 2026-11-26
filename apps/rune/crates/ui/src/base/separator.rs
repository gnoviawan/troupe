//! `Separator` (`components/ui/separator.tsx`): a 1 px `border` line, decorative by default.

use rune::{El, el};
use theme::Tokens;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

/// A full-width (or full-height) hairline. A vertical one needs a height from its caller.
pub fn separator(t: &Tokens, orientation: Orientation) -> El {
    let e = el().fill(t.colors.border).a11y_hidden();
    match orientation {
        Orientation::Horizontal => e.h(1.0).w_full(),
        Orientation::Vertical => e.w(1.0).h_full(),
    }
}
