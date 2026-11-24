//! `IconSwap` (`components/ui/icon-swap.tsx`): a 14 px slot that cross-fades between glyphs
//! keyed by state, scaling from .96. Reduced motion replaces the glyph at once.

use rune::{El, Icon, Transition, icon, row};

/// The glyph for the current state. `key` changes with the state, so Rune's presence runs the
/// old glyph's exit and the new one's entrance.
pub fn icon_swap(glyph: &'static Icon, key: &str, size: f32, ink: rune::Color) -> El {
    row()
        .w(14.0)
        .h(14.0)
        .center()
        .child(
            icon(glyph, size)
                .color(ink)
                .key(format!("icon-swap/{key}"))
                .transition(Transition::scale(0.96)),
        )
}
