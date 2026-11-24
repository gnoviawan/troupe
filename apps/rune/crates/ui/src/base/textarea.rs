//! `Textarea` (`components/ui/textarea.tsx`): the `Input` surface over several lines, 80 px
//! tall at least, with a 6 px radius.

use rune::{El, TextEdit};
use theme::Tokens;

use super::input::field;

pub struct Textarea<'a> {
    edit: &'a TextEdit,
    placeholder: String,
    rows: u32,
    disabled: bool,
    coarse: bool,
    label: Option<String>,
}

impl<'a> Textarea<'a> {
    pub fn new(edit: &'a TextEdit) -> Textarea<'a> {
        Textarea {
            edit,
            placeholder: String::new(),
            rows: 2,
            disabled: false,
            coarse: false,
            label: None,
        }
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    /// The visible line count (`rows`); the box never drops below 80 px.
    pub fn rows(mut self, rows: u32) -> Self {
        self.rows = rows;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn coarse(mut self, c: bool) -> Self {
        self.coarse = c;
        self
    }
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let line = if self.coarse { 24.0 } else { 20.0 };
        let h = (self.rows as f32 * line + 18.0).max(80.0);
        field(t, self.edit, &self.placeholder, self.disabled, self.coarse, self.label, false)
            .h(h)
            .radius(6.0)
    }
}
