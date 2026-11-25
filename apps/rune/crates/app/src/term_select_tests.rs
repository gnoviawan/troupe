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
