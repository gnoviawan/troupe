//! Pointer reports a program asked for (`CSI ? 1000 h` and the rest), encoded as xterm.js's
//! `CoreMouseService` encodes them, and the text a paste sends.

use crate::{Modes, MouseEncoding, MouseProtocol};

/// A pointer button, or the wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    /// Motion with no button held.
    None,
    WheelUp,
    WheelDown,
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Down,
    Up,
    Move,
}

/// A pointer event on cell `(col, row)` of the screen, both from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub button: Button,
    pub action: Action,
    pub col: usize,
    pub row: usize,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// Whether the protocol in `modes` reports `e` (`DEFAULT_PROTOCOLS` restrictions).
fn wanted(protocol: MouseProtocol, e: &MouseEvent) -> bool {
    let wheel = matches!(e.button, Button::WheelUp | Button::WheelDown);
    match protocol {
        MouseProtocol::None => false,
        MouseProtocol::X10 => e.action == Action::Down && !wheel,
        MouseProtocol::Vt200 => e.action != Action::Move,
        MouseProtocol::Drag => e.action != Action::Move || e.button != Button::None,
        MouseProtocol::Any => true,
    }
}

/// The bytes reporting `e` to the program, or None when it did not ask for such events.
pub fn report(modes: &Modes, e: &MouseEvent) -> Option<Vec<u8>> {
    if !wanted(modes.mouse, e) {
        return None;
    }
    let sgr = matches!(modes.mouse_encoding, MouseEncoding::Sgr | MouseEncoding::SgrPixels);
    let mut code: u32 = match e.button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
        Button::None => 3,
        Button::WheelUp => 64,
        Button::WheelDown => 65,
    };
    // The default encoding has no release per button: every release is button 3.
    if e.action == Action::Up && !sgr {
        code = 3;
    }
    if e.action == Action::Move {
        code += 32;
    }
    // X10 mode reports no modifiers.
    if modes.mouse != MouseProtocol::X10 {
        code += 4 * u32::from(e.shift) + 8 * u32::from(e.alt) + 16 * u32::from(e.ctrl);
    }
    let (x, y) = (e.col + 1, e.row + 1);
    if sgr {
        let end = if e.action == Action::Up { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    // The default encoding carries each value plus 32 in one byte, up to 223.
    if x > 223 || y > 223 {
        return None;
    }
    Some(vec![0x1b, b'[', b'M', (code + 32) as u8, (x + 32) as u8, (y + 32) as u8])
}

/// The bytes a paste of `text` sends: newlines become carriage returns, and with bracketed
/// paste on the text is wrapped in `ESC [ 200 ~` and `ESC [ 201 ~` with every Escape inside
/// shown as `␛`, so a pasted sequence cannot end the paste early (xterm.js's
/// `prepareTextForTerminal` and `bracketTextForPaste`).
pub fn paste(modes: &Modes, text: &str) -> String {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if !modes.bracketed_paste {
        return text;
    }
    format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "\u{241b}"))
}

#[cfg(test)]
#[path = "mouse_tests.rs"]
mod tests;
