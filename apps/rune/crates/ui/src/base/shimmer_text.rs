//! `ShimmerText` (`components/ui/shimmer-text.tsx`): a loading label in `shimmer-base`. The web
//! client sweeps a `shimmer-highlight` band across the glyphs; Rune has no text-clipped gradient,
//! so a highlight copy on top breathes in and out over the same `--shimmer-dur` instead. Reduced
//! motion shows the base colour only, as the CSS does.

use rune::{El, LoopSpec, TextStyle, el, text};
use theme::Tokens;

pub fn shimmer_text(t: &Tokens, s: impl Into<String>, font: TextStyle, still: bool) -> El {
    let s = s.into();
    let base = text(s.clone()).font(font).color(t.colors.shimmer_base);
    if still {
        return base;
    }
    let period = f64::from(t.numbers.shimmer_dur) / 1000.0;
    let glow = text(s)
        .font(font)
        .color(t.colors.shimmer_highlight)
        .absolute(0.0, 0.0)
        .a11y_hidden()
        .looping(LoopSpec::opacity(period, 0.0, 0.0, 1.0));
    el().child(base).child(glow)
}
