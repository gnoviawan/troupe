//! UI zoom at the root: one factor scales every component, as the web client's `uiZoomLevel`
//! scales the page (`types/settings.ts`: 0.5 to 3, steps of 0.1, 1 by default). The view applies
//! it with [`Zoom::apply`] on every run; Rune lays out in `1 / k` as many points and draws at `k`
//! times the scale, so borders and text stay on the device grid.

use rune::Cx;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom(f32);

impl Zoom {
    pub const DEFAULT: Zoom = Zoom(1.0);
    pub const MIN: f32 = 0.5;
    pub const MAX: f32 = 3.0;
    pub const STEP: f32 = 0.1;

    /// `k` clamped to the allowed range; a non-finite `k` is 1.
    pub fn new(k: f32) -> Zoom {
        let k = if k.is_finite() { k } else { 1.0 };
        Zoom(k.clamp(Self::MIN, Self::MAX))
    }

    /// `k` to two decimals, so repeated steps don't drift (0.1 has no exact f32).
    fn stepped(k: f32) -> Zoom {
        Zoom::new((k * 100.0).round() / 100.0)
    }

    pub fn factor(self) -> f32 {
        self.0
    }

    /// One step larger (Ctrl+=), stopping at [`Self::MAX`].
    pub fn zoom_in(self) -> Zoom {
        Zoom::stepped(self.0 + Self::STEP)
    }

    /// One step smaller (Ctrl+-), stopping at [`Self::MIN`].
    pub fn zoom_out(self) -> Zoom {
        Zoom::stepped(self.0 - Self::STEP)
    }

    pub fn apply<S>(self, cx: &mut Cx<S>) {
        cx.zoom(self.0);
    }
}

impl Default for Zoom {
    fn default() -> Zoom {
        Zoom::DEFAULT
    }
}
