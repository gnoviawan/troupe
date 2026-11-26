//! `Kbd` (`components/ui/kbd.tsx`): a hairline key chip. The preview pads it `px-1.5 py-0.5`.

use rune::{El, row, text};
use theme::Tokens;

use crate::paint::pad;
use crate::type_::{MICRO, sans};

/// A key chip reading `keys` ("Ctrl+K", "Esc").
pub fn kbd(t: &Tokens, keys: impl Into<String>) -> El {
    // 1 px border plus `px-1.5 py-0.5`.
    pad(row(), [3.0, 7.0, 3.0, 7.0])
        .radius(4.0)
        .border(1.0, t.colors.border)
        .child(
            text(keys)
                .font(sans(MICRO, 500.0, 18.0))
                .color(t.colors.muted_foreground),
        )
}
