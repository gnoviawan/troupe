//! The phone bottom sheet (`components/ui/mobile-sheet.tsx`): a `bg-popover` sheet up to 85 %
//! of the window, a 36 × 5 grab handle that closes it, a 44 px header with Back above the root
//! view, and stacked sub-views. Each sub-view is one more key on [`Overlays`](super::Overlays)
//! above the sheet's own, so Escape, Back and system back pop one level at a time.
//!
//! Dragging the handle or the header moves the sheet with the finger (a fifth of it upward, past
//! the top) and fades the scrim; letting go past min(64 px, 15 % of the sheet), or with a
//! downward flick, closes it; otherwise it springs back. The sheet's travel lives in the app's
//! state as a [`SheetDrag`].

use rune::{Drag, El, Phase, Tx, col, icon, row, text};
use theme::Tokens;

use super::{Close, scrim};
use crate::type_::sans;

/// Pointer travel before a press becomes a drag, so a tap on Back or the handle stays a tap.
pub const DRAG_SLOP: f32 = 4.0;
/// A release this fast downward closes the sheet by itself (0.11 px/ms).
pub const DISMISS_VELOCITY: f32 = 110.0;
/// A move sample older than this at release is a hold, not a flick.
pub const STALE_VELOCITY: f64 = 0.1;
/// An upward pull past the top follows a fifth of the finger.
pub const RUBBER_BAND: f32 = 0.2;

/// Where a drag on the sheet is: its offset from home and the finger's last speed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SheetDrag {
    /// Points below home (negative: pulled up, rubber-banded).
    pub y: f32,
    dragging: bool,
    last_dy: f32,
    last_t: f64,
    /// Downward speed in points per second.
    velocity: f32,
}

/// What a drag step asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetRelease {
    /// Keep tracking (a move), or nothing happened (a tap).
    None,
    /// The sheet springs back home.
    Settle,
    /// The sheet closes.
    Close,
}

impl SheetDrag {
    /// Feeds one phase of the gesture at clock time `now` (seconds); `height` is the sheet's.
    pub fn step(&mut self, phase: Phase, dy: f32, now: f64, height: f32, cancelled: bool) -> SheetRelease {
        match phase {
            Phase::Down => {
                *self = SheetDrag { last_t: now, ..SheetDrag::default() };
                SheetRelease::None
            }
            Phase::Move => {
                if !self.dragging && dy.abs() < DRAG_SLOP {
                    return SheetRelease::None;
                }
                self.dragging = true;
                let dt = (now - self.last_t).max(0.001) as f32;
                self.velocity = (dy - self.last_dy) / dt;
                self.last_dy = dy;
                self.last_t = now;
                self.y = if dy >= 0.0 { dy } else { dy * RUBBER_BAND };
                SheetRelease::None
            }
            Phase::Up => {
                let was = std::mem::take(self);
                if !was.dragging {
                    return SheetRelease::None;
                }
                let velocity = if now - was.last_t > STALE_VELOCITY { 0.0 } else { was.velocity };
                let threshold = 64.0_f32.min(height * 0.15);
                let close = !cancelled && (was.y > threshold || (velocity > DISMISS_VELOCITY && was.y > 0.0));
                if close { SheetRelease::Close } else { SheetRelease::Settle }
            }
        }
    }
}

/// `MobileSheet::new("model", "Model").depth(n).child(rows).open(..).drag(s.drag)
/// .on_drag(..).on_back(..).on_close(..).build(t, (w, h), &mut layer)`. The sheet is keyed
/// `{key}-sheet`. `title` is the top view's.
pub struct MobileSheet {
    key: String,
    title: String,
    depth: usize,
    body: Vec<El>,
    open: bool,
    drag: SheetDrag,
    close: Option<Close>,
    back: Option<Box<dyn Fn(El) -> El>>,
    on_drag: Option<Box<dyn FnOnce(El, f32) -> El>>,
}

