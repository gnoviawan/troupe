//! Translates Rune pointer, wheel, magnify, key and IME events into `egui::Event`s, and maps
//! egui's cursor icon back to `rune::Cursor`.

use egui::{CursorIcon, Event, ImeEvent, Key as EguiKey, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton as EguiButton, TouchPhase, pos2, vec2};
use rune::{Cursor, GesturePhase, Ime, Key, KeyEvent, MagnifyEvent, Mods, PointerButton, WheelEvent};

/// Maps Rune's modifiers to egui's. On macOS `cmd` maps to `mac_cmd` + `command`; elsewhere to
/// `ctrl` + `command`.
pub fn modifiers(m: Mods) -> Modifiers {
    let mac = rune::KeyPlatform::current() == rune::KeyPlatform::Mac;
    Modifiers {
        alt: m.alt,
        ctrl: if mac { m.ctrl } else { m.ctrl || m.cmd },
        shift: m.shift,
        mac_cmd: mac && m.cmd,
        command: m.cmd || (!mac && m.ctrl),
    }
}

/// Maps Rune's mouse button to egui's.
pub fn pointer_button(button: PointerButton) -> EguiButton {
    match button {
        PointerButton::Primary => EguiButton::Primary,
        PointerButton::Secondary => EguiButton::Secondary,
        PointerButton::Middle => EguiButton::Middle,
        PointerButton::Back => EguiButton::Extra1,
        PointerButton::Forward => EguiButton::Extra2,
        _ => EguiButton::Primary,
    }
}

/// Translates a Rune wheel event (in slot-local points) into egui pointer-move + wheel events.
pub fn wheel_events(e: &WheelEvent) -> [Event; 2] {
    let phase = match e.phase {
        GesturePhase::Began => TouchPhase::Start,
        GesturePhase::Moved => TouchPhase::Move,
        GesturePhase::Ended => TouchPhase::End,
    };
    let source = if e.precise { MouseWheelSource::Trackpad } else { MouseWheelSource::Wheel };
    [
        Event::PointerMoved(pos2(e.x, e.y)),
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(e.dx, e.dy),
            phase,
            source,
            modifiers: modifiers(e.mods),
        },
    ]
}

/// Translates a Rune magnify (pinch or Ctrl+wheel) event into egui pointer-move + zoom events.
pub fn magnify_events(e: &MagnifyEvent) -> [Event; 2] {
    let factor = (1.0 + e.delta).clamp(0.1, 10.0);
    [Event::PointerMoved(pos2(e.x, e.y)), Event::Zoom(factor)]
}

/// Translates a Rune key press or release into egui events (`Copy`/`Cut`, `Key`, and `Text` for
/// printable presses). Paste is handled by the caller via `Tx::read_clipboard` → `Event::Paste`.
pub fn key_events(event: &KeyEvent, pressed: bool) -> Vec<Event> {
    let mods = modifiers(event.mods);
    let mut out = Vec::new();
    if pressed {
        if event.is_command('c') && !event.mods.shift && !event.mods.alt {
            out.push(Event::Copy);
            return out;
        }
        if event.is_command('x') && !event.mods.shift && !event.mods.alt {
            out.push(Event::Cut);
            return out;
        }
    }
    if let Some(key) = map_key(&event.key) {
        out.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: mods,
        });
    }
    if pressed
        && !event.is_command_chord()
        && let Key::Text(text) = &event.key
        && !text.is_empty()
        && text.chars().all(|c| !c.is_control())
    {
        out.push(Event::Text(text.clone()));
    }
    out
}

/// Translates a Rune IME event into egui's `Event::Ime`.
pub fn ime_event(ime: &Ime) -> Option<Event> {
    match ime {
        Ime::Preedit(text, range, _) => Some(Event::Ime(ImeEvent::Preedit {
            text: text.clone(),
            active_range_chars: range.map(|(start, end)| byte_range_to_chars(text, start, end)),
        })),
        Ime::Commit(text) => Some(Event::Ime(ImeEvent::Commit(text.clone()))),
        Ime::Enabled | Ime::Disabled | Ime::Select(_) => None,
    }
}

fn byte_range_to_chars(text: &str, start: usize, end: usize) -> std::ops::Range<usize> {
    let start = start.min(text.len());
    let end = end.clamp(start, text.len());
    let c_start = text.get(..start).map_or(0, |s| s.chars().count());
    let c_end = text.get(..end).map_or(c_start, |s| s.chars().count());
    c_start..c_end
}

