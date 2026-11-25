//! The phone's terminal key bar (`MobileTerminalControls`): under the active terminal, a
//! toggle and Paste stay at the start and nine keys a phone keyboard lacks scroll sideways
//! in their own lane. A key writes its bytes to the PTY; Paste writes the clipboard as it is.

use rune::{El, Tx, el, row};
use theme::Tokens;
use ui::type_::mono;
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;

/// `[visible label, bytes, spoken name]`, as React's `KEYS`.
pub const KEYS: [(&str, &str, &str); 9] = [
    ("Esc", "\x1b", "Esc, escape"),
    ("Tab", "\t", "Tab"),
    ("Ctrl+C", "\x03", "Ctrl+C, interrupt"),
    ("←", "\x1b[D", "Left arrow"),
    ("↑", "\x1b[A", "Up arrow"),
    ("↓", "\x1b[B", "Down arrow"),
    ("→", "\x1b[C", "Right arrow"),
    ("PgUp", "\x1b[5~", "PgUp, page up"),
    ("PgDn", "\x1b[6~", "PgDn, page down"),
];

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

/// The bar under terminal `id`.
pub fn bar(s: &State, t: &'static Tokens, id: &str) -> El {
    let c = &t.colors;
    let expanded = !s.terms.keys_hidden;
    let toggle = Button::icon_only(glyph("Keyboard"), "Show/hide key bar", ButtonSize::Touch)
        .variant(ButtonVariant::Ghost)
        .build(t)
        .key("terminal-keys-toggle")
        .min_w(44.0)
        .px(12.0)
        .expanded(expanded)
        .on_press(|s: &mut State| s.terms.keys_hidden = !s.terms.keys_hidden);
    let paste_id = id.to_owned();
    let paste = Button::new("Paste")
        .icon(glyph("ClipboardPaste"))
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Touch)
        .build(t)
        .key("terminal-keys-paste")
        .px(12.0)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| s.key_bar_paste(&paste_id, tx));
    let mut group = row()
        .key("terminal-keys")
        .role("group")
        .label("Terminal keys")
        .w_full()
        .gap(4.0)
        .items_center()
        .child(toggle)
        .child(paste)
        .child(el().w(0.5).h(24.0).fill(c.border).a11y_hidden());
    if expanded {
        let mut lane = row().key("terminal-keys-lane").gap(6.0).py(2.0).items_center();
        for (label, bytes, name) in KEYS {
            let id = id.to_owned();
            // `variant="secondary"` at 44 px with the label in the mono face.
            let key = row()
                .key(format!("terminal-key-{label}"))
                .role("button")
                .label(name)
                .h(44.0)
                .min_w(44.0)
                .px(12.0)
                .center()
                .radius(8.0)
                .fill(c.control_fill)
                .hover(c.control_fill_hover)
                .child(rune::text(label).font(mono(14.0, 500.0, 20.0)).color(c.foreground))
                .on_press(move |s: &mut State| s.key_bar_write(&id, bytes));
            lane = lane.child(ui::paint::shadows(key, &t.shadows.shadow_control));
        }
        group = group.child(el().grow(1.0).min_w(0.0).child(lane).scroll_x());
    }
    rune::col()
        .key(format!("terminal-key-bar-{id}"))
        .w_full()
        .px(8.0)
        .pt(2.0)
        .pb(8.0)
        .fill(c.card)
        .border_sides([0.5, 0.0, 0.0, 0.0], c.border)
        .child(group)
}

impl State {
    /// A key bar key: its bytes go to the PTY.
    pub(crate) fn key_bar_write(&mut self, id: &str, bytes: &str) {
        log::info!("[term] key bar sent {bytes:?} to {id}");
        self.terms.blink_from = self.now;
        self.terminal_write(id, bytes);
    }

    /// The key bar's Paste: the clipboard's text, unbracketed, as React's bar sends it.
    pub(crate) fn key_bar_paste(&mut self, id: &str, tx: &mut Tx) {
        let id = id.to_owned();
        tx.read_clipboard(move |s: &mut State, content, _tx| {
            let text = content.text();
            if !text.is_empty() {
                s.terminal_write(&id, text);
            }
        });
    }
}

#[cfg(test)]
#[path = "term_keys_tests.rs"]
mod tests;
