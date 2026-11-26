//! `DropdownMenu` and `ContextMenu` (`components/ui/dropdown-menu.tsx`, `context-menu.tsx`) on
//! the shared menu styles (`menu-styles.ts`): a 12 px-radius popover with 4 px of padding, 32 px
//! rows at 12/16, a `foreground/6 %` highlight that the pointer and the arrow keys share, group
//! labels, 0.5 px separators, shortcuts at the row end and submenus.
//!
//! Up and Down move between rows and wrap, Home and End jump, typing selects by name, Return
//! and Space pick (Rune's `focus_group` on a `menu`). Escape and a press outside close the top
//! menu only; focus returns to the trigger.

use rune::{El, PointerPress, col, el, icon, row, text};
use theme::Tokens;

use super::floating::{Placement, anchor};
use super::{Close, popover_surface};
use crate::type_::sans;

pub enum MenuEntry {
    Label(String),
    Item(Box<MenuItem>),
    Separator,
}

/// One row. `checked` gives it the 32 px indicator lane (checkbox and radio rows).
pub struct MenuItem {
    label: String,
    icon: Option<&'static rune::Icon>,
    shortcut: Option<String>,
    checked: Option<bool>,
    destructive: bool,
    disabled: bool,
    submenu: Option<Box<Menu>>,
    select: Option<Box<dyn FnOnce(El) -> El>>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> MenuItem {
        MenuItem {
            label: label.into(),
            icon: None,
            shortcut: None,
            checked: None,
            destructive: false,
            disabled: false,
            submenu: None,
            select: None,
        }
    }
    /// A 14 px glyph before the label (`size-3.5 mr-2`).
    pub fn icon(mut self, i: &'static rune::Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    /// A checkbox or radio row: the tick shows when `on`.
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }
    /// Deletes or discards something for good: the last row, after a separator.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    /// Opens `menu` beside this row; this row's `on_select` opens it.
    pub fn submenu(mut self, menu: Menu) -> Self {
        self.submenu = Some(Box::new(menu));
        self
    }
    pub fn on_select<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.select = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }
    /// `on_select` with the frame's `Tx`, for a row that copies or moves focus.
    pub fn on_select_tx<S: 'static>(mut self, f: impl Fn(&mut S, &mut rune::Tx) + 'static) -> Self {
        self.select = Some(Box::new(move |e: El| e.on_press_tx(f)));
        self
    }

    fn build(self, t: &Tokens, key: String, layer: &mut Vec<El>) -> El {
        let c = &t.colors;
        let ink = if self.destructive { c.destructive } else { c.foreground };
        let role = if self.checked.is_some() { "menuitemcheckbox" } else { "menuitem" };
        let lane = self.checked.is_some();
        let mut e = row()
            .key(key.clone())
            .role(role)
            .label(self.label.clone())
            .min_h(32.0)
            .pt(6.0)
            .pb(6.0)
            .pr(8.0)
            .pl(if lane { 32.0 } else { 8.0 })
            .items_center()
            .radius(8.0)
            .focus_highlight(if self.destructive { c.destructive.alpha(0.1) } else { c.foreground.alpha(0.06) })
            .focusable(!self.disabled)
            .disabled(self.disabled);
        if let Some(on) = self.checked {
            e = e.checked(on);
            let tick = theme::icons::named("Check").expect("bundled icon");
            e = e.child(
                el().key("indicator")
                    .absolute(8.0, 0.0)
                    .h_full()
                    .w(14.0)
                    .center()
                    .a11y_hidden()
                    .child(icon(tick, 14.0).color(ink).opacity(if on { 1.0 } else { 0.0 })),
            );
        }
        if let Some(glyph) = self.icon {
            e = e.child(icon(glyph, 14.0).key("icon").mr(8.0).color(ink).a11y_hidden());
        }
        e = e.child(text(self.label).key("label").font(sans(12.0, 400.0, 16.0)).color(ink).truncate().shrink().grow(1.0));
        if let Some(s) = self.shortcut {
            e = e.child(text(s).key("shortcut").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).pl(16.0).a11y_hidden());
        }
        if let Some(sub) = self.submenu {
            let chevron = theme::icons::named("ChevronRight").expect("bundled icon");
            e = e.expanded(sub.open).child(row().key("chevron").pl(16.0).a11y_hidden().child(icon(chevron, 14.0).color(c.muted_foreground)));
            if sub.open {
                e = e.fill(c.foreground.alpha(0.06));
                let mut deeper = Vec::new();
                let surface = sub.surface(t, &mut deeper);
                layer.push(anchor(surface, &key, Placement::RightStart, 4.0));
                layer.extend(deeper);
            }
        }
        if self.disabled {
            return e.opacity(0.5);
        }
        match self.select {
            Some(select) => select(e),
            None => e,
        }
    }
}

