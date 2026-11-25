//! What a key sends to the PTY, as xterm.js's `evaluateKeyboardEvent` encodes it: printable
//! text as typed, Ctrl with a letter as its control byte, Alt as an Escape prefix, and the
//! cursor, editing and function keys as their VT sequences, in application mode where the
//! program asked for it (`DECCKM`).

use rune::{Key, KeyEvent};

/// The xterm modifier parameter (`1 + shift + 2*alt + 4*ctrl`), or None with none held.
fn modifier(e: &KeyEvent) -> Option<u8> {
    let m = e.mods;
    let n = u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl);
    (n > 0).then_some(n + 1)
}

/// `CSI 1;m X` with modifiers, else `CSI X`, or `SS3 X` in application mode.
fn cursor(e: &KeyEvent, app: bool, x: char) -> Vec<u8> {
    match modifier(e) {
        Some(m) => format!("\x1b[1;{m}{x}").into_bytes(),
        None if app => format!("\x1bO{x}").into_bytes(),
        None => format!("\x1b[{x}").into_bytes(),
    }
}

/// `CSI n ~`, or `CSI n;m ~` with modifiers.
fn tilde(e: &KeyEvent, n: u8) -> Vec<u8> {
    match modifier(e) {
        Some(m) => format!("\x1b[{n};{m}~").into_bytes(),
        None => format!("\x1b[{n}~").into_bytes(),
    }
}

/// The control byte Ctrl turns `c` into, as a terminal keyboard sends it.
fn control(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        c @ 'a'..='z' => Some(c as u8 - b'a' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '/' | '-' | '7' => Some(0x1f),
        '8' | '?' => Some(0x7f),
        _ => None,
    }
}

/// The bytes `e` sends, or None for a key the terminal leaves to the app (Cmd chords on macOS).
pub fn encode(e: &KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
    let m = e.mods;
    let esc = |mut bytes: Vec<u8>| {
        if m.alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    Some(match &e.key {
        Key::Text(t) if m.ctrl => {
            let c = t.chars().next()?;
            esc(vec![control(c)?])
        }
        Key::Command(c) if m.ctrl => esc(vec![control(*c)?]),
        Key::Command(_) => return None,
        // ⌘ chords on macOS are the app's (copy, paste); off macOS `cmd` comes with `ctrl`.
        Key::Text(_) if m.cmd && !m.alt => return None,
        Key::Text(t) => esc(t.as_bytes().to_vec()),
        Key::Enter => esc(vec![b'\r']),
        Key::Backspace if m.ctrl => esc(vec![0x08]),
        Key::Backspace => esc(vec![0x7f]),
        Key::Tab if m.shift => b"\x1b[Z".to_vec(),
        Key::Tab => esc(vec![b'\t']),
        Key::Escape => esc(vec![0x1b]),
        Key::Up => cursor(e, application_cursor, 'A'),
        Key::Down => cursor(e, application_cursor, 'B'),
        Key::Right => cursor(e, application_cursor, 'C'),
        Key::Left => cursor(e, application_cursor, 'D'),
        Key::Home => cursor(e, application_cursor, 'H'),
        Key::End => cursor(e, application_cursor, 'F'),
        Key::Insert => tilde(e, 2),
        Key::Delete => tilde(e, 3),
        Key::PageUp => tilde(e, 5),
        Key::PageDown => tilde(e, 6),
        Key::Function(n @ 1..=4) => match modifier(e) {
            Some(m) => format!("\x1b[1;{m}{}", (b'P' + n - 1) as char).into_bytes(),
            None => format!("\x1bO{}", (b'P' + n - 1) as char).into_bytes(),
        },
        Key::Function(n @ 5..=12) => tilde(e, [15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)]),
        _ => return None,
    })
}
