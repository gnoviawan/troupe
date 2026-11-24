//! Surfaces anchored to a trigger: `Popover` (`components/ui/popover.tsx`), `HoverCard`
//! (`hover-card.tsx`) and `Tooltip` (`tooltip.tsx`). Each sits `gap` points off its trigger on
//! the side and alignment of its [`Placement`], flips to the other side when it doesn't fit
//! and stays 8 points inside the window (Rune's `El::anchored`).
//!
//! A popover is modal in Rune's sense (it holds focus; Escape and a press outside close it) and
//! joins the back stack. A tooltip and a hover card follow the pointer: they open once it has
//! rested on their trigger for the `--tt-delay` wait, and never join the back stack. Rune has
//! no focus-change handler at this pin, so a keyboard-focused trigger does not open one.

use std::time::Duration;

use rune::{Align, El, Side, col, row, text};
use theme::Tokens;

use super::{Close, popover_surface};
use crate::type_::sans;

/// Where a surface sits relative to its trigger (Radix's `side` and `align`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    Top,
    TopStart,
    TopEnd,
    #[default]
    Bottom,
    BottomStart,
    BottomEnd,
    Left,
    Right,
    RightStart,
}

impl Placement {
    pub(crate) fn split(self) -> (Side, Align) {
        match self {
            Placement::Top => (Side::Top, Align::Center),
            Placement::TopStart => (Side::Top, Align::Start),
            Placement::TopEnd => (Side::Top, Align::End),
            Placement::Bottom => (Side::Bottom, Align::Center),
            Placement::BottomStart => (Side::Bottom, Align::Start),
            Placement::BottomEnd => (Side::Bottom, Align::End),
            Placement::Left => (Side::Left, Align::Center),
            Placement::Right => (Side::Right, Align::Center),
            Placement::RightStart => (Side::Right, Align::Start),
        }
    }
}

/// Anchors `surface` to the element keyed `anchor`. Above or below, a surface longer than the
/// room left is cut to fit (a long list scrolls); beside, it only flips.
pub(crate) fn anchor(surface: El, anchor: &str, placement: Placement, gap: f32) -> El {
    let (side, align) = placement.split();
    let e = if matches!(side, Side::Top | Side::Bottom) {
        surface.anchored_fit(anchor, side, align, gap)
    } else {
        surface.anchored(anchor, side, align, gap)
    };
    e.anchored_round()
}

/// Mounts `surface` at a window point (a context menu's press), below and after it, flipped and
/// kept inside the window like any anchored surface. The point is keyed `{key}-point`.
pub fn at_point(key: &str, p: rune::PointerPress, surface: El, layer: &mut Vec<El>) {
    let point = format!("{key}-point");
    layer.push(rune::el().key(point.clone()).absolute(p.x, p.y).size(0.0, 0.0));
    layer.push(anchor(surface, &point, Placement::BottomStart, 0.0));
}

/// `Popover::new("find").trigger(button).child(body).open(..).on_close(..).build(t, &mut layer)`
/// returns the trigger wrapped in its anchor (`{key}-trigger`); the surface is keyed
/// `{key}-content`.
pub struct Popover {
    key: String,
    trigger: El,
    label: Option<String>,
    body: Vec<El>,
    open: bool,
    placement: Placement,
    width: f32,
    padding: f32,
    close: Option<Close>,
}

impl Popover {
    pub fn new(key: impl Into<String>, trigger: El) -> Popover {
        Popover {
            key: key.into(),
            trigger,
            label: None,
            body: Vec::new(),
            open: false,
            placement: Placement::Bottom,
            width: 288.0,
            padding: 16.0,
            close: None,
        }
    }
    /// The accessible name of the surface.
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }
    pub fn child(mut self, e: El) -> Self {
        self.body.push(e);
        self
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    pub fn placement(mut self, p: Placement) -> Self {
        self.placement = p;
        self
    }
    /// `w-72` (288) by default.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    /// `p-4` (16) by default; 0 lets a search band draw edge to edge.
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }
    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Close::new(f));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) -> El {
        let anchor_key = format!("{}-trigger", self.key);
        let trigger = row().key(anchor_key.clone()).child(self.trigger.expanded(self.open));
        let (Some(close), true) = (self.close, self.open) else { return trigger };
        let mut surface = col()
            .key(format!("{}-content", self.key))
            .role("dialog")
            .w(self.width)
            .padding(self.padding + 1.0)
            .focus_return(&anchor_key)
            .children(self.body);
        if let Some(l) = self.label {
            surface = surface.label(l);
        }
        let surface = close.modal(popover_surface(t, surface, 6.0));
        layer.push(anchor(surface, &anchor_key, self.placement, 4.0));
        trigger
    }
}

