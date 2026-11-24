//! Toasts (`components/ui/sonner.tsx`): a bottom-right stack, 20 px from the window edges, at
//! most four on screen, newest at the bottom, each 356 px wide on `bg-card` with a close
//! button, dismissed after four seconds. The phone shell keeps the stack expanded, as here;
//! the desktop's collapsed hover pile is not drawn yet.

use rune::{El, col, icon, row, text};
use theme::Tokens;

use crate::paint::shadows;
use crate::type_::sans;
use crate::{Button, ButtonSize, ButtonVariant};

/// How long a toast stays, in seconds (`duration={4000}`).
pub const DURATION: f64 = 4.0;
/// Toasts on screen at once (`visibleToasts={4}`); the rest wait their turn.
pub const VISIBLE: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastKind {
    #[default]
    Message,
    Success,
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub description: Option<String>,
    /// An action button's label (`action: { label }`); the app handles its press.
    pub action: Option<String>,
    /// When it leaves, in the app's clock seconds; None while it waits off screen. Its clock
    /// starts once it shows.
    pub until: Option<f64>,
}

/// The toasts raised and not yet gone, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toasts {
    list: Vec<Toast>,
    next: u64,
}

impl Toasts {
    /// Raises a toast at clock time `now`; returns its id.
    pub fn push(&mut self, now: f64, kind: ToastKind, title: impl Into<String>, description: Option<&str>) -> u64 {
        self.next += 1;
        self.list.push(Toast {
            id: self.next,
            kind,
            title: title.into(),
            description: description.map(str::to_owned),
            action: None,
            until: None,
        });
        self.settle(now);
        self.next
    }

    /// Stops the clocks of toasts off screen and starts those of toasts just shown.
    fn settle(&mut self, now: f64) {
        let waiting = self.list.len().saturating_sub(VISIBLE);
        for (i, t) in self.list.iter_mut().enumerate() {
            if i < waiting {
                t.until = None;
            } else if t.until.is_none() {
                t.until = Some(now + DURATION);
            }
        }
    }

    /// Gives toast `id` an action button.
    pub fn action(&mut self, id: u64, label: impl Into<String>) {
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            t.action = Some(label.into());
        }
    }

    /// Closes toast `id`. A toast that moves on screen in its place starts its clock at the
    /// next [`Toasts::expire`].
    pub fn dismiss(&mut self, id: u64) {
        self.list.retain(|t| t.id != id);
        let waiting = self.list.len().saturating_sub(VISIBLE);
        for t in &mut self.list[..waiting] {
            t.until = None;
        }
    }

    /// Drops the toasts whose time is up; true when any left. A waiting toast's clock starts
    /// only once it is on screen.
    pub fn expire(&mut self, now: f64) -> bool {
        self.settle(now);
        let before = self.list.len();
        self.list.retain(|t| t.until.is_none_or(|until| until > now));
        self.settle(now);
        self.list.len() != before
    }

    /// The toasts on screen, oldest first.
    pub fn visible(&self) -> &[Toast] {
        &self.list[self.list.len().saturating_sub(VISIBLE)..]
    }

    /// The soonest a shown toast leaves, for `Tx::wake_at`.
    pub fn next_expiry(&self) -> Option<f64> {
        self.visible().iter().filter_map(|t| t.until).min_by(f64::total_cmp)
    }

    /// The region keyed `toaster`, bottom-right; push it into the overlay layer. `close` builds
    /// each close button's handler from the toast's id.
    pub fn build<S: 'static>(&self, t: &Tokens, close: impl Fn(u64) -> Box<dyn Fn(&mut S)>) -> El {
        let c = &t.colors;
        let mut region = col().key("toaster").role("region").label("Notifications").w(356.0).gap(14.0);
        for toast in self.visible() {
            let glyph = match toast.kind {
                ToastKind::Message => None,
                ToastKind::Success => Some(("CircleCheck", c.foreground)),
                ToastKind::Error => Some(("XCircle", c.foreground)),
                ToastKind::Warning => Some(("AlertTriangle", c.foreground)),
                ToastKind::Info => Some(("Info", c.foreground)),
            };
            let mut body = col().grow(1.0).shrink().gap(2.0).child(text(toast.title.clone()).font(sans(13.0, 500.0, 19.5)).color(c.foreground).wrap());
            if let Some(d) = &toast.description {
                body = body.child(text(d.clone()).font(sans(13.0, 400.0, 19.5)).color(c.muted_foreground).wrap());
            }
            let mut card = row()
                .key(format!("toast{}", toast.id))
                .role("status")
                .label(toast.title.clone())
                .w_full()
                .padding(17.0)
                .gap(6.0)
                .items_center()
                .radius(8.0)
                .fill(c.card)
                .border(1.0, c.border);
            if let Some((name, ink)) = glyph {
                card = card.child(icon(theme::icons::named(name).expect("bundled icon"), 16.0).color(ink).a11y_hidden());
            }
            card = card.child(body);
            if let Some(label) = &toast.action {
                card = card.child(Button::new(label.clone()).variant(ButtonVariant::Default).size(ButtonSize::Xs).build(t).key("action"));
            }
            let x = theme::icons::named("X").expect("bundled icon");
            let close_button = row()
                .key("close")
                .role("button")
                .label("Close toast")
                .size(20.0, 20.0)
                .radius(crate::paint::PILL)
                .fill(c.card)
                .border(1.0, c.border)
                .center()
                .child(icon(x, 12.0).color(c.foreground))
                .focusable(true)
                .on_press(close(toast.id));
            card = card.child(row().key("corner").absolute(-7.0, -7.0).child(close_button));
            region = region.child(shadows(card, &t.shadows.elevation_lg));
        }
        col().key("toaster-layer").absolute(0.0, 0.0).w_full().h_full().justify_end().items_end().padding(20.0).child(region)
    }
}