/// `Menu::new("view-menu").label("Editor").item(..).separator().open(..).on_close(..)
/// .build(t, &mut layer, button)` returns the trigger in its anchor (`{key}-trigger`); the
/// surface is keyed `{key}-menu`. A context menu opens [`Menu::at`] the pressed point.
pub struct Menu {
    key: String,
    entries: Vec<MenuEntry>,
    open: bool,
    placement: Placement,
    width: Option<f32>,
    at: Option<PointerPress>,
    name: Option<String>,
    close: Option<Close>,
}

impl Menu {
    pub fn new(key: impl Into<String>) -> Menu {
        Menu {
            key: key.into(),
            entries: Vec::new(),
            open: false,
            placement: Placement::BottomStart,
            width: None,
            at: None,
            name: None,
            close: None,
        }
    }
    /// The menu's accessible name: its trigger's text, as `aria-labelledby` gives it.
    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = Some(n.into());
        self
    }
    /// A group label (`label-panel`, 12 px medium muted).
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.entries.push(MenuEntry::Label(l.into()));
        self
    }
    pub fn item(mut self, i: MenuItem) -> Self {
        self.entries.push(MenuEntry::Item(Box::new(i)));
        self
    }
    pub fn separator(mut self) -> Self {
        self.entries.push(MenuEntry::Separator);
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
    /// A fixed width (`w-56`); otherwise the rows set it, 128 at least.
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    /// A context menu: opens at the window point a secondary press (or Shift+F10) gave.
    pub fn at(mut self, p: Option<PointerPress>) -> Self {
        self.open = p.is_some();
        self.at = p;
        self
    }
    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Close::new(f));
        self
    }

    fn surface(self, t: &Tokens, layer: &mut Vec<El>) -> El {
        let c = &t.colors;
        let key = self.key;
        let mut frame = col()
            .key(format!("{key}-menu"))
            .role("menu")
            .focus_group()
            .min_w(128.0)
            .padding(5.0)
            .items_stretch();
        if let Some(w) = self.width {
            frame = frame.w(w);
        }
        if let Some(n) = self.name {
            frame = frame.label(n);
        }
        for (i, entry) in self.entries.into_iter().enumerate() {
            frame = frame.child(match entry {
                MenuEntry::Label(l) => crate::paint::pad(row(), [8.0, 8.0, 4.0, 8.0])
                    .key(format!("label{i}"))
                    .child(text(l).font(sans(12.0, 500.0, 16.8)).color(c.muted_foreground)),
                MenuEntry::Separator => el().key(format!("sep{i}")).h(0.5).mt(4.0).mb(4.0).ml(-4.0).mr(-4.0).fill(c.border).role("separator"),
                MenuEntry::Item(item) => item.build(t, format!("{key}-item{i}"), layer),
            });
        }
        let frame = popover_surface(t, frame, 12.0);
        match self.close {
            Some(close) => close.modal(frame),
            None => frame,
        }
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>, trigger: El) -> El {
        let anchor_key = format!("{}-trigger", self.key);
        let context = self.at.is_some();
        let trigger = row().key(anchor_key.clone()).child(if context { trigger } else { trigger.expanded(self.open) });
        if !self.open {
            return trigger;
        }
        let (anchor_to, gap) = match self.at {
            Some(p) => {
                let point = format!("{}-point", self.key);
                layer.push(el().key(point.clone()).absolute(p.x, p.y).size(0.0, 0.0));
                (point, 0.0)
            }
            None => (anchor_key.clone(), 4.0),
        };
        let placement = if context { Placement::BottomStart } else { self.placement };
        // Submenus anchor to rows inside this surface, so they mount after it.
        let mut subs = Vec::new();
        let surface = self.surface(t, &mut subs).focus_return(&anchor_key);
        layer.push(anchor(surface, &anchor_to, placement, gap));
        layer.extend(subs);
        trigger
    }
}
