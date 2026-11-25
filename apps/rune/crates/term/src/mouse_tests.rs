use crate::mouse::{Action, Button, MouseEvent, paste, report};
use crate::{Modes, MouseEncoding, MouseProtocol};

fn modes(mouse: MouseProtocol, encoding: MouseEncoding) -> Modes {
    Modes { mouse, mouse_encoding: encoding, ..Modes::default() }
}

fn ev(button: Button, action: Action, col: usize, row: usize) -> MouseEvent {
    MouseEvent { button, action, col, row, shift: false, alt: false, ctrl: false }
}

#[test]
fn the_default_encoding_adds_32_and_reports_every_release_as_button_3() {
    let m = modes(MouseProtocol::Vt200, MouseEncoding::Default);
    assert_eq!(report(&m, &ev(Button::Left, Action::Down, 0, 0)), Some(b"\x1b[M !!".to_vec()));
    assert_eq!(report(&m, &ev(Button::Right, Action::Up, 9, 4)), Some(b"\x1b[M#*%".to_vec()));
    assert_eq!(report(&m, &ev(Button::Left, Action::Move, 1, 1)), None, "1000 reports no motion");
    assert_eq!(report(&m, &ev(Button::Left, Action::Down, 94, 0)), Some(b"\x1b[M \x7f!".to_vec()), "the last one-byte column");
    assert_eq!(report(&m, &ev(Button::Left, Action::Down, 95, 0)), None, "column 96 needs byte 0x80");
    assert_eq!(report(&m, &ev(Button::Left, Action::Down, 0, 120)), None, "so does row 121");
}

#[test]
fn sgr_names_the_button_released_and_keeps_large_coordinates() {
    let m = modes(MouseProtocol::Drag, MouseEncoding::Sgr);
    assert_eq!(report(&m, &ev(Button::Left, Action::Down, 299, 4)), Some(b"\x1b[<0;300;5M".to_vec()));
    assert_eq!(report(&m, &ev(Button::Left, Action::Move, 2, 2)), Some(b"\x1b[<32;3;3M".to_vec()));
    assert_eq!(report(&m, &ev(Button::None, Action::Move, 2, 2)), None, "1002 reports motion only while a button is held");
    assert_eq!(report(&m, &ev(Button::Left, Action::Up, 2, 2)), Some(b"\x1b[<0;3;3m".to_vec()));
    let mut wheel = ev(Button::WheelDown, Action::Down, 0, 0);
    wheel.ctrl = true;
    assert_eq!(report(&m, &wheel), Some(b"\x1b[<81;1;1M".to_vec()));
}

#[test]
fn x10_reports_presses_only_without_modifiers() {
    let m = modes(MouseProtocol::X10, MouseEncoding::Default);
    let mut press = ev(Button::Left, Action::Down, 0, 0);
    press.shift = true;
    assert_eq!(report(&m, &press), Some(b"\x1b[M !!".to_vec()));
    assert_eq!(report(&m, &ev(Button::Left, Action::Up, 0, 0)), None);
    assert_eq!(report(&modes(MouseProtocol::None, MouseEncoding::Sgr), &press), None);
}

#[test]
fn a_paste_turns_newlines_into_returns_and_brackets_only_when_asked() {
    let plain = Modes::default();
    assert_eq!(paste(&plain, "a\nb\r\nc"), "a\rb\rc");
    let bracketed = Modes { bracketed_paste: true, ..Modes::default() };
    assert_eq!(paste(&bracketed, "ls\n\x1b[201~rm"), "\x1b[200~ls\r\u{241b}[201~rm\x1b[201~");
}
