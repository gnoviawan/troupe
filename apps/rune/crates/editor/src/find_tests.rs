use crate::find::{self, Query};
use crate::{Command, Editor, Selection};

const FIXTURE: &str = include_str!("../fixtures/find.rs");

fn query(text: &str) -> Query {
    Query { text: text.to_owned(), ..Query::default() }
}

fn found(ed: &Editor, q: &Query) -> Vec<String> {
    find::matches(ed.rope(), q).ranges.into_iter().map(|r| ed.rope().slice(r).to_string()).collect()
}

#[test]
fn plain_text_matches_ignore_case_by_default() {
    let ed = Editor::new(FIXTURE);
    let m = find::matches(ed.rope(), &query("total"));
    assert_eq!(m.ranges, [3..8, 80..85, 217..222, 274..279, 396..401, 439..444, 455..460]);
    assert!(!m.capped);
    assert_eq!(find::matches(ed.rope(), &query("TOTAL")).ranges, m.ranges);
    assert_eq!(found(&ed, &query("Totals")), ["Totals", "totals"]);
    let sensitive = Query { case_sensitive: true, ..query("Totals") };
    assert_eq!(found(&ed, &sensitive), ["Totals"]);
}

#[test]
fn whole_words_skip_matches_inside_longer_words() {
    let ed = Editor::new(FIXTURE);
    let whole = Query { whole_word: true, ..query("total") };
    let ranges = find::matches(ed.rope(), &whole).ranges;
    assert_eq!(ranges, [217..222, 274..279, 439..444]);
    assert!(ranges.iter().all(|r| ed.rope().slice(r.clone()) == "total"));
}

#[test]
fn a_regex_query_matches_and_an_invalid_one_matches_nothing() {
    let ed = Editor::new(FIXTURE);
    let re = Query { regex: true, case_sensitive: true, ..query(r"pub (\w+) (\w+)") };
    assert_eq!(found(&ed, &re), ["pub struct Order", "pub struct Line", "pub fn total", "pub fn totals_match"]);
    let anchored = Query { regex: true, ..query(r"^impl") };
    assert_eq!(find::matches(ed.rope(), &anchored).ranges.len(), 1);
    let broken = Query { regex: true, ..query("(unclosed") };
    assert!(!broken.is_valid());
    assert!(find::matches(ed.rope(), &broken).ranges.is_empty());
}

#[test]
fn replace_all_rewrites_every_match_as_one_undo_step() {
    let mut ed = Editor::new(FIXTURE);
    let whole = Query { whole_word: true, case_sensitive: true, ..query("cents") };
    assert_eq!(find::replace_all(&mut ed, &whole, "price", 1.0), 3);
    let text = ed.text();
    assert!(text.contains("pub price: u64,"));
    assert!(text.contains("line.price * u64::from"));
    assert!(text.contains("every line's price times"));
    assert!(text.contains("total_cents"));
    assert_eq!(text.len(), FIXTURE.len());
    ed.apply(Command::Undo, 2.0);
    assert_eq!(ed.text(), FIXTURE);
}

#[test]
fn replace_all_expands_regex_groups() {
    let mut ed = Editor::new("a=1, b=2");
    let re = Query { regex: true, ..query(r"(\w)=(\d)") };
    assert_eq!(find::replace_all(&mut ed, &re, "$2:$1 ($&) $$", 0.0), 2);
    assert_eq!(ed.text(), "1:a (a=1) $, 2:b (b=2) $");
}

#[test]
fn replace_next_replaces_the_selected_match_and_selects_the_next() {
    let mut ed = Editor::new("one two one two one");
    let q = query("one");
    assert!(!find::replace_next(&mut ed, &q, "1", 0.0));
    assert_eq!(ed.selection(), Selection { anchor: 0, head: 3 });
    assert!(find::replace_next(&mut ed, &q, "1", 0.0));
    assert_eq!(ed.text(), "1 two one two one");
    assert_eq!(ed.selection(), Selection { anchor: 6, head: 9 });
}

#[test]
fn matches_stop_at_the_limit() {
    let ed = Editor::new(&"x ".repeat(1500));
    let m = find::matches(ed.rope(), &query("x"));
    assert_eq!((m.ranges.len(), m.capped), (find::MATCH_LIMIT, true));
}

#[test]
fn next_and_previous_wrap_around() {
    let ranges = [2..4, 8..10];
    assert_eq!(find::next_match(&ranges, 5, false), Some(8..10));
    assert_eq!(find::next_match(&ranges, 9, false), Some(2..4));
    assert_eq!(find::next_match(&ranges, 2, true), Some(8..10));
    assert_eq!(find::match_index(&ranges, &(8..10)), 2);
    assert_eq!(find::match_index(&ranges, &(8..9)), 0);
}

#[test]
fn go_to_line_reads_codemirrors_grammar() {
    let mut ed = Editor::new(FIXTURE);
    let line_start = |ed: &Editor, n: usize| ed.offset_of(n, 1);
    assert_eq!(find::go_to_line(&ed, "14"), Some(line_start(&ed, 14)));
    assert_eq!(find::go_to_line(&ed, "14:9"), Some(line_start(&ed, 14) + 8));
    assert_eq!(find::go_to_line(&ed, "50%"), Some(line_start(&ed, 12)));
    ed.apply(Command::Select(Selection::caret(line_start(&ed, 10))), 0.0);
    assert_eq!(find::go_to_line(&ed, "+3"), Some(line_start(&ed, 13)));
    assert_eq!(find::go_to_line(&ed, "-30"), Some(0));
    assert_eq!(find::go_to_line(&ed, "abc"), None);
    assert_eq!(find::go_to_line(&ed, ""), None);
}