/// Maps egui's cursor icon to Rune's cursor.
pub fn cursor(icon: CursorIcon) -> Cursor {
    match icon {
        CursorIcon::Default | CursorIcon::ContextMenu | CursorIcon::Help | CursorIcon::Progress | CursorIcon::Wait | CursorIcon::Alias | CursorIcon::Copy => Cursor::Default,
        CursorIcon::None => Cursor::Hidden,
        CursorIcon::PointingHand => Cursor::Pointer,
        CursorIcon::Cell => Cursor::Cell,
        CursorIcon::Crosshair => Cursor::Crosshair,
        CursorIcon::Text | CursorIcon::VerticalText => Cursor::Text,
        CursorIcon::Move | CursorIcon::AllScroll => Cursor::Move,
        CursorIcon::NoDrop | CursorIcon::NotAllowed => Cursor::NotAllowed,
        CursorIcon::Grab => Cursor::Grab,
        CursorIcon::Grabbing => Cursor::Grabbing,
        CursorIcon::ResizeHorizontal | CursorIcon::ResizeColumn | CursorIcon::ResizeEast | CursorIcon::ResizeWest => Cursor::ResizeColumn,
        CursorIcon::ResizeVertical | CursorIcon::ResizeRow | CursorIcon::ResizeNorth | CursorIcon::ResizeSouth => Cursor::ResizeRow,
        CursorIcon::ResizeNeSw | CursorIcon::ResizeNwSe | CursorIcon::ResizeNorthEast | CursorIcon::ResizeSouthWest | CursorIcon::ResizeNorthWest | CursorIcon::ResizeSouthEast => Cursor::Move,
        CursorIcon::ZoomIn => Cursor::ZoomIn,
        CursorIcon::ZoomOut => Cursor::ZoomOut,
    }
}

fn map_key(key: &Key) -> Option<EguiKey> {
    match key {
        Key::Enter => Some(EguiKey::Enter),
        Key::Backspace => Some(EguiKey::Backspace),
        Key::Delete => Some(EguiKey::Delete),
        Key::Left => Some(EguiKey::ArrowLeft),
        Key::Right => Some(EguiKey::ArrowRight),
        Key::Up => Some(EguiKey::ArrowUp),
        Key::Down => Some(EguiKey::ArrowDown),
        Key::Home => Some(EguiKey::Home),
        Key::End => Some(EguiKey::End),
        Key::PageUp => Some(EguiKey::PageUp),
        Key::PageDown => Some(EguiKey::PageDown),
        Key::Insert => Some(EguiKey::Insert),
        Key::Escape => Some(EguiKey::Escape),
        Key::Tab => Some(EguiKey::Tab),
        Key::Function(n) => match n {
            1 => Some(EguiKey::F1),
            2 => Some(EguiKey::F2),
            3 => Some(EguiKey::F3),
            4 => Some(EguiKey::F4),
            5 => Some(EguiKey::F5),
            6 => Some(EguiKey::F6),
            7 => Some(EguiKey::F7),
            8 => Some(EguiKey::F8),
            9 => Some(EguiKey::F9),
            10 => Some(EguiKey::F10),
            11 => Some(EguiKey::F11),
            12 => Some(EguiKey::F12),
            _ => None,
        },
        Key::Command(c) => map_char(*c),
        Key::Text(s) => {
            let mut chars = s.chars();
            let first = chars.next()?;
            chars.next().is_none().then(|| map_char(first)).flatten()
        }
        _ => None,
    }
}

fn map_char(c: char) -> Option<EguiKey> {
    match c.to_ascii_lowercase() {
        ' ' => Some(EguiKey::Space),
        'a' => Some(EguiKey::A),
        'b' => Some(EguiKey::B),
        'c' => Some(EguiKey::C),
        'd' => Some(EguiKey::D),
        'e' => Some(EguiKey::E),
        'f' => Some(EguiKey::F),
        'g' => Some(EguiKey::G),
        'h' => Some(EguiKey::H),
        'i' => Some(EguiKey::I),
        'j' => Some(EguiKey::J),
        'k' => Some(EguiKey::K),
        'l' => Some(EguiKey::L),
        'm' => Some(EguiKey::M),
        'n' => Some(EguiKey::N),
        'o' => Some(EguiKey::O),
        'p' => Some(EguiKey::P),
        'q' => Some(EguiKey::Q),
        'r' => Some(EguiKey::R),
        's' => Some(EguiKey::S),
        't' => Some(EguiKey::T),
        'u' => Some(EguiKey::U),
        'v' => Some(EguiKey::V),
        'w' => Some(EguiKey::W),
        'x' => Some(EguiKey::X),
        'y' => Some(EguiKey::Y),
        'z' => Some(EguiKey::Z),
        '0' => Some(EguiKey::Num0),
        '1' => Some(EguiKey::Num1),
        '2' => Some(EguiKey::Num2),
        '3' => Some(EguiKey::Num3),
        '4' => Some(EguiKey::Num4),
        '5' => Some(EguiKey::Num5),
        '6' => Some(EguiKey::Num6),
        '7' => Some(EguiKey::Num7),
        '8' => Some(EguiKey::Num8),
        '9' => Some(EguiKey::Num9),
        '-' => Some(EguiKey::Minus),
        '=' => Some(EguiKey::Equals),
        '+' => Some(EguiKey::Plus),
        '[' => Some(EguiKey::OpenBracket),
        ']' => Some(EguiKey::CloseBracket),
        '\\' => Some(EguiKey::Backslash),
        ';' => Some(EguiKey::Semicolon),
        '\'' => Some(EguiKey::Quote),
        ',' => Some(EguiKey::Comma),
        '.' => Some(EguiKey::Period),
        '/' => Some(EguiKey::Slash),
        '`' => Some(EguiKey::Backtick),
        _ => None,
    }
}
