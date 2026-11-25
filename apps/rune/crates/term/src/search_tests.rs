use crate::Term;
use crate::links::link_at;
use crate::search::{Match, Query, compile, find_all, next_after, previous_before};
use crate::text::Point;

fn matches(t: &Term, query: Query) -> Vec<(usize, usize, usize, usize)> {
    let re = compile(&query).expect("the query compiles");
    find_all(t.lines(), &re, usize::MAX).into_iter().map(|m| (m.start.line, m.start.col, m.end.line, m.end.col)).collect()
}

fn plain(text: &str) -> Query {
    Query { text: text.into(), ..Query::default() }
}

#[test]
fn a_regex_finds_matches_in_the_scrollback_and_the_screen() {
    let mut t = Term::with_scrollback(10, 2, 100);
    t.advance(b"error 1\r\nok\r\nERROR 22\r\nok\r\nerror 333");
    assert_eq!(t.screen_start(), 3, "three lines are in the scrollback");
    let q = Query { text: r"error \d+".into(), regex: true, ..Query::default() };
    assert_eq!(matches(&t, q), [(0, 0, 0, 7), (2, 0, 2, 8), (4, 0, 4, 9)]);
}

#[test]
fn a_match_across_a_wrapped_row_ends_on_the_next_row() {
    let mut t = Term::new(6, 3);
    t.advance(b"abcdefghij");
    assert_eq!(matches(&t, plain("efgh")), [(0, 4, 1, 2)]);
}

#[test]
fn case_whole_word_and_wide_characters() {
    let mut t = Term::new(20, 2);
    t.advance("中文 Cat cat catalog".as_bytes());
    assert_eq!(matches(&t, plain("cat")).len(), 3, "plain search ignores case");
    assert_eq!(matches(&t, Query { case_sensitive: true, ..plain("cat") }), [(0, 9, 0, 12), (0, 13, 0, 16)]);
    assert_eq!(matches(&t, Query { whole_word: true, ..plain("cat") }), [(0, 5, 0, 8), (0, 9, 0, 12)]);
    assert_eq!(matches(&t, plain("文")), [(0, 2, 0, 4)], "a wide character covers two cells");
}

#[test]
fn next_and_previous_wrap_around() {
    let m = |line, col| Match { start: Point::new(line, col), end: Point::new(line, col + 1) };
    let all = [m(0, 3), m(2, 0), m(5, 1)];
    assert_eq!(next_after(&all, Point::new(1, 0)), Some(1));
    assert_eq!(next_after(&all, Point::new(6, 0)), Some(0));
    assert_eq!(previous_before(&all, Point::new(2, 0)), Some(0));
    assert_eq!(previous_before(&all, Point::new(0, 0)), Some(2));
    assert_eq!(next_after(&[], Point::new(0, 0)), None);
}

#[test]
fn an_invalid_regex_is_an_error_not_a_panic() {
    assert!(compile(&Query { text: "(".into(), regex: true, ..Query::default() }).is_err());
    assert!(compile(&plain("(")).is_ok(), "a plain query escapes its text");
}

#[test]
fn a_link_is_found_under_the_pointer_without_trailing_punctuation() {
    let mut t = Term::new(20, 3);
    t.advance(b"see https://example.com/a?b=1. ok");
    let row = t.screen_start();
    let link = link_at(t.lines(), Point::new(row, 25)).expect("a link on the second row");
    assert_eq!(link.url, "https://example.com/a?b=1");
    assert_eq!((link.at.start, link.at.end), (Point::new(row, 4), Point::new(row + 1, 9)));
    assert!(link_at(t.lines(), Point::new(row, 1)).is_none());
    assert!(link_at(t.lines(), Point::new(row + 1, 9)).is_none(), "the full stop is not part of it");
}

#[test]
fn find_all_stops_at_its_limit() {
    let mut t = crate::Term::with_scrollback(40, 5, 2000);
    for _ in 0..1500 {
        t.advance(b"ab\r\n");
    }
    let re = crate::search::compile(&crate::search::Query { text: "ab".into(), ..crate::search::Query::default() }).unwrap();
    assert_eq!(crate::search::find_all(t.lines(), &re, 1000).len(), 1000);
    assert_eq!(crate::search::find_all(t.lines(), &re, usize::MAX).len(), 1500);
}
