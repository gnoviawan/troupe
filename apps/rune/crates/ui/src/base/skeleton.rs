//! `Skeleton` (`components/ui/skeleton.tsx`): a `skeleton` block pulsing between full and half
//! opacity every 2 s (`animate-pulse`). Size stays at the call site.

use rune::{El, LoopSpec, el};
use theme::Tokens;

pub fn skeleton(t: &Tokens) -> El {
    el().radius(6.0)
        .fill(t.colors.skeleton)
        .a11y_hidden()
        .looping(LoopSpec::opacity(2.0, 0.0, 0.5, 1.0))
}
