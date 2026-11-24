//! `Collapsible` (`components/ui/collapsible.tsx`): a trigger and content that shows only while
//! open. Radix adds no look of its own, so the caller draws both; this pairs them and reports the
//! state to assistive technology.

use rune::{El, col};

pub struct Collapsible {
    trigger: El,
    content: El,
    open: bool,
    gap: f32,
}

impl Collapsible {
    pub fn new(trigger: El, content: El, open: bool) -> Collapsible {
        Collapsible {
            trigger,
            content,
            open,
            gap: 0.0,
        }
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }

    pub fn build(self) -> El {
        let mut e = col().gap(self.gap).child(self.trigger.expanded(self.open));
        if self.open {
            e = e.child(self.content);
        }
        e
    }
}
