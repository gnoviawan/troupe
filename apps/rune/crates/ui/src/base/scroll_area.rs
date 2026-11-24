//! `ScrollArea` (`components/ui/scroll-area.tsx`): a clipped vertical viewport with Radix's thin
//! `muted` pill thumb over its right edge (a 10 px gutter with 1 px padding: an 8 px thumb). The
//! caller bounds the height.

use rune::{El, ScrollSpec, col};
use rune_kit::{ScrollBar, ScrollBarStyle};
use theme::Tokens;

/// `content` in a viewport keyed `key`. The bar keeps its state per call site, as rune-kit's
/// bars do.
#[track_caller]
pub fn scroll_area(t: &Tokens, key: &str, content: El) -> El {
    let viewport = col()
        .key(key.to_owned())
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(content);
    viewport.scroll_bar_styled(ScrollBarStyle::native(t.colors.muted, t.colors.muted).width(8.0).hover_width(8.0))
}
