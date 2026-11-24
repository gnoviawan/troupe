//! Troupe's own modal card (`NewGroupModal`, `NewProjectModal`, `NewWorktreeModal`): a `card`
//! panel 400 px wide at most, an 8 px radius and a hairline border, over a 60 % `overlay`
//! wash. The header row (`px-4 py-3`, `secondary/50`) holds the 14 px semibold title and a
//! 14 px ✕; the body is `p-4`; the footer row repeats the header's fill with its buttons at the
//! end. This is not the shadcn `Dialog`, whose panel has no header or footer bands.

use rune::{El, col, icon, row, text};
use theme::Tokens;

use super::{Close, scrim};
use crate::paint::shadows;
use crate::type_::sans;

/// `Modal::new("new-group", "Create New Group").child(body).footer(buttons).open(..)
/// .on_close(..).build(t, &mut layer)`. The card is keyed `{key}-dialog`.
pub struct Modal {
    key: String,
    title: String,
    body: Vec<El>,
    footer: Vec<El>,
    width: f32,
    open: bool,
    close: Option<Close>,
}

impl Modal {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Modal {
        Modal {
            key: key.into(),
            title: title.into(),
            body: Vec::new(),
            footer: Vec::new(),
            width: 400.0,
            open: false,
            close: None,
        }
    }
    pub fn child(mut self, e: El) -> Self {
        self.body.push(e);
        self
    }
    pub fn footer(mut self, e: El) -> Self {
        self.footer.push(e);
        self
    }
    /// The card's widest (`max-w-lg` is 512).
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Close::new(f));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) {
        let (Some(close), true) = (self.close, self.open) else {
            return;
        };
        let c = &t.colors;
        let x = theme::icons::named("X").expect("bundled icon");
        let close_button = close.press(
            row()
                .key("close")
                .role("button")
                .label("Close")
                .size(14.0, 14.0)
                .child(icon(x, 14.0).color(c.muted_foreground)),
        );
        let band = |key: &str| {
            row()
                .key(key.to_owned())
                .w_full()
                .px(16.0)
                .py(12.0)
                .items_center()
                .fill(c.secondary.alpha(0.5))
        };
        let header = band("header")
            .justify_between()
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
            .child(
                text(self.title.clone())
                    .key("title")
                    .font(sans(14.0, 600.0, 20.0))
                    .color(c.foreground)
                    .role("heading"),
            )
            .child(close_button);
        let mut card = col()
            .key(format!("{}-dialog", self.key))
            .role("dialog")
            .label(self.title)
            .w_full()
            .max_w(self.width)
            .radius(8.0)
            .clip_rounded(8.0)
            .fill(c.card)
            .border(1.0, c.border)
            .child(header)
            .child(
                col()
                    .key("body")
                    .w_full()
                    .padding(16.0)
                    .gap(16.0)
                    .children(self.body),
            );
        if !self.footer.is_empty() {
            card = card.child(
                band("footer")
                    .justify_end()
                    .gap(8.0)
                    .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
                    .children(self.footer),
            );
        }
        let card = close.modal(shadows(card, &t.shadows.elevation_2xl));
        layer.push(scrim(format!("{}-scrim", self.key), c.overlay.alpha(0.6)).backdrop_blur(4.0));
        layer.push(
            col()
                .key(format!("{}-layer", self.key))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .padding(16.0)
                .center()
                .child(card),
        );
    }
}
