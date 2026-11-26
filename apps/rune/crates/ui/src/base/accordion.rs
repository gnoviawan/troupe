//! `Accordion` (`components/ui/accordion.tsx`): stacked items split by `border` hairlines, each
//! a 56 px trigger row (16 px medium title, a chevron that turns over when open) above its
//! 14 px content. One item opens at a time in the preview (`type="single"`).

use rune::{El, col, icon, row, text};
use theme::Tokens;

use crate::type_::sans;

pub struct AccordionItem {
    title: String,
    body: String,
    open: bool,
}

impl AccordionItem {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> AccordionItem {
        AccordionItem {
            title: title.into(),
            body: body.into(),
            open: false,
        }
    }
    pub fn open(mut self, o: bool) -> Self {
        self.open = o;
        self
    }
}

pub struct Accordion {
    items: Vec<AccordionItem>,
}

impl Accordion {
    pub fn new(items: Vec<AccordionItem>) -> Accordion {
        Accordion { items }
    }

    /// The accordion; item `i`'s trigger is keyed `"{key}/{i}"`, and pressing it calls
    /// `on_toggle(state, i)`.
    pub fn build<S: 'static>(
        self,
        t: &Tokens,
        key: &str,
        on_toggle: impl Fn(&mut S, usize) + Clone + 'static,
    ) -> El {
        let c = &t.colors;
        let chevron = theme::icons::named("ChevronDown").expect("bundled icon");
        let mut root = col().w_full();
        for (i, item) in self.items.into_iter().enumerate() {
            let toggle = on_toggle.clone();
            let trigger = row()
                .key(format!("{key}/{i}"))
                .role("button")
                .label(item.title.clone())
                .expanded(item.open)
                .w_full()
                .py(16.0)
                .items_center()
                .justify_between()
                .focusable(true)
                .focus_ring(2.0, 0.0, c.focus_ring)
                .on_press(move |s: &mut S| toggle(s, i))
                .child(text(item.title).font(sans(16.0, 500.0, 24.0)).color(c.foreground))
                .child(
                    icon(chevron, 16.0)
                        .color(c.foreground)
                        .rotate(if item.open { std::f32::consts::PI } else { 0.0 })
                        .a11y_hidden(),
                );
            let mut section = col()
                .w_full()
                .pb(1.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
                .child(row().role("heading").w_full().child(trigger));
            if item.open {
                section = section.child(
                    col().w_full().pb(16.0).child(
                        text(item.body)
                            .font(sans(14.0, 400.0, 20.0))
                            .color(c.foreground)
                            .wrap(),
                    ),
                );
            }
            root = root.child(section);
        }
        root
    }
}
