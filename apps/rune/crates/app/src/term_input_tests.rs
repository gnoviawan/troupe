use std::sync::{Arc, Mutex};

use troupe_term::Term;

use crate::State;
use crate::terminal::TermView;

fn with_terminal(output: &[u8]) -> State {
    let mut s = State::default();
    let mut term = Term::new(20, 5);
    term.advance(output);
    s.terms.views.insert("pty-1".into(), TermView::new(Arc::new(Mutex::new(term))));
    s.terms.focused = Some("pty-1".into());
    s
}

#[test]
fn the_focused_cursor_blinks_every_600_ms_from_the_last_input() {
    let mut s = with_terminal(b"$ ");
    s.terms.blink_from = 10.0;
    s.now = 10.3;
    assert!(s.cursor_shown("pty-1", true));
    assert_eq!(s.next_blink(), Some(10.6));
    assert!(s.blink_flipped(), "the first frame draws the cursor");
    assert!(!s.blink_flipped(), "nothing to redraw within a phase");
    s.now = 10.7;
    assert!(!s.cursor_shown("pty-1", true), "off in the second phase");
    assert!(s.blink_flipped(), "the frame redraws when it turns off");
    assert!(s.cursor_shown("pty-1", false), "a steady cursor never blinks");
    assert!(s.cursor_shown("pty-2", true), "an unfocused terminal does not blink");
}

#[test]
fn a_program_that_asks_for_the_mouse_gets_drags_and_steady_cursors_do_not_wake() {
    let mut s = with_terminal(b"\x1b[?1000h\x1b[?1006h\x1b[2 q");
    assert!(s.terminal_wants_mouse("pty-1"));
    assert_eq!(s.next_blink(), None, "DECSCUSR 2 asked for a steady block");
    let plain = with_terminal(b"$ ");
    assert!(!plain.terminal_wants_mouse("pty-1"));
    s.terms.views.get_mut("pty-1").unwrap().cells.lock().unwrap().advance(b"\x1b[?1000l");
    assert!(!s.terminal_wants_mouse("pty-1"));
}

#[test]
fn the_wheel_goes_to_the_program_on_the_alternate_screen_only() {
    let mut s = with_terminal(b"$ ");
    assert!(!s.terminal_wheel_input("pty-1", 3, 10.0, 10.0), "the normal screen scrolls its history");
    s.terms.views.get_mut("pty-1").unwrap().cells.lock().unwrap().advance(b"\x1b[?1049h");
    assert!(s.terminal_wheel_input("pty-1", -2, 10.0, 10.0), "less and vim hear arrow keys");
    assert!(!s.terminal_wheel_input("pty-1", 0, 10.0, 10.0));
}
