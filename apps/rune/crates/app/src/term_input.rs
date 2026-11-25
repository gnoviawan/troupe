//! What a terminal tab sends besides typed keys: pointer reports for programs that asked for
//! them (`vim`, `htop`, `tmux`), the wheel as arrow keys on the alternate screen, pastes
//! (bracketed when the program turned that on), and the cursor's blink, as xterm.js has them.

use rune::{Drag, Phase, Tx};
use troupe_term::mouse::{Action, Button, MouseEvent, paste, report, wants_wheel};
use troupe_term::{MouseProtocol, Term};

use crate::State;

/// xterm.js's cursor blink: on and off every 600 ms, restarted by input and output.
pub const BLINK: f64 = 0.6;
/// The largest paste React sends (`MAX_CLIPBOARD_SIZE`).
const MAX_PASTE: usize = 10 * 1024 * 1024;

/// The screen cell under `(x, y)` points.
fn cell(term: &Term, cell_w: f32, cell_h: f32, x: f32, y: f32) -> (usize, usize) {
    let col = (x / cell_w).floor().clamp(0.0, (term.cols() - 1) as f32) as usize;
    let row = (y / cell_h).floor().clamp(0.0, (term.rows() - 1) as f32) as usize;
    (col, row)
}

impl State {
    /// The bytes a pointer event on terminal `id` reports, when its program asked for them.
    /// A move to the cell the last report named is not reported again, as xterm.js drops
    /// equal events.
    fn mouse_report(&mut self, id: &str, button: Button, action: Action, at: (f32, f32), mods: rune::Mods) -> Option<String> {
        let m = self.terms.metrics();
        let view = self.terms.views.get(id)?;
        let term = view.cells.lock().ok()?;
        if term.modes().mouse == MouseProtocol::None {
            return None;
        }
        let (col, row) = cell(&term, m.cell_w, m.cell_h, at.0, at.1);
        let event = MouseEvent { button, action, col, row, shift: mods.shift, alt: mods.alt, ctrl: mods.ctrl };
        let last = &mut self.terms.last_mouse;
        if action == Action::Move && last.as_ref().is_some_and(|(lid, e)| lid == id && *e == event) {
            return None;
        }
        *last = Some((id.to_owned(), event));
        report(term.modes(), &event).and_then(|b| String::from_utf8(b).ok())
    }

    /// Whether terminal `id`'s program takes pointer events: then a drag reports instead of
    /// selecting.
    pub(crate) fn terminal_wants_mouse(&self, id: &str) -> bool {
        let Some(view) = self.terms.views.get(id) else { return false };
        view.cells.lock().is_ok_and(|t| t.modes().mouse != MouseProtocol::None)
    }

    /// A drag on a terminal whose program reports the pointer. A drag carries no modifiers
    /// at the pin.
    pub(crate) fn terminal_mouse_drag(&mut self, id: &str, d: &Drag) {
        let action = match d.phase {
            Phase::Down => Action::Down,
            Phase::Move => Action::Move,
            Phase::Up => Action::Up,
        };
        if let Some(bytes) = self.mouse_report(id, Button::Left, action, (d.x, d.y), rune::Mods::default()) {
            self.terminal_write(id, &bytes);
        }
    }

    /// The pointer moved with no button held: programs in any-event mode hear it.
    pub(crate) fn terminal_mouse_motion(&mut self, id: &str, x: f32, y: f32) {
        let any = self.terms.views.get(id).and_then(|v| v.cells.lock().ok().map(|t| t.modes().mouse == MouseProtocol::Any));
        if any == Some(true)
            && let Some(bytes) = self.mouse_report(id, Button::None, Action::Move, (x, y), rune::Mods::default())
        {
            self.terminal_write(id, &bytes);
        }
    }

