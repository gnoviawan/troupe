use std::sync::{Arc, Mutex};

use troupe_term::Term;
use troupe_term::selection::{Selection, Unit};
use troupe_term::text::Point;

use super::{Find, Held, marks};
use crate::terminal::TermView;

fn palette() -> ui::terminal::Palette {
    ui::terminal::Palette::new(theme::tokens("troupe", true))
}

fn view(term: Term) -> TermView {
    TermView::new(Arc::new(Mutex::new(term)))
}

#[test]
fn a_selection_follows_scrolled_out_lines_and_clears_with_its_start() {
    let mut term = Term::with_scrollback(10, 2, 2);
    term.advance(b"a\r\nb\r\nc\r\nd");
    let sel = Selection { anchor: Point::new(1, 0), head: Point::new(2, 1), unit: Unit::Cell };
    let held = Held::new(sel, &term);
    term.advance(b"\r\ne");
    assert_eq!(held.current(&term).map(|s| s.anchor), Some(Point::new(0, 0)), "one line dropped");
    term.advance(b"\r\nf");
    assert_eq!(held.current(&term), None, "its first line is gone");
}

#[test]
fn marks_fill_the_selection_and_the_matches_on_visible_rows() {
    let mut term = Term::new(10, 3);
    term.advance(b"one two\r\nthree");
    let start = term.screen_start();
    let mut v = view(term);
    {
        let term = v.cells.lock().unwrap();
        v.selection = Some(Held::new(Selection { anchor: Point::new(start, 4), head: Point::new(start + 1, 2), unit: Unit::Cell }, &term));
    }
    v.find = Some(Find { matches: vec![troupe_term::search::Match { start: Point::new(start + 1, 0), end: Point::new(start + 1, 5) }], active: Some(0), ..Find::default() });
    let p = palette();
    let search = [rune::Color::BLACK, rune::Color::WHITE];
    let term = v.cells.lock().unwrap();
    let got: Vec<_> = marks(&v, &term, &p, search, true).iter().map(|m| (m.row, m.start, m.end, m.fill)).collect();
    assert_eq!(
        got,
        [(1, 0, 5, Some(rune::Color::WHITE)), (0, 4, 10, Some(p.selection)), (1, 0, 2, Some(p.selection))],
        "the active match, then the selection over two rows"
    );
}

#[test]
fn the_find_count_reads_like_a_find_widget() {
    let mut f = Find::default();
    assert_eq!(f.count(), "");
    f.edit = rune::TextEdit::new("x");
    assert_eq!(f.count(), "No results");
    f.matches = vec![troupe_term::search::Match { start: Point::new(0, 0), end: Point::new(0, 1) }; 3];
    f.active = Some(1);
    assert_eq!(f.count(), "2 of 3");
}

/// A state showing one terminal tab whose cells already hold `output`.
fn shown(output: &[u8]) -> rune::Runtime<crate::State> {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    let mut rt = rune::Runtime::new(s, crate::view, (1280.0, 800.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    let mut term = Term::new(80, 24);
    term.advance(output);
    rt.update_if(move |s, _| {
        s.terms.views.insert("pty-1".into(), view(std::mem::replace(&mut term, Term::new(2, 1))));
        s.workspace.step(model::workspace::Event::AddTerminalTab("pty-1".into(), None));
        true
    });
    rt.frame(0.05);
    rt
}

#[test]
fn a_drag_on_the_screen_selects_and_the_modifier_c_copies() {
    let mut rt = shown(b"hello world\r\nsecond line");
    let (cx, cy) = rt.find("terminal-pty-1").expect("the terminal screen");
    let m = rt.state.terms.metrics();
    let rows = 24.0;
    let (x0, y0) = (cx - 40.0 * m.cell_w, cy - rows / 2.0 * m.cell_h);
    rt.pointer_down(x0 + 1.0, y0 + m.cell_h / 2.0);
    rt.pointer_move(x0 + 3.0 * m.cell_w, y0 + m.cell_h * 1.2);
    rt.pointer_move(x0 + 6.0 * m.cell_w + 1.0, y0 + m.cell_h * 1.5);
    rt.pointer_up(x0 + 6.0 * m.cell_w + 1.0, y0 + m.cell_h * 1.5);
    rt.frame(0.048);
    let selected = |rt: &rune::Runtime<crate::State>| {
        let view = &rt.state.terms.views["pty-1"];
        let term = view.cells.lock().unwrap();
        view.selection.and_then(|h| h.current(&term)).map(|s| s.text(term.lines()))
    };
    assert_eq!(selected(&rt).as_deref(), Some("hello world\nsecond"));
    // Ctrl+C off macOS arrives as a command key with `cmd` set: it copies and keeps the selection.
    let mods = rune::Mods { cmd: true, ctrl: true, ..rune::Mods::default() };
    rt.key(rune::KeyEvent { key: rune::Key::Command('c'), mods });
    rt.frame(0.064);
    assert_eq!(selected(&rt).as_deref(), Some("hello world\nsecond"), "a copy is not input");
}

#[test]
fn a_click_on_a_url_asks_before_opening_it() {
    let mut rt = shown(b"see https://example.com/a ok");
    let (cx, cy) = rt.find("terminal-pty-1").expect("the terminal screen");
    let m = rt.state.terms.metrics();
    let (x, y) = (cx - 40.0 * m.cell_w + 8.5 * m.cell_w, cy - 12.0 * m.cell_h + m.cell_h / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.048);
    assert!(rt.state.terms.views["pty-1"].hover.is_some(), "the link under the pointer is underlined");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.064);
    assert_eq!(rt.state.chat.link_confirm.as_deref(), Some("https://example.com/a"));
}
