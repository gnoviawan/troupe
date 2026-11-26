//! The command palette shell (`components/ui/command.tsx`, cmdk): a search band over grouped
//! rows on `bg-popover`, inline or in a dialog (`CommandDialog`), where the rows grow to 44 px
//! with 20 px glyphs. Filtering is the caller's: it passes the rows that match. The search
//! field is the caller's `text_input`, as every field is.

use rune::{El, TextEdit, col, el, icon, row, text, text_input};
use theme::Tokens;

use super::{Close, scrim};
use crate::kbd;
use crate::paint::shadows;
use crate::type_::sans;

pub struct CommandItem {
    label: String,
    icon: Option<&'static rune::Icon>,
    shortcut: Option<String>,
    select: Option<Box<dyn FnOnce(El) -> El>>,
}

impl CommandItem {
    pub fn new(label: impl Into<String>) -> CommandItem {
        CommandItem { label: label.into(), icon: None, shortcut: None, select: None }
    }
    pub fn icon(mut self, i: &'static rune::Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    pub fn on_select<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.select = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }
}

/// `Command::new("palette", &edit).placeholder("Type a command").group("Terminal", items)
/// .build(t)` for the inline palette; `.open(..).on_close(..).dialog(t, &mut layer)` for the
/// dialog, keyed `{key}-dialog`. The search field is keyed `{key}-input`: give it `on_edit`
/// with [`Command::input`]'s key.
pub struct Command<'a> {
    key: String,
    edit: &'a TextEdit,
    placeholder: String,
    groups: Vec<(String, Vec<CommandItem>)>,
    empty: String,
    max_height: Option<f32>,
    edit_handler: Option<Box<dyn FnOnce(El) -> El + 'a>>,
    open: bool,
    close: Option<Close>,
}

impl<'a> Command<'a> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit) -> Command<'a> {
        Command {
            key: key.into(),
            edit,
            placeholder: String::new(),
            groups: Vec::new(),
            empty: "No results found.".into(),
            max_height: Some(300.0),
            edit_handler: None,
            open: false,
            close: None,
        }
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    /// A heading and its rows; groups after the first get a separator.
    pub fn group(mut self, heading: impl Into<String>, items: Vec<CommandItem>) -> Self {
        self.groups.push((heading.into(), items));
        self
    }
    /// What shows when no group has rows (`CommandEmpty`).
    pub fn empty(mut self, e: impl Into<String>) -> Self {
        self.empty = e.into();
        self
    }
    /// The list's tallest (300 by default); None lets it grow.
    pub fn max_height(mut self, h: Option<f32>) -> Self {
        self.max_height = h;
        self
    }
    /// The search field's edit handler (`El::on_edit`), applied to the field.
    pub fn on_edit(mut self, f: impl FnOnce(El) -> El + 'a) -> Self {
        self.edit_handler = Some(Box::new(f));
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

    fn shell(self, t: &Tokens, large: bool) -> El {
        let c = &t.colors;
        let (band, glyph, row_pad) = if large { (48.0, 20.0, 12.0) } else { (44.0, 16.0, 6.0) };
        let search = theme::icons::named("Search").expect("bundled icon");
        let mut field = text_input(self.edit, sans(14.0, 400.0, 20.0))
            .key(format!("{}-input", self.key))
            .role("combobox")
            .label(if self.placeholder.is_empty() { "Search".to_owned() } else { self.placeholder.clone() })
            .placeholder(self.placeholder.clone(), c.muted_foreground)
            .color(c.foreground)
            .single_line()
            .grow(1.0)
            .no_focus_ring();
        if let Some(h) = self.edit_handler {
            field = h(field);
        }
        let band = row()
            .key("band")
            .w_full()
            .h(band + 1.0)
            .px(12.0)
            .pb(1.0)
            .items_center()
            .gap(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
            .child(icon(search, glyph).color(c.foreground).opacity(0.5).a11y_hidden())
            .child(field);
        let mut list = col().key("list").role("listbox").label("Suggestions").focus_group().w_full().items_stretch();
        let mut any = false;
        for (g, (heading, items)) in self.groups.into_iter().enumerate() {
            if items.is_empty() {
                continue;
            }
            if any {
                list = list.child(el().key(format!("sep{g}")).h(0.5).w_full().fill(c.border).a11y_hidden());
            }
            any = true;
            let mut group = col()
                .key(format!("group{g}"))
                .role("group")
                .label(heading.clone())
                .px(if large { 8.0 } else { 4.0 })
                .py(4.0)
                .items_stretch()
                .child(row().key("heading").px(8.0).py(6.0).child(text(heading).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground)));
            for (i, item) in items.into_iter().enumerate() {
                let mut e = row()
                    .key(format!("item{g}-{i}"))
                    .role("option")
                    .label(item.label.clone())
                    .px(8.0)
                    .py(row_pad)
                    .items_center()
                    .radius(4.0)
                    .focus_highlight(c.foreground.alpha(0.06))
                    .focusable(true);
                if let Some(glyph_icon) = item.icon {
                    e = e.child(icon(glyph_icon, glyph).color(c.foreground).a11y_hidden());
                }
                e = e.child(text(item.label).font(sans(14.0, 400.0, 20.0)).color(c.foreground).truncate().shrink().grow(1.0));
                if let Some(s) = item.shortcut {
                    e = e.child(row().a11y_hidden().child(kbd(t, s)));
                }
                if let Some(select) = item.select {
                    e = select(e);
                }
                group = group.child(e);
            }
            list = list.child(group);
        }
        if !any {
            list = list.child(row().key("empty").w_full().py(24.0).center().child(text(self.empty).font(sans(14.0, 400.0, 20.0)).color(c.foreground)));
        }
        let mut scroller = col().key("scroll").w_full().shrink().min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(list);
        if let Some(h) = self.max_height {
            scroller = scroller.max_h(h);
        }
        col().key(format!("{}-command", self.key)).w_full().fill(c.popover).radius(6.0).clip_rounded(6.0).child(band).child(scroller)
    }

    /// The inline palette, inside a 12 px-radius border as the gallery shows it.
    pub fn build(self, t: &Tokens) -> El {
        let border = t.colors.border;
        let key = self.key.clone();
        row().key(format!("{key}-frame")).w_full().radius(12.0).border(1.0, border).padding(1.0).clip_rounded(12.0).child(self.shell(t, false))
    }

    /// The palette in a dialog (`CommandDialog`): no padding, a 512 px card over a 90 % scrim.
    pub fn dialog(mut self, t: &Tokens, layer: &mut Vec<El>) {
        let (Some(close), true) = (self.close.take(), self.open) else { return };
        let c = &t.colors;
        let key = self.key.clone();
        self.max_height = Some(300.0);
        let card = col()
            .key(format!("{key}-dialog"))
            .role("dialog")
            .label("Command palette")
            .w_full()
            .max_w(512.0)
            .padding(1.0)
            .radius(16.0)
            .fill(c.card)
            .border(1.0, c.border)
            .clip_rounded(16.0)
            .child(self.shell(t, true));
        let card = close.modal(shadows(card, &t.shadows.elevation_lg));
        layer.push(scrim(format!("{key}-scrim"), c.overlay.alpha(0.9)));
        layer.push(col().key(format!("{key}-layer")).absolute(0.0, 0.0).w_full().h_full().padding(16.0).center().child(card));
    }
}
