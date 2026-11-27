//! The terminal screen drawn in a runtime: rows read as text in the accessibility tree, wide
//! cells keep their columns, and unchanged lines reuse their rows.

use std::cell::RefCell;

use rune::{Cx, El, Env, Key, KeyEvent, Runtime};
use troupe_term::Term;

use super::{Metrics, Palette, RowCache, Screen, keys, screen, status_gutter, visible_rows};

struct S {
    term: Term,
    cache: RefCell<RowCache>,
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let t = theme::tokens("termul", true);
    let m = Metrics::new(super::FONT_SIZE, super::LINE_HEIGHT);
    let shown = Screen { term: &s.term, scrolled: 0, focused: true, blink_on: true, marks: &[], compact: false };
    let palette = Palette::new(t);
    rune::row()
        .child(status_gutter("term", &shown, &palette, &m, super::PAD[0]))
        .child(screen("term", &shown, &palette, &m, &s.cache).clip_rounded(0.0))
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
    Screen { term, scrolled: 0, focused: true, blink_on: true, marks: &[], compact: false }
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

#[test]
fn box_drawing_and_blocks_are_shapes_and_text_is_not() {
    use super::glyphs::{custom, draw};
    for c in ['─', '│', '┼', '╔', '╬', '╭', '█', '▄', '░', '▞'] {
        assert!(custom(c), "{c} is drawn");
        assert!(!draw(rune::Shape::new(), c, 0.0, 8.0, 19.0, rune::Color::WHITE).is_empty(), "{c} draws something");
    }
    for c in ['a', '中', '⎺', '·'] {
        assert!(!custom(c), "{c} is the font's");
    }
}

#[test]
fn a_double_corner_meets_at_its_outer_and_inner_lines() {
    use rune::{Color, Shape};
    let w = Color::WHITE;
    let r = |s: Shape, x0: f32, y0: f32, x1: f32, y1: f32| s.fill(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], w);
    // An 8 x 19 cell: the centre is (4, 9); double lines sit one point either side of it.
    let mut want = r(Shape::new(), 3.0, 8.0, 8.0, 9.0);
    want = r(want, 5.0, 10.0, 8.0, 11.0);
    want = r(want, 3.0, 8.0, 4.0, 19.0);
    want = r(want, 5.0, 10.0, 6.0, 19.0);
    assert_eq!(super::glyphs::draw(Shape::new(), '╔', 0.0, 8.0, 19.0, w), want);
}

#[test]
fn a_finished_command_shows_a_success_or_failure_dot_left_of_its_row() {
    let rt = runtime(b"\x1b]133;A;termul\x07$ true\r\n\x1b]133;D;0;termul\x07\x1b]133;A;termul\x07$ false\r\n\x1b]133;D;1;termul\x07$ ");
    let dots: Vec<_> = rt.elements().into_iter().filter(|e| e.role == "img" && !e.exiting).collect();
    let labels: Vec<_> = dots.iter().filter_map(|e| e.label.clone()).collect();
    assert_eq!(labels, ["Command succeeded (exit code 0)", "Command failed (exit code 1)"]);
    let grid = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("term-grid")).expect("the grid").rect;
    let m = Metrics::new(super::FONT_SIZE, super::LINE_HEIGHT);
    for (row, dot) in dots.iter().enumerate() {
        assert_eq!((dot.rect[2], dot.rect[3]), (6.0, 6.0));
        assert_eq!(grid[0] - (dot.rect[0] + dot.rect[2]), 5.0, "in the padding, 5 px from the text");
        let centre = dot.rect[1] + 3.0 - grid[1];
        assert!((centre - (row as f32 + 0.5) * m.cell_h).abs() < 0.5, "centred on row {row}");
    }
    let t = theme::tokens("termul", true);
    assert_eq!((dots[0].fill, dots[1].fill), (t.colors.success, t.colors.destructive));
}
