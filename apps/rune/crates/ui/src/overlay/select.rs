//! `Select` (`components/ui/select.tsx`): a 40 px field-styled trigger (`combobox`) that opens a
//! `listbox` under it on the menu surface, as wide as the trigger, its picked option ticked in
//! the 32 px lane. Up and Down move, Return picks, Escape closes; the listbox opens on the
//! picked option.

use std::rc::Rc;

use rune::{El, col, el, icon, row, text};
use theme::Tokens;

use super::floating::{Placement, anchor};
use super::{Close, popover_surface};
use crate::type_::sans;

type Pick = Rc<dyn Fn(El, usize) -> El>;

/// `Select::new("branch", &["main", "dev"]).value(Some(0)).label("Branch").width(224.0)
/// .open(..).on_open(..).on_pick(..).on_close(..).build(t, &mut layer)`. The trigger is keyed
/// `{key}-trigger`, the listbox `{key}-listbox`.
pub struct Select {
    key: String,
    options: Vec<String>,
    value: Option<usize>,
    placeholder: String,
    label: String,
    group: Option<String>,
    width: f32,
    disabled: bool,
    open: bool,
    toggle: Option<Box<dyn FnOnce(El) -> El>>,
    pick: Option<Pick>,
    close: Option<Close>,
}

impl Select {
    pub fn new(key: impl Into<String>, options: &[&str]) -> Select {
        Select {
            key: key.into(),
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            value: None,
            placeholder: String::new(),
            label: String::new(),
            group: None,
            width: 224.0,
            disabled: false,
            open: false,
            toggle: None,
            pick: None,
            close: None,
        }
    }
    pub fn value(mut self, v: Option<usize>) -> Self {
        self.value = v;
        self
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    /// The trigger's accessible name (`aria-label`).
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    /// A label over the options (`SelectLabel`).
    pub fn group(mut self, g: impl Into<String>) -> Self {
        self.group = Some(g.into());
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
    /// A press on the trigger.
    pub fn on_open<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }
    /// An option picked, by index. The handler also closes the listbox.
    pub fn on_pick<S: 'static>(mut self, f: impl Fn(&mut S, usize) + Clone + 'static) -> Self {
        self.pick = Some(Rc::new(move |e: El, i| {
            let f = f.clone();
            e.on_press(move |s: &mut S| f(s, i))
        }));
        self
    }
    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Close::new(f));
        self
    }

    pub fn build(self, t: &Tokens, layer: &mut Vec<El>) -> El {
        let c = &t.colors;
        let trigger_key = format!("{}-trigger", self.key);
        let shown = self.value.and_then(|i| self.options.get(i));
        let (value, ink) = match shown {
            Some(v) => (v.clone(), c.foreground),
            None => (self.placeholder.clone(), c.muted_foreground),
        };
        let chevron = theme::icons::named("ChevronDown").expect("bundled icon");
        let mut trigger = row()
            .key(trigger_key.clone())
            .role("combobox")
            .label(self.label.clone())
            .expanded(self.open)
            .w(self.width)
            .h(40.0)
            .pl(13.0)
            .pr(13.0)
            .items_center()
            .justify_between()
            .radius(8.0)
            .fill(c.card)
            .border(1.0, c.border.alpha(0.6))
            .focusable(!self.disabled)
            .child(text(value).key("value").font(sans(14.0, 400.0, 20.0)).color(ink).truncate().shrink())
            .child(icon(chevron, 16.0).color(c.foreground).opacity(0.5).a11y_hidden());
        if self.disabled {
            trigger = trigger.disabled(true).opacity(0.5);
        } else if let Some(toggle) = self.toggle {
            trigger = toggle(trigger);
        }
        let (Some(close), true) = (self.close, self.open && !self.disabled) else { return trigger };
        let tick = theme::icons::named("Check").expect("bundled icon");
        let mut list = col()
            .key(format!("{}-listbox", self.key))
            .role("listbox")
            .label(self.label)
            .focus_group()
            .min_w(self.width + 2.0)
            .padding(5.0)
            .items_stretch();
        if let Some(g) = self.group {
            list = list.child(crate::paint::pad(row(), [8.0, 8.0, 4.0, 32.0]).key("group").child(text(g).font(sans(12.0, 500.0, 16.8)).color(c.muted_foreground)));
        }
        for (i, option) in self.options.into_iter().enumerate() {
            let picked = self.value == Some(i);
            let mut row_el = row()
                .key(format!("option{i}"))
                .role("option")
                .label(option.clone())
                .min_h(32.0)
                .pt(6.0)
                .pb(6.0)
                .pr(8.0)
                .pl(32.0)
                .items_center()
                .radius(8.0)
                .focus_highlight(c.foreground.alpha(0.06))
                .focusable(true)
                .focus_selected(picked)
                .child(
                    el().key("indicator")
                        .absolute(8.0, 0.0)
                        .h_full()
                        .w(14.0)
                        .center()
                        .a11y_hidden()
                        .child(icon(tick, 14.0).color(c.foreground).opacity(if picked { 1.0 } else { 0.0 })),
                )
                .child(text(option).font(sans(12.0, 400.0, 16.0)).color(c.foreground).truncate().shrink());
            if let Some(pick) = &self.pick {
                row_el = pick(row_el, i);
            }
            list = list.child(row_el);
        }
        let list = close.modal(popover_surface(t, list, 12.0)).focus_return(&trigger_key);
        layer.push(anchor(list, &trigger_key, Placement::BottomStart, 4.0));
        trigger
    }
}
