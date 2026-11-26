//! `Message` and its parts (`components/ui/message.tsx`): a row with an avatar slot and a
//! content column holding a header, the bubble and a footer. `end` mirrors the row for the
//! user's own messages.

use rune::{El, col, row, text};
use theme::Tokens;

use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageAlign {
    #[default]
    Start,
    End,
}

pub struct Message {
    align: MessageAlign,
    avatar: Option<El>,
    header: Option<String>,
    body: Vec<El>,
    footer: Vec<El>,
}

impl Message {
    pub fn new(align: MessageAlign) -> Message {
        Message {
            align,
            avatar: None,
            header: None,
            body: Vec::new(),
            footer: Vec::new(),
        }
    }
    /// The 32 px avatar slot's content.
    pub fn avatar(mut self, a: El) -> Self {
        self.avatar = Some(a);
        self
    }
    pub fn header(mut self, h: impl Into<String>) -> Self {
        self.header = Some(h.into());
        self
    }
    pub fn body(mut self, b: El) -> Self {
        self.body.push(b);
        self
    }
    pub fn footer(mut self, f: El) -> Self {
        self.footer.push(f);
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let c = &t.colors;
        let end = self.align == MessageAlign::End;
        let mut content = col().grow(1.0).min_w(0.0).gap(4.0);
        if end {
            content = content.items_end();
        }
        if let Some(h) = self.header {
            content = content.child(
                text(h)
                    .font(sans(12.0, 500.0, 16.0))
                    .color(c.muted_foreground)
                    .self_start(),
            );
        }
        content = content.children(self.body);
        if !self.footer.is_empty() {
            let mut footer = self.footer;
            if end {
                footer.reverse();
            }
            content = content.child(row().items_center().gap(4.0).children(footer));
        }
        let avatar = row()
            .w(32.0)
            .self_end()
            .justify_center()
            .children(self.avatar);
        let mut e = row().role("group").w_full().items_end().gap(8.0);
        e = if end {
            e.child(content).child(avatar)
        } else {
            e.child(avatar).child(content)
        };
        e
    }
}
