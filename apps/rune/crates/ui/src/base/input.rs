//! `Input` (`components/ui/input.tsx`): a 40 px single-line field on `card` with a quiet 60 %
//! `border` hairline that steps up to `field-border` on hover; focus turns it `focus-ring` with
//! a 1 px ring outside. 14 px text on a desktop pointer, 16 px and 44 px tall on a coarse one
//! (`pointer-coarse:`), so iOS does not zoom.

use rune::{El, TextEdit, text_input};
use theme::Tokens;

use crate::paint::pad;
use crate::type_::sans;

pub struct Input<'a> {
    edit: &'a TextEdit,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    coarse: bool,
    label: Option<String>,
}

impl<'a> Input<'a> {
    pub fn new(edit: &'a TextEdit) -> Input<'a> {
        Input {
            edit,
            placeholder: String::new(),
            disabled: false,
            invalid: false,
            coarse: false,
            label: None,
        }
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    /// `aria-invalid`: announced; the web client draws it the same as a valid field.
    pub fn invalid(mut self, i: bool) -> Self {
        self.invalid = i;
        self
    }
    /// A touch screen: 44 px tall and 16 px text.
    pub fn coarse(mut self, c: bool) -> Self {
        self.coarse = c;
        self
    }
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        field(t, self.edit, &self.placeholder, self.disabled, self.coarse, self.label, self.invalid)
            .single_line()
            .h(if self.coarse { 44.0 } else { 40.0 })
            .radius(8.0)
    }
}

/// The field surface `Input` and `Textarea` share: `px-3 py-2` plus the 1 px border.
pub(crate) fn field(
    t: &Tokens,
    edit: &TextEdit,
    placeholder: &str,
    disabled: bool,
    coarse: bool,
    label: Option<String>,
    invalid: bool,
) -> El {
    let c = &t.colors;
    let font = if coarse { sans(16.0, 400.0, 24.0) } else { sans(14.0, 400.0, 20.0) };
    let mut e = pad(text_input(edit, font), [9.0, 13.0, 9.0, 13.0])
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .fill(c.card)
        .border(1.0, if disabled { c.border } else { c.border.alpha(0.6) })
        .focus_ring(2.0, -1.0, c.focus_ring)
        .disabled(disabled);
    if invalid {
        e = e.invalid(true);
    }
    if let Some(l) = label {
        e = e.label(l);
    }
    if disabled { e.opacity(0.6).read_only(true) } else { e }
}