    /// A wheel event over terminal `id` that moved `lines` (positive: content down, toward
    /// older lines). True when the program took it, as xterm.js sends it: one wheel report
    /// per event while the program takes the wheel (none when it cannot be encoded), else one
    /// arrow key per event on the alternate screen. False leaves the scrollback to scroll.
    pub(crate) fn terminal_wheel_input(&mut self, id: &str, lines: i32, e: &rune::WheelEvent) -> bool {
        let Some(view) = self.terms.views.get(id) else { return false };
        let Ok(term) = view.cells.lock() else { return false };
        let (wheel, alt, app) = (wants_wheel(term.modes()), term.alt_screen(), term.modes().application_cursor_keys);
        drop(term);
        if !wheel && !alt {
            return false;
        }
        if lines == 0 {
            return true;
        }
        let up = lines > 0;
        if wheel {
            let button = if up { Button::WheelUp } else { Button::WheelDown };
            if let Some(bytes) = self.mouse_report(id, button, Action::Down, (e.x, e.y), e.mods) {
                self.terminal_write(id, &bytes);
            }
            return true;
        }
        let arrow = match (up, app) {
            (true, true) => "\x1bOA",
            (true, false) => "\x1b[A",
            (false, true) => "\x1bOB",
            (false, false) => "\x1b[B",
        };
        self.terminal_write(id, arrow);
        true
    }

    /// Reads the clipboard and pastes it into terminal `id` (Ctrl+V, ⌘V on macOS).
    pub(crate) fn terminal_paste_clipboard(&mut self, id: &str, tx: &mut Tx) {
        let id = id.to_owned();
        tx.read_clipboard(move |s: &mut State, content, _tx| s.terminal_paste(&id, content.text()));
    }

    /// Pastes `text` into terminal `id`: newlines as returns, bracketed when the program
    /// turned bracketed paste on.
    pub(crate) fn terminal_paste(&mut self, id: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        if text.len() > MAX_PASTE {
            log::warn!("[term] paste of {} bytes refused; the most is {MAX_PASTE}", text.len());
            return;
        }
        let Some(view) = self.terms.views.get_mut(id) else { return };
        let Ok(term) = view.cells.lock() else { return };
        let data = paste(term.modes(), text);
        drop(term);
        view.scrolled = 0;
        view.selection = None;
        log::info!("[term] paste of {} bytes into {id}", text.len());
        self.terms.blink_from = self.now;
        self.terminal_write(id, &data);
    }

    /// Whether terminal `id`'s cursor shows now: a blinking cursor is off for every other
    /// 600 ms since the last input or output, and only the focused terminal blinks.
    pub(crate) fn cursor_shown(&self, id: &str, blinks: bool) -> bool {
        if !blinks || self.terms.focused.as_deref() != Some(id) {
            return true;
        }
        (((self.now - self.terms.blink_from).max(0.0) / BLINK) as u64).is_multiple_of(2)
    }

    /// Whether the focused cursor turned on or off since the last call: the frame redraws.
    pub(crate) fn blink_flipped(&mut self) -> bool {
        let shown = self.terms.focused.clone().and_then(|id| {
            let blinks = self.terms.views.get(&id)?.cells.lock().ok()?.cursor().blink.unwrap_or(true);
            Some(self.cursor_shown(&id, blinks))
        });
        let flipped = shown != self.terms.blink_shown;
        self.terms.blink_shown = shown;
        flipped
    }

    /// When the focused terminal's cursor next turns on or off, if it blinks.
    pub(crate) fn next_blink(&self) -> Option<f64> {
        let id = self.terms.focused.as_deref()?;
        let view = self.terms.views.get(id)?;
        let blinks = view.cells.lock().ok()?.cursor().blink.unwrap_or(true);
        if !blinks {
            return None;
        }
        let since = (self.now - self.terms.blink_from).max(0.0);
        Some(self.terms.blink_from + ((since / BLINK).floor() + 1.0) * BLINK)
    }
}

#[cfg(test)]
#[path = "term_input_tests.rs"]
mod tests;
