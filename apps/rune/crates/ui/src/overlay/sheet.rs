//! `Sheet` (`components/ui/sheet.tsx`): a slide-over pinned to one window edge over an 80 %
//! scrim, `bg-background`, 24 px in, a ✕ in the top corner. Left and right sheets are three
//! quarters of the window wide up to 384 px; top and bottom sheets span the window, and a
//! bottom sheet rounds its top corners. The phone's drag-to-close sheet is
//! `MobileSheet`; this is the shell it is built on.

use rune::{El, col, icon, row, text};
use theme::Tokens;

use super::{Close, scrim};
use crate::paint::shadows;
use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetSide {
    Top,
    #[default]
    Right,
    Bottom,
    Left,
}

impl SheetSide {
    pub const ALL: [SheetSide; 4] = [SheetSide::Top, SheetSide::Right, SheetSide::Bottom, SheetSide::Left];

    pub fn name(self) -> &'static str {
        match self {
            SheetSide::Top => "top",
            SheetSide::Right => "right",
            SheetSide::Bottom => "bottom",
            SheetSide::Left => "left",
        }
    }
}

/// `Sheet::new("details", "Session details").side(SheetSide::Right).child(body).footer(button)
/// .open(..).on_close(..).build(t, &mut layer)`. The panel is keyed `{key}-dialog`.
pub struct Sheet {
    key: String,
    side: SheetSide,
    title: String,
    description: Option<String>,
    body: Vec<El>,
    footer: Vec<El>,
    open: bool,
    close: Option<Close>,
}

impl Sheet {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Sheet {
        Sheet {
            key: key.into(),
            side: SheetSide::Right,
            title: title.into(),
            description: None,
            body: Vec::new(),
            footer: Vec::new(),
            open: false,
            close: None,
        }
    }
    pub fn side(mut self, s: SheetSide) -> Self {
        self.side = s;
        self
    }
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }
    pub fn child(mut self, e: El) -> Self {
        self.body.push(e);
        self
    }
    pub fn footer(mut self, e: El) -> Self {
        self.footer.push(e);
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
        let (Some(close), true) = (self.close, self.open) else { return };
        let c = &t.colors;
        let mut header = col()
            .key("header")
            .w_full()
            .gap(8.0)
            .child(text(self.title.clone()).key("title").font(sans(18.0, 600.0, 28.0)).color(c.foreground).wrap());
        if let Some(d) = &self.description {
            header = header.child(text(d.clone()).key("description").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap());
        }
        // The edge it opens from has the border; 24 px in, plus the border's pixel on that side.
        let (border, pad) = match self.side {
            SheetSide::Top => ([0.0, 0.0, 1.0, 0.0], [24.0, 24.0, 25.0, 24.0]),
            SheetSide::Right => ([0.0, 0.0, 0.0, 1.0], [24.0, 24.0, 24.0, 25.0]),
            SheetSide::Bottom => ([1.0, 0.0, 0.0, 0.0], [25.0, 24.0, 24.0, 24.0]),
            SheetSide::Left => ([0.0, 1.0, 0.0, 0.0], [24.0, 25.0, 24.0, 24.0]),
        };
        let mut panel = crate::paint::pad(col(), pad)
            .key(format!("{}-dialog", self.key))
            .role("dialog")
            .label(self.title)
            .description(self.description.unwrap_or_default())
            .gap(16.0)
            .fill(c.background)
            .border_sides(border, c.border)
            .child(header)
            .children(self.body);
        if !self.footer.is_empty() {
            panel = panel.child(row().key("footer").w_full().justify_end().gap(8.0).children(self.footer));
        }
        let x = theme::icons::named("X").expect("bundled icon");
        let button = close.press(
            row()
                .key("close")
                .role("button")
                .label("Close")
                .size(28.0, 28.0)
                .radius(4.0)
                .opacity(0.7)
                .center()
                .child(icon(x, 16.0).color(c.foreground)),
        );
        panel = panel.child(row().key("corner").absolute(0.0, 10.0).w_full().pr(10.0).justify_end().child(button));
        let panel = match self.side {
            SheetSide::Left | SheetSide::Right => panel.h_full().w_frac(0.75).max_w(384.0),
            SheetSide::Top => panel.w_full(),
            SheetSide::Bottom => panel.w_full().corners([12.0, 12.0, 0.0, 0.0]),
        };
        let panel = close.modal(shadows(panel, &t.shadows.elevation_lg));
        let place = col().key(format!("{}-layer", self.key)).absolute(0.0, 0.0).w_full().h_full();
        let place = match self.side {
            SheetSide::Top => place.justify_start(),
            SheetSide::Bottom => place.justify_end(),
            SheetSide::Left => row().key(format!("{}-layer", self.key)).absolute(0.0, 0.0).w_full().h_full().justify_start(),
            SheetSide::Right => row().key(format!("{}-layer", self.key)).absolute(0.0, 0.0).w_full().h_full().justify_end(),
        };
        layer.push(scrim(format!("{}-scrim", self.key), c.overlay.alpha(0.8)));
        layer.push(place.child(panel));
    }
}
