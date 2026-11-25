//! The terminal screen drawn in a runtime: rows read as text in the accessibility tree, wide
//! cells keep their columns, and unchanged lines reuse their rows.

use std::cell::RefCell;

use rune::{Cx, El, Env, Key, KeyEvent, Runtime};
use troupe_term::Term;

use super::{Metrics, Palette, RowCache, Screen, keys, screen, visible_rows};

struct S {
    term: Term,
    cache: RefCell<RowCache>,
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let t = theme::tokens("termul", true);
    let m = Metrics::new(super::FONT_SIZE, super::LINE_HEIGHT);
    let shown = Screen { term: &s.term, scrolled: 0, focused: true, blink_on: true };
    screen("term", &shown, &Palette::new(t), &m, &s.cache)
}

fn runtime(bytes: &[u8]) -> Runtime<S> {
    let mut term = Term::new(40, 6);
    term.advance(bytes);
    let mut rt = Runtime::new(S { term, cache: RefCell::default() }, view, (400.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

#[test]
fn rows_read_as_text_with_wide_cells_in_place() {
    let rt = runtime("\x1b[31mred\x1b[0m plain\r\n中文|x\r\n\x1b[7minverse\x1b[0m".as_bytes());
    let tree = format!("{:?}", rt.accessibility_tree());
    for want in ["red plain", "中文|x", "inverse"] {
        assert!(tree.contains(want), "{want:?} in {tree}");
    }
}

fn live(term: &Term) -> Screen<'_> {
    Screen { term, scrolled: 0, focused: true, blink_on: true }
}

#[test]
fn an_unchanged_line_keeps_its_row() {
    let mut term = Term::new(20, 3);
    term.advance(b"one\r\ntwo");
    let t = theme::tokens("termul", true);
    let (p, m) = (Palette::new(t), Metrics::new(14.0, 1.2));
    let mut cache = RowCache::default();
    let first = visible_rows(&live(&term), &p, &m, &mut cache);
    term.advance(b"!");
    let second = visible_rows(&live(&term), &p, &m, &mut cache);
    assert!(std::rc::Rc::ptr_eq(&first[0], &second[0]), "row 0 did not change");
    assert!(!std::rc::Rc::ptr_eq(&first[1], &second[1]), "row 1 did");
}

#[test]
fn the_palette_follows_xterm() {
    let p = Palette::new(theme::tokens("termul", true));
    let c = p.ansi[16 + 36 * 5 + 6 * 2 + 1];
    assert_eq!([c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8), [0xff, 0x87, 0x5f]);
    let g = p.ansi[232];
    assert_eq!((g.r * 255.0).round() as u8, 8);
}

fn key(key: Key, ctrl: bool, alt: bool, shift: bool) -> KeyEvent {
    let mut e = KeyEvent { key, mods: Default::default() };
    e.mods.ctrl = ctrl;
    e.mods.cmd = ctrl;
    e.mods.alt = alt;
    e.mods.shift = shift;
    e
}

#[test]
fn keys_encode_as_xterm_sends_them() {
    let cases: [(KeyEvent, bool, &[u8]); 9] = [
        (key(Key::Text("a".into()), false, false, false), false, b"a"),
        (key(Key::Command('c'), true, false, false), false, b"\x03"),
        (key(Key::Text("x".into()), false, true, false), false, b"\x1bx"),
        (key(Key::Up, false, false, false), false, b"\x1b[A"),
        (key(Key::Up, false, false, false), true, b"\x1bOA"),
        (key(Key::Right, true, false, false), true, b"\x1b[1;5C"),
        (key(Key::Backspace, false, false, false), false, b"\x7f"),
        (key(Key::Tab, false, false, true), false, b"\x1b[Z"),
        (key(Key::Function(5), false, false, false), false, b"\x1b[15~"),
    ];
    for (e, app, want) in cases {
        assert_eq!(keys::encode(&e, app).as_deref(), Some(want), "{e:?}");
    }
}
