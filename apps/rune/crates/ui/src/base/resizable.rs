//! `ResizableHandle` (`components/ui/resizable.tsx`): the 1 px `border` line between two panels,
//! with an optional grip. Panel sizes are the caller's state; the handle reports drags through
//! the `on_drag` the caller adds.

use rune::{El, el, icon, row};
use theme::Tokens;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizableDirection {
    #[default]
    Horizontal,
    Vertical,
}

/// The divider between panels laid out in `direction`; `with_handle` adds the grip.
pub fn resizable_handle(t: &Tokens, direction: ResizableDirection, with_handle: bool) -> El {
    let c = &t.colors;
    let line = el().fill(c.border).role("separator").focusable(true);
    let line = match direction {
        ResizableDirection::Horizontal => line.w(1.0).h_full(),
        ResizableDirection::Vertical => line.h(1.0).w_full(),
    };
    if !with_handle {
        return line;
    }
    let grip = theme::icons::named("GripVertical").expect("bundled icon");
    line.center().child(
        row()
            .w(12.0)
            .h(16.0)
            .center()
            .radius(2.0)
            .border(1.0, c.border)
            .fill(c.border)
            .child(icon(grip, 10.0).color(c.foreground)),
    )
}
