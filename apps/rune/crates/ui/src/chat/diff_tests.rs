use serde_json::Value;

use super::diff::{FOLD, Kind, Line, counts, first_change, lines};

const FIXTURES: &str = include_str!("../../../../../../src/renderer/components/chat/diff-line-counts.fixtures.json");

fn line(kind: Kind, text: &str, old_line: Option<usize>, new_line: Option<usize>) -> Line {
    Line { kind, text: text.to_owned(), old_line, new_line }
}

/// One side of a shared fixture, expanded per the file's `about`.
fn side(v: &Value) -> Option<String> {
    if v.is_null() {
        return None;
    }
    let eol = v.get("eol").and_then(Value::as_str).unwrap_or("\n");
    let mut text = String::new();
    for l in v["lines"].as_array().unwrap() {
        let expanded: Vec<String> = if let Some(s) = l.as_str() {
            vec![s.to_owned()]
        } else if let Some(times) = l.get("times").and_then(Value::as_u64) {
            vec![l["line"].as_str().unwrap().to_owned(); times as usize]
        } else {
            let prefix = l["prefix"].as_str().unwrap();
            let (from, to) = (l["from"].as_i64().unwrap(), l["to"].as_i64().unwrap());
            let range: Vec<i64> = if from <= to { (from..=to).collect() } else { (to..=from).rev().collect() };
            range.into_iter().map(|n| format!("{prefix}{n}")).collect()
        };
        for l in expanded {
            text.push_str(&l);
            text.push_str(eol);
        }
    }
    Some(text)
}

#[test]
fn counts_match_the_renderer_on_the_shared_fixtures() {
    let parsed: Value = serde_json::from_str(FIXTURES).unwrap();
    for f in parsed["fixtures"].as_array().unwrap() {
        let (old, new) = (side(&f["oldText"]), side(&f["newText"]).unwrap_or_default());
        let got = counts(&lines(old.as_deref(), &new));
        let want = (f["expected"]["added"].as_u64().unwrap() as usize, f["expected"]["removed"].as_u64().unwrap() as usize);
        assert_eq!(got, want, "{}", f["name"]);
    }
}

#[test]
fn a_change_keeps_three_lines_of_context_and_folds_the_rest() {
    let old: String = (1..=12).map(|n| format!("l{n}\n")).collect();
    let new = old.replace("l6\n", "six\n").replace("l12\n", "twelve\n");
    assert_eq!(
        lines(Some(&old), &new),
        [
            line(Kind::Context, "l3", Some(3), Some(3)),
            line(Kind::Context, "l4", Some(4), Some(4)),
            line(Kind::Context, "l5", Some(5), Some(5)),
            line(Kind::Removed, "l6", Some(6), None),
            line(Kind::Added, "six", None, Some(6)),
            line(Kind::Context, "l7", Some(7), Some(7)),
            line(Kind::Context, "l8", Some(8), Some(8)),
            line(Kind::Context, "l9", Some(9), Some(9)),
            line(Kind::Context, "l10", Some(10), Some(10)),
            line(Kind::Context, "l11", Some(11), Some(11)),
            line(Kind::Removed, "l12", Some(12), None),
            line(Kind::Added, "twelve", None, Some(12)),
        ]
    );
    let far: String = (1..=20).map(|n| format!("l{n}\n")).collect();
    let edited = far.replace("l2\n", "two\n").replace("l19\n", "nineteen\n");
    let shown = lines(Some(&far), &edited);
    assert_eq!(shown.iter().filter(|l| l.text == FOLD).count(), 1, "one fold between distant hunks");
}

#[test]
fn new_and_emptied_files_and_the_first_change() {
    assert_eq!(lines(None, "a\r\nb"), [line(Kind::Added, "a", None, Some(1)), line(Kind::Added, "b", None, Some(2))]);
    assert_eq!(lines(Some("a"), ""), [line(Kind::Removed, "a", Some(1), None)]);
    let edit = lines(Some("a\nb\nc\nd\n"), "a\nb\nC\nd\n");
    assert_eq!(first_change(&edit), Some((2, 3)));
    assert_eq!(first_change(&lines(Some("a\nb\n"), "a\n")), Some((1, 2)));
    assert_eq!(first_change(&lines(Some("a\n"), "a\n")), None);
}

#[test]
fn a_rebuild_reuses_the_diff_and_colours_of_unchanged_texts() {
    let (old, new) = ("a\nb\n", "a\nB\n");
    let first = super::diff::lines_of(Some(old), new);
    assert!(std::rc::Rc::ptr_eq(&first, &super::diff::lines_of(Some(old), new)), "the same texts diff once");
    assert!(!std::rc::Rc::ptr_eq(&first, &super::diff::lines_of(None, new)), "a new file is another diff");
    let t = theme::tokens("termul", true);
    let colours = super::diff::Colours::of_texts(t, "a.rs", Some(old), new);
    assert!(std::rc::Rc::ptr_eq(&colours, &super::diff::Colours::of_texts(t, "a.rs", Some(old), new)), "the same file highlights once");
}