/// The trigger of a tooltip or hover card: it reports the pointer resting on it for `enter`.
fn hover_trigger<S: 'static>(trigger: El, enter: Duration, set: impl Fn(&mut S, bool) + 'static) -> El {
    trigger.hover_delays(enter, Duration::ZERO).on_hover_change(move |s: &mut S, inside, _| set(s, inside))
}

/// `Tooltip::new("split", button, "Split terminal").open(s.tip).on_toggle(|s, on| s.tip = on)
/// .build(t, &mut layer)`: 14 px text in a popover pill, 4 points off its trigger, after
/// `--tt-delay`. The tooltip's text describes its trigger while it shows.
pub struct Tooltip {
    key: String,
    trigger: El,
    text: String,
    open: bool,
    placement: Placement,
    toggle: Option<Box<dyn FnOnce(El, Duration) -> El>>,
}

impl Tooltip {
    pub fn new(key: impl Into<String>, trigger: El, text: impl Into<String>) -> Tooltip {
        Tooltip { key: key.into(), trigger, text: text.into(), open: false, placement: Placement::Top, toggle: None }
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    pub fn placement(mut self, p: Placement) -> Self {
        self.placement = p;
        self
    }
    /// Called with true once the pointer has rested on the trigger for `--tt-delay`, and with
    /// false when it leaves.
    pub fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.toggle = Some(Box::new(move |e, enter| hover_trigger(e, enter, f)));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) -> El {
        let anchor_key = format!("{}-trigger", self.key);
        let delay = Duration::from_secs_f32(t.numbers.tt_delay / 1000.0);
        let mut trigger = self.trigger;
        if self.open {
            trigger = trigger.description(self.text.clone());
        }
        if let Some(toggle) = self.toggle {
            trigger = toggle(trigger, delay);
        }
        let trigger = row().key(anchor_key.clone()).child(trigger);
        if !self.open {
            return trigger;
        }
        let c = &t.colors;
        let pill = row()
            .key(format!("{}-tooltip", self.key))
            .role("tooltip")
            .label(self.text.clone())
            .px(13.0)
            .py(7.0)
            .child(text(self.text).font(sans(14.0, 400.0, 20.0)).color(c.popover_foreground).a11y_hidden());
        layer.push(anchor(popover_surface(t, pill, 8.0), &anchor_key, self.placement, 4.0));
        trigger
    }
}

/// `HoverCard::new("branch", link).child(body).open(..).on_toggle(..).build(t, &mut layer)`: a
/// 256 px popover card under its trigger, 16 px in, opening after 120 ms.
pub struct HoverCard {
    key: String,
    trigger: El,
    body: Vec<El>,
    open: bool,
    placement: Placement,
    toggle: Option<Box<dyn FnOnce(El, Duration) -> El>>,
}

impl HoverCard {
    pub fn new(key: impl Into<String>, trigger: El) -> HoverCard {
        HoverCard { key: key.into(), trigger, body: Vec::new(), open: false, placement: Placement::Bottom, toggle: None }
    }
    pub fn child(mut self, e: El) -> Self {
        self.body.push(e);
        self
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    pub fn placement(mut self, p: Placement) -> Self {
        self.placement = p;
        self
    }
    pub fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.toggle = Some(Box::new(move |e, enter| hover_trigger(e, enter, f)));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) -> El {
        let anchor_key = format!("{}-trigger", self.key);
        let mut trigger = self.trigger;
        if let Some(toggle) = self.toggle {
            trigger = toggle(trigger, Duration::from_millis(120));
        }
        let trigger = row().key(anchor_key.clone()).child(trigger);
        if !self.open {
            return trigger;
        }
        let card = col().key(format!("{}-card", self.key)).w(256.0).padding(17.0).gap(2.0).children(self.body);
        layer.push(anchor(popover_surface(t, card, 6.0), &anchor_key, self.placement, 4.0));
        trigger
    }
}
