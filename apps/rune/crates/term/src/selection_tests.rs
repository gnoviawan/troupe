use crate::Term;
use crate::selection::{Selection, Unit, word_at};
use crate::text::Point;

fn screen_point(t: &Term, row: usize, col: usize) -> Point {
    Point::new(t.screen_start() + row, col)
}

#[test]
fn a_drag_across_a_wrapped_line_and_a_wide_character_copies_one_line() {
    // 8 columns: "hello wo", "rld 中文" and "!" are one wrapped line.
    let mut t = Term::new(8, 6);
    t.advance("hello world 中文!\r\nnext\r\n".as_bytes());
    let mut s = Selection::new(screen_point(&t, 0, 6), Unit::Cell);
    s.head = screen_point(&t, 3, 2);
    assert_eq!(s.text(t.lines()), "world 中文!\nne");
}

#[test]
fn a_selection_ending_on_a_wide_character_takes_the_whole_character() {
    let mut t = Term::new(10, 2);
    t.advance("ab中文cd".as_bytes());
    let mut s = Selection::new(screen_point(&t, 0, 1), Unit::Cell);
    s.head = screen_point(&t, 0, 4);
    assert_eq!(s.text(t.lines()), "b中");
}

#[test]
fn rows_are_trimmed_after_their_last_written_cell_and_joined_by_newlines() {
    // Printed spaces are content, as in xterm.js; the cells after them are not.
    let mut t = Term::new(10, 3);
    t.advance(b"one  \x1b[1D\x1b[K\r\ntwo\r\nthree");
    let mut s = Selection::new(screen_point(&t, 0, 0), Unit::Cell);
    s.head = screen_point(&t, 2, 10);
    assert_eq!(s.text(t.lines()), "one \ntwo\nthree");
}

#[test]
fn a_double_click_selects_a_word_and_a_triple_click_its_wrapped_line() {
    let mut t = Term::new(8, 3);
    t.advance(b"ls (src/main.rs)");
    let at = screen_point(&t, 0, 5);
    assert_eq!(word_at(t.lines(), at), (screen_point(&t, 0, 4), screen_point(&t, 0, 8)));
    let word = Selection::new(screen_point(&t, 1, 2), Unit::Word);
    assert_eq!(word.text(t.lines()), "main.rs");
    let line = Selection::new(screen_point(&t, 1, 0), Unit::Line);
    assert_eq!(line.text(t.lines()), "ls (src/main.rs)");
}

#[test]
fn a_press_without_a_drag_selects_nothing() {
    let t = Term::new(8, 3);
    let s = Selection::new(screen_point(&t, 0, 1), Unit::Cell);
    assert_eq!(s.range(t.lines()), None);
    assert_eq!(s.text(t.lines()), "");
}

#[test]
fn a_selection_follows_lines_the_scrollback_drops() {
    let s = Selection { anchor: Point::new(5, 1), head: Point::new(7, 2), unit: Unit::Cell };
    assert_eq!(s.shift(2).map(|s| (s.anchor, s.head)), Some((Point::new(3, 1), Point::new(5, 2))));
    assert_eq!(s.shift(6), None, "its start scrolled out of the buffer");
}
