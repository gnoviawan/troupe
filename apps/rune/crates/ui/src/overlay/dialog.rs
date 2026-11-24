//! `Dialog` and `AlertDialog` (`components/ui/dialog.tsx`, `alert-dialog.tsx`) and
//! `ConfirmDialog` (`components/ConfirmDialog.tsx`): modal panels centred in the window over a
//! scrim. `ConfirmDialog` has `default` and `danger` variants, never `destructive`; a danger
//! confirm opens on Cancel so a stray Return destroys nothing.

use rune::{El, col, icon, row, text};
use theme::Tokens;

use super::{Close, scrim};
use crate::paint::shadows;
use crate::type_::sans;
use crate::{Button, ButtonSize, ButtonVariant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogKind {
    /// A form or detail: `bg-card`, a ✕ in the corner, scrim at 90 %.
    #[default]
    Dialog,
    /// A confirm inside chat or the phone shell (`alertdialog`): `bg-background`, no ✕, scrim
    /// at 80 %, the title on a 28 px line.
    Alert,
}

/// `Dialog::new("rename", "Rename session").description(..).child(body).footer(buttons)
/// .open(..).on_close(..).build(t, &mut layer)`. The panel is keyed `{key}-dialog`.
pub struct Dialog {
    key: String,
    kind: DialogKind,
    title: String,
    description: Option<String>,
    body: Vec<El>,
    footer: Vec<El>,
    open: bool,
    width: f32,
    close: Option<Close>,
}

impl Dialog {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Dialog {
        Dialog {
            key: key.into(),
            kind: DialogKind::Dialog,
            title: title.into(),
            description: None,
            body: Vec::new(),
            footer: Vec::new(),
            open: false,
            width: 512.0,
            close: None,
        }
    }
    pub fn kind(mut self, kind: DialogKind) -> Self {
        self.kind = kind;
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
    /// A footer button, right-aligned in order (Cancel, then the action).
    pub fn footer(mut self, e: El) -> Self {
        self.footer.push(e);
        self
    }
    /// The widest the panel grows (`max-w-lg`, 512, by default).
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    /// Escape, a press outside and the ✕ call `f`.
    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Close::new(f));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) {
        let (Some(close), true) = (self.close, self.open) else { return };
        let c = &t.colors;
        let alert = self.kind == DialogKind::Alert;
        let (wash, fill, title_line, tracking, gap, role) = if alert {
            (0.8, c.background, 28.0, 0.0, 8.0, "alertdialog")
        } else {
            (0.9, c.card, 18.0, -0.45, 6.0, "dialog")
        };
        let mut header = col()
            .key("header")
            .w_full()
            .gap(gap)
            .child(text(self.title.clone()).key("title").font(sans(18.0, 600.0, title_line).tracking(tracking)).color(c.foreground).wrap());
        if let Some(d) = &self.description {
            header = header.child(text(d.clone()).key("description").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap());
        }
        let mut panel = col()
            .key(format!("{}-dialog", self.key))
            .role(role)
            .label(self.title)
            .description(self.description.unwrap_or_default())
            .w_full()
            .max_w(self.width)
            // A Rune border takes no room: 24 px of padding plus the 1 px border.
            .padding(25.0)
            .gap(16.0)
            .radius(16.0)
            .fill(fill)
            .border(1.0, c.border)
            .child(header)
            .children(self.body);
        if !self.footer.is_empty() {
            panel = panel.child(row().key("footer").w_full().justify_end().gap(8.0).children(self.footer));
        }
        if !alert {
            let x = theme::icons::named("X").expect("bundled icon");
            let button = close.press(
                row()
                    .key("close")
                    .role("button")
                    .label("Close")
                    .size(16.0, 16.0)
                    .radius(8.0)
                    .opacity(0.7)
                    .child(icon(x, 16.0).color(c.foreground)),
            );
            panel = panel.child(row().key("corner").absolute(0.0, 17.0).w_full().pr(17.0).justify_end().child(button));
        }
        let panel = close.modal(shadows(panel, &t.shadows.elevation_lg));
        layer.push(scrim(format!("{}-scrim", self.key), c.overlay.alpha(wash)));
        layer.push(col().key(format!("{}-layer", self.key)).absolute(0.0, 0.0).w_full().h_full().padding(16.0).center().child(panel));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConfirmVariant {
    #[default]
    Default,
    Danger,
}

/// `ConfirmDialog::new("close-all", "Close 3 terminals?", "Running processes stop.")
/// .variant(ConfirmVariant::Danger).confirm_label("Close terminals").open(..)
/// .on_cancel(..).on_confirm(..).build(t, &mut layer)`. The card is keyed `{key}-dialog`.
pub struct ConfirmDialog {
    key: String,
    title: String,
    message: String,
    confirm_label: String,
    cancel_label: String,
    variant: ConfirmVariant,
    loading: bool,
    open: bool,
    cancel: Option<Close>,
    confirm: Option<Close>,
    secondary: Option<(String, Close)>,
}

impl ConfirmDialog {
    pub fn new(key: impl Into<String>, title: impl Into<String>, message: impl Into<String>) -> ConfirmDialog {
        ConfirmDialog {
            key: key.into(),
            title: title.into(),
            message: message.into(),
            confirm_label: "Confirm".into(),
            cancel_label: "Cancel".into(),
            variant: ConfirmVariant::Default,
            loading: false,
            open: false,
            cancel: None,
            confirm: None,
            secondary: None,
        }
    }
    /// A third action between Cancel and the confirm (`secondaryAction`): a ghost button in
    /// destructive ink, such as "Discard".
    pub fn secondary<S: 'static>(mut self, label: impl Into<String>, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.secondary = Some((label.into(), Close::new(f)));
        self
    }
    pub fn variant(mut self, v: ConfirmVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn confirm_label(mut self, l: impl Into<String>) -> Self {
        self.confirm_label = l.into();
        self
    }
    pub fn cancel_label(mut self, l: impl Into<String>) -> Self {
        self.cancel_label = l.into();
        self
    }
    /// The action is running: both buttons are disabled and the confirm reads "Loading...".
    pub fn loading(mut self, l: bool) -> Self {
        self.loading = l;
        self
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    /// Cancel, Escape and a press outside call `f`.
    pub fn on_cancel<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.cancel = Some(Close::new(f));
        self
    }
    pub fn on_confirm<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.confirm = Some(Close::new(f));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) {
        let (Some(cancel), Some(confirm), true) = (self.cancel, self.confirm, self.open) else { return };
        let c = &t.colors;
        let danger = self.variant == ConfirmVariant::Danger;
        let mut lead = row().key("body").w_full().items_start().gap(16.0).padding(24.0);
        if danger {
            let warn = theme::icons::named("AlertTriangle").expect("bundled icon");
            lead = lead.child(
                row()
                    .key("mark")
                    .size(40.0, 40.0)
                    .radius(crate::paint::PILL)
                    .fill(c.destructive.alpha(0.1))
                    .center()
                    .a11y_hidden()
                    .child(icon(warn, 20.0).color(c.destructive)),
            );
        }
        lead = lead.child(
            col()
                .grow(1.0)
                .shrink()
                .gap(4.0)
                .child(text(self.title.clone()).key("title").font(sans(14.0, 600.0, 20.0)).color(c.foreground).wrap())
                .child(text(self.message.clone()).key("message").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap()),
        );
        let disabled = self.loading;
        let mut cancel_button = Button::new(self.cancel_label).variant(ButtonVariant::Ghost).size(ButtonSize::Sm).disabled(disabled).build(t).key("cancel");
        let confirm_variant = if danger { ButtonVariant::Destructive } else { ButtonVariant::Default };
        let label = if self.loading { "Loading...".to_owned() } else { self.confirm_label };
        let mut confirm_button = Button::new(label).variant(confirm_variant).size(ButtonSize::Sm).disabled(disabled).build(t).key("confirm");
        if !disabled {
            cancel_button = cancel.press(cancel_button);
            confirm_button = confirm.press(confirm_button);
            if danger {
                cancel_button = cancel_button.autofocus();
            } else {
                confirm_button = confirm_button.autofocus();
            }
        }
        let footer = row()
            .key("footer")
            .w_full()
            .px(24.0)
            .pt(13.0)
            .pb(12.0)
            .justify_end()
            .gap(8.0)
            .fill(c.secondary.alpha(0.5))
            .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
            .child(cancel_button)
            .children(self.secondary.map(|(label, action)| {
                let button = Button::new(label)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .ink(c.destructive)
                    .disabled(disabled)
                    .build(t)
                    .key("secondary");
                if disabled { button } else { action.press(button) }
            }))
            .child(confirm_button);
        let card = col()
            .key(format!("{}-dialog", self.key))
            .role("alertdialog")
            .label(self.title)
            .description(self.message)
            .w_full()
            .max_w(400.0)
            .padding(1.0)
            .radius(8.0)
            .fill(c.card)
            .border(1.0, c.border)
            .clip_rounded(8.0)
            .child(lead)
            .child(footer);
        let card = cancel.modal(shadows(card, &t.shadows.elevation_2xl));
        layer.push(scrim(format!("{}-scrim", self.key), c.overlay.alpha(0.6)).backdrop_blur(4.0));
        layer.push(col().key(format!("{}-layer", self.key)).absolute(0.0, 0.0).w_full().h_full().padding(16.0).center().child(card));
    }
}
