//! `Switch` (`components/ui/switch.tsx`): a pill track with a round thumb, colours from the
//! per-theme `--switch-*` tokens. `default` is 36x20, `sm` 28x16 for dense rows, `ios` the
//! phone sheet's 51x31 with a 23 px knob.

use rune::{El, el, row};
use theme::Tokens;

use crate::paint::PILL;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchSize {
    #[default]
    Default,
    Sm,
    Ios,
}

impl SwitchSize {
    /// Track width and height, thumb size, and the thumb's left inset off and on.
    fn metrics(self) -> (f32, f32, f32, f32, f32) {
        match self {
            SwitchSize::Default => (36.0, 20.0, 16.0, 2.0, 18.0),
            SwitchSize::Sm => (28.0, 16.0, 12.0, 2.0, 14.0),
            SwitchSize::Ios => (51.0, 31.0, 23.0, 4.0, 24.0),
        }
    }
}

/// A state the gallery pins without a pointer (`switch.states` "forced" row).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchPin {
    #[default]
    None,
    Hover,
    Pressed,
    Focus,
}

pub struct Switch {
    checked: bool,
    size: SwitchSize,
    disabled: bool,
    label: Option<String>,
    pin: SwitchPin,
}

impl Switch {
    pub fn new(checked: bool) -> Switch {
        Switch {
            checked,
            size: SwitchSize::Default,
            disabled: false,
            label: None,
            pin: SwitchPin::None,
        }
    }
    pub fn size(mut self, s: SwitchSize) -> Self {
        self.size = s;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }
    /// Draw the hover, pressed or focus look at rest.
    pub fn pin(mut self, p: SwitchPin) -> Self {
        self.pin = p;
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let (w, h, knob, off, on) = self.size.metrics();
        let (track, hover, active, thumb) = match (self.checked, self.disabled) {
            (true, false) => (c.switch_track_on, c.switch_track_on_hover, c.switch_track_on_active, c.switch_thumb_on),
            (false, false) => (c.switch_track_off, c.switch_track_off_hover, c.switch_track_off_active, c.switch_thumb_off),
            (true, true) => (c.switch_track_on_disabled, c.switch_track_on_disabled, c.switch_track_on_disabled, c.switch_thumb_on_disabled),
            (false, true) => (c.switch_track_off_disabled, c.switch_track_off_disabled, c.switch_track_off_disabled, c.switch_thumb_off_disabled),
        };
        let fill = match self.pin {
            SwitchPin::Hover => hover,
            SwitchPin::Pressed => active,
            _ => track,
        };
        let inset = (h - knob) / 2.0;
        let mut knob_el = el()
            .absolute(if self.checked { on } else { off }, inset)
            .w(knob)
            .h(knob)
            .radius(PILL)
            .fill(thumb);
        if self.size == SwitchSize::Ios {
            knob_el = knob_el.shadow(1.0, 2.0, rune::Color::BLACK.alpha(0.4));
        }
        let mut e = row()
            .role("switch")
            .checked(self.checked)
            .w(w)
            .h(h)
            .radius(PILL)
            .fill(fill)
            .focusable(!self.disabled)
            .disabled(self.disabled)
            .focus_ring(2.0, 0.0, c.focus_ring)
            .child(knob_el);
        if !self.disabled && self.pin == SwitchPin::None {
            e = e.hover(hover).press(active);
        }
        if self.pin == SwitchPin::Focus {
            e = e.box_shadow(rune::Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread: 1.0,
                color: c.foreground,
                inset: false,
            });
        }
        if let Some(l) = self.label {
            e = e.label(l);
        }
        e
    }
}
