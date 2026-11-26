//! `Label` (`components/ui/label.tsx`): 14 px medium, one line tall. A label for a disabled
//! control draws at 70 % opacity.

use rune::{El, text};
use theme::Tokens;

use crate::type_::sans;

pub fn label(t: &Tokens, s: impl Into<String>, disabled: bool) -> El {
    let e = text(s).font(sans(14.0, 500.0, 14.0)).color(t.colors.foreground);
    if disabled { e.opacity(0.7) } else { e }
}
