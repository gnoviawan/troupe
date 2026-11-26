//! Paint helpers the components share.

use rune::{El, Shadow};

/// Adds a CSS `box-shadow` list. CSS paints the first layer on top; Rune paints each added
/// layer above the ones before it, so the list goes on in reverse.
pub fn shadows(e: El, list: &[Shadow]) -> El {
    list.iter().rev().fold(e, |e, s| e.box_shadow(*s))
}

/// A full pill radius (`rounded-full`).
pub const PILL: f32 = 9999.0;

/// CSS-order padding: top, right, bottom, left.
pub fn pad(e: El, [top, right, bottom, left]: [f32; 4]) -> El {
    e.pt(top).pr(right).pb(bottom).pl(left)
}
