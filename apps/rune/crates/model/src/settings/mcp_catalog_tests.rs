use serde_json::json;

use super::*;

fn row(name: &str) -> Row {
    serde_json::from_value(json!({ "name": name, "state": "installable" })).unwrap()
}

fn page(names: &[&str], next: Option<&str>) -> Page {
    Page { items: names.iter().map(|n| row(n)).collect(), next_cursor: next.map(str::to_owned), total: 120, updated_at: Some("2026-11-26T10:00:00Z".into()), sample: false }
}

#[test]
fn a_new_search_keeps_the_old_rows_until_it_answers_and_more_appends() {
    let mut s = Search::default();
    assert!(s.shown().is_none());
    s.page(page(&["io.github/a", "io.github/b"], Some("c1")), "", false);
    s.search();
    assert_eq!(s.shown().map(|r| r.rows.len()), Some(2), "the old rows stay while loading");
    s.page(page(&["io.github/a", "io.github/b"], Some("c1")), "", false);
    assert_eq!(s.more_cursor(), Some("c1"));
    s.set_more(More::Loading);
    s.page(page(&["io.github/c"], None), "", true);
    let shown = s.shown().unwrap();
    assert_eq!(shown.rows.iter().map(Row::display_name).collect::<Vec<_>>(), ["a", "b", "c"]);
    assert_eq!(s.more_cursor(), None);
}

#[test]
fn five_hundred_rows_stop_paging() {
    let mut s = Search::default();
    let names: Vec<String> = (0..MAX_ROWS).map(|i| format!("r/{i}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    s.page(page(&refs, Some("more")), "", false);
    assert_eq!(s.more_cursor(), None);
}

#[test]
fn the_words_follow_react() {
    assert!(too_many_terms("a b c d e f g h i"));
    assert!(!too_many_terms("  a  b c d e f g h "));
    assert_eq!(count(1_234_567), "1,234,567");
    assert_eq!(reason_text(&json!({ "code": "needs-runtime", "runtime": "uvx" })), "Install uv (uvx) on this computer to run it.");
    let results = Results { query: "git".into(), rows: vec![], total: 1, next_cursor: None, updated_at: None, sample: false };
    assert_eq!(summary(&results), "1 match");
}
