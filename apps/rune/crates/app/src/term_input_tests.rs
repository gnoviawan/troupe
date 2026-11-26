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

/// The `data` of every write sent to the PTY.
fn writes(s: &State) -> Vec<String> {
    s.terms
        .sent
        .iter()
        .filter_map(|c| match c {
            client::terminal::Command::Call { kind: troupe_protocol::terminal::WebTerminalRequestType::Write, payload, .. } => {
                payload["data"].as_str().map(str::to_owned)
            }
            _ => None,
        })
        .collect()
}

/// `with_terminal` with a live session, so writes go out.
fn live(output: &[u8]) -> State {
    let mut s = with_terminal(output);
    s.terms.sessions.spawn(9, "p", None);
    let spawned = model::terminal::Spawned { id: "pty-1".into(), shell: "sh".into(), cwd: "/".into(), pid: 1, cols: 20, rows: 5, claim: "c".into() };
    s.terms.sessions.spawned(9, spawned);
    s
}

fn wheel(dy: f32, x: f32) -> rune::WheelEvent {
    rune::WheelEvent::new(0.0, dy).at(x, 10.0).mods(rune::Mods { ctrl: true, ..rune::Mods::default() })
}

#[test]
fn the_wheel_goes_to_the_program_once_per_event() {
    let mut s = live(b"$ ");
    assert!(!s.terminal_wheel_input("pty-1", 3, &wheel(60.0, 10.0)), "the normal screen scrolls its history");
    s.terms.views["pty-1"].cells.lock().unwrap().advance(b"\x1b[?1049h");
    assert!(s.terminal_wheel_input("pty-1", -5, &wheel(-100.0, 10.0)), "less and vim hear arrow keys");
    assert!(s.terminal_wheel_input("pty-1", 0, &wheel(-2.0, 10.0)), "a part line still belongs to the program");
    assert_eq!(writes(&s), ["\x1b[B"], "one arrow for the whole event");
    s.terms.views["pty-1"].cells.lock().unwrap().advance(b"\x1b[?1000h\x1b[?1006h");
    assert!(s.terminal_wheel_input("pty-1", 4, &wheel(80.0, 10.0)));
    assert_eq!(writes(&s)[1..], ["\x1b[<80;1;1M".to_owned()], "one wheel-up report with Ctrl, not four");
}

#[test]
fn a_default_encoded_report_past_column_95_is_not_sent_and_moves_report_once_per_cell() {
    let mut s = live(b"\x1b[?1003h");
    s.terms.views.insert("pty-1".into(), TermView::new(Arc::new(Mutex::new(Term::new(200, 5)))));
    s.terms.views["pty-1"].cells.lock().unwrap().advance(b"\x1b[?1003h");
    let m = s.terms.metrics();
    let at = |col: f32| (col * m.cell_w + 1.0, 1.0);
    let none = rune::Mods::default();
    use troupe_term::mouse::{Action, Button};
    assert_eq!(s.mouse_report("pty-1", Button::Left, Action::Down, at(94.0), none).as_deref(), Some("\x1b[M \x7f!"));
    assert_eq!(s.mouse_report("pty-1", Button::Left, Action::Down, at(120.0), none), None, "no lossy bytes reach vim");
    assert!(s.mouse_report("pty-1", Button::None, Action::Move, at(3.0), none).is_some());
    assert_eq!(s.mouse_report("pty-1", Button::None, Action::Move, (at(3.0).0 + 2.0, 3.0), none), None, "same cell");
    assert!(s.mouse_report("pty-1", Button::None, Action::Move, at(4.0), none).is_some(), "next cell");
}