impl MobileSheet {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> MobileSheet {
        MobileSheet {
            key: key.into(),
            title: title.into(),
            depth: 0,
            body: Vec::new(),
            open: false,
            drag: SheetDrag::default(),
            close: None,
            back: None,
            on_drag: None,
        }
    }
    /// How many sub-views are pushed above the root (Back shows above 0).
    pub fn depth(mut self, d: usize) -> Self {
        self.depth = d;
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
    /// The drag in progress, from the app's state.
    pub fn drag(mut self, d: SheetDrag) -> Self {
        self.drag = d;
        self
    }
    /// Escape, the scrim and the handle: pops the top view (or closes the sheet at the root).
    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Close::new(f));
        self
    }
    /// The header's Back button.
    pub fn on_back<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.back = Some(Box::new(move |e: El| e.on_press(f.clone())));
        self
    }
    /// The app's drag state and what to do on release: `get` reads the [`SheetDrag`] out of the
    /// state, `close` closes the whole sheet.
    pub fn on_drag<S: 'static>(mut self, get: fn(&mut S) -> &mut SheetDrag, close: impl Fn(&mut S) + 'static) -> Self {
        self.on_drag = Some(Box::new(move |e: El, height: f32| {
            e.on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
                let now = tx.clock();
                match get(s).step(d.phase, d.dy, now, height, d.cancelled()) {
                    SheetRelease::Close => close(s),
                    SheetRelease::Settle => tx.animated(),
                    SheetRelease::None => {}
                }
            })
        }));
        self
    }

    /// `window` is the window's size in points: the sheet is at most 85 % of its height.
    pub fn build(self, t: &Tokens, window: (f32, f32), layer: &mut Vec<El>) {
        let (Some(close), true) = (self.close, self.open) else { return };
        let c = &t.colors;
        let max_h = window.1 * 0.85;
        let handle = close.press(
            row()
                .key("handle")
                .role("button")
                .label("Close")
                .w(36.0)
                .h(5.0)
                .radius(crate::paint::PILL)
                .fill(c.muted),
        );
        let handle = crate::paint::shadows(handle, &t.shadows.shadow_control);
        let mut back = row().key("back-slot").w(44.0).h(44.0);
        if self.depth > 0 {
            let arrow = theme::icons::named("ArrowLeft").expect("bundled icon");
            let button = row()
                .key("back")
                .role("button")
                .label("Back")
                .size(44.0, 44.0)
                .radius(crate::paint::PILL)
                .center()
                .hover(c.foreground.alpha(0.06))
                .child(icon(arrow, 18.0).color(c.foreground));
            back = match &self.back {
                Some(f) => f(button),
                None => button,
            };
        }
        let header = col()
            .key("grab")
            .w_full()
            .child(row().key("handle-row").w_full().h(28.0).center().child(handle))
            .child(
                row()
                    .key("header")
                    .w_full()
                    .h(44.0)
                    .px(8.0)
                    .items_center()
                    .child(back)
                    .child(row().grow(1.0).shrink().justify_center().child(text(self.title.clone()).key("title").font(sans(16.0, 500.0, 24.0)).color(c.foreground).truncate()))
                    .child(row().w(44.0).h(44.0)),
            );
        let header = match self.on_drag {
            Some(f) => f(header, max_h),
            None => header,
        };
        let body = col()
            .key("body")
            .w_full()
            .shrink()
            .min_h(0.0)
            .px(12.0)
            .pt(4.0)
            .pb(12.0)
            .gap(8.0)
            .items_stretch()
            .scroll(rune::ScrollSpec { stick_bottom: false })
            .children(self.body);
        let sheet = col()
            .key(format!("{}-sheet", self.key))
            .role("dialog")
            .label(self.title)
            .w_full()
            .max_h(max_h)
            .corners([14.0, 14.0, 0.0, 0.0])
            .fill(c.popover)
            .border_sides([0.5, 0.0, 0.0, 0.0], c.border)
            .shadow(-4.0, 16.0, rune::Color::BLACK.alpha(0.4))
            .offset(0.0, self.drag.y)
            .child(header)
            .child(body);
        let sheet = close.modal(sheet);
        let fade = 1.0 - (self.drag.y / max_h.max(1.0)).clamp(0.0, 1.0);
        layer.push(scrim(format!("{}-scrim", self.key), c.overlay.alpha(0.8)).opacity(fade));
        layer.push(col().key(format!("{}-layer", self.key)).absolute(0.0, 0.0).w_full().h_full().justify_end().child(sheet));
    }
}
