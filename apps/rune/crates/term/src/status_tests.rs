use crate::Term;

const A: &str = "\x1b]133;A;termul\x07";

fn finish(code: &str) -> String {
    format!("\x1b]133;D;{code};termul\x07")
}

/// Each line's text (trimmed) and status, for the lines that have either.
fn rows(t: &Term) -> Vec<(String, Option<i64>)> {
    t.lines()
        .iter()
        .map(|l| (l.cells.iter().map(crate::Cell::text).collect::<String>().trim().to_owned(), l.status))
        .filter(|(text, status)| !text.is_empty() || status.is_some())
        .collect()
}

fn term(bytes: &str) -> Term {
    let mut t = Term::new(40, 10);
    t.advance(bytes.as_bytes());
    t
}

#[test]
fn each_finished_command_marks_its_prompt_row_with_the_exit_code() {
    let t = term(&format!("{A}$ ls\r\nfile\r\n{}{A}$ false\r\n{}{A}$ ", finish("0"), finish("1")));
    assert_eq!(
        rows(&t),
        vec![("$ ls".into(), Some(0)), ("file".into(), None), ("$ false".into(), Some(1)), ("$".into(), None)]
    );
}

#[test]
fn the_command_row_is_the_first_non_blank_row_within_two_spacer_rows() {
    let t = term(&format!("{A}\r\n$ make\r\nbuilt\r\n{}", finish("2")));
    assert_eq!(rows(&t), vec![("$ make".into(), Some(2)), ("built".into(), None)]);
    let t = term(&format!("{A}\r\n\r\n\r\n$ late\r\n{}", finish("0")));
    assert_eq!(rows(&t), vec![("$ late".into(), None)], "three blank rows: past the spacer");
}

#[test]
fn only_troupe_marks_count_and_each_prompt_closes_once() {
    let t = term(&format!("\x1b]133;A\x07$ a\r\n\x1b]133;D;0\x07{}", finish("0")));
    assert_eq!(rows(&t), vec![("$ a".into(), None)], "plain 133 marks from prompt frameworks are ignored");
    let t = term(&format!("{A}$ b\r\n{}{}", finish("0"), finish("1")));
    assert_eq!(rows(&t), vec![("$ b".into(), Some(0))], "a second finish has no prompt to close");
    for bad in ["x", "", "12345678901", "1.5"] {
        let t = term(&format!("{A}$ c\r\n{}", finish(bad)));
        assert_eq!(rows(&t), vec![("$ c".into(), None)], "exit code {bad:?}");
    }
    let t = term(&format!("{A}$ d\r\n{}", finish("-1")));
    assert_eq!(rows(&t), vec![("$ d".into(), Some(-1))]);
}

#[test]
fn marks_on_the_alternate_screen_are_ignored() {
    let t = term(&format!("\x1b[?1049h{A}$ vim\r\n{}\x1b[?1049l", finish("0")));
    assert!(rows(&t).iter().all(|(_, s)| s.is_none()));
    let t = term(&format!("{A}$ vim\r\n\x1b[?1049h{}\x1b[?1049l{}", finish("1"), finish("0")));
    assert_eq!(rows(&t), vec![("$ vim".into(), Some(0))], "the finish after leaving closes the prompt");
}

#[test]
fn a_command_that_erased_its_echo_gets_no_status() {
    let t = term(&format!("{A}$ fzf\x1b[2K\r{}", finish("0")));
    assert!(rows(&t).iter().all(|(_, s)| s.is_none()));
}

#[test]
fn the_status_scrolls_with_its_row_and_leaves_with_it() {
    let mut t = Term::with_scrollback(20, 3, 2);
    t.advance(format!("{A}$ one\r\n{}", finish("0")).as_bytes());
    t.advance(b"a\r\nb\r\nc\r\n");
    assert_eq!(rows(&t)[0], ("$ one".into(), Some(0)), "kept in the scrollback");
    t.advance(b"d\r\ne\r\nf\r\n");
    assert!(rows(&t).iter().all(|(_, s)| s.is_none()), "dropped with its line");
    t.advance(format!("{A}$ two\r\n{}", finish("3")).as_bytes());
    assert_eq!(rows(&t).iter().filter(|(_, s)| s.is_some()).count(), 1, "a recycled line starts clean");
    assert!(rows(&t).contains(&("$ two".into(), Some(3))));
}

#[test]
fn a_prompt_whose_line_scrolled_out_closes_without_a_status() {
    let mut t = Term::with_scrollback(20, 2, 1);
    t.advance(format!("{A}$ long\r\n1\r\n2\r\n3\r\n4\r\n{}", finish("0")).as_bytes());
    assert!(rows(&t).iter().all(|(_, s)| s.is_none()));
}
