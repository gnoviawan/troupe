//! `MarqueeText` (`components/ui/marquee-text.tsx`): a row label that clips to its width. When
//! the text overflows it scrolls on a slow loop so the tail stays readable; a fitting label, and
//! any label under reduced motion, is a plain truncated line.

use rune::{El, LoopSpec, TextStyle, el, text};
use theme::Tokens;

/// Slowest and fastest loops, and the extra time per point of overflow (as the web client).
const MIN_S: f64 = 6.0;
const MAX_S: f64 = 16.0;
const S_PER_PT: f64 = 0.030;

/// `content` is the text's natural width and `room` the box's, in points.
pub fn marquee_text(
    t: &Tokens,
    s: impl Into<String>,
    font: TextStyle,
    content: f32,
    room: f32,
    still: bool,
) -> El {
    let overflow = content - room;
    let line = text(s).font(font).color(t.colors.foreground).single_line();
    if overflow <= 0.0 || still {
        return line.truncate().min_w(0.0).grow(1.0);
    }
    let period = (MIN_S + f64::from(overflow) * S_PER_PT).min(MAX_S);
    el().min_w(0.0)
        .grow(1.0)
        .clip_soft(0.0)
        .child(line.looping(LoopSpec::slide(period, [0.0, 0.0], [-overflow / content, 0.0])))
}
