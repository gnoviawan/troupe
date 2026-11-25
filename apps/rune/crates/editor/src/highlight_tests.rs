use crate::Rope;
use crate::highlight::{Highlights, Tag, language_for_path, whole};

fn tags(spans: &[(std::ops::Range<usize>, Tag)], line: &str) -> Vec<(String, Tag)> {
    spans.iter().map(|(r, t)| (line[r.clone()].to_owned(), *t)).collect()
}

#[test]
fn languages_follow_the_react_editors_map() {
    assert_eq!(language_for_path("src/main.rs"), Some("rust"));
    assert_eq!(language_for_path("a/b.TSX"), Some("typescript"));
    assert_eq!(language_for_path("styles.scss"), Some("css"));
    assert_eq!(language_for_path("README.md"), Some("markdown"));
    assert_eq!(language_for_path("go.mod"), None);
    assert_eq!(language_for_path("main.go"), None);
    assert_eq!(language_for_path(".env"), None);
}

#[test]
fn rust_tokens_get_codemirrors_styles() {
    let line = "fn add(a: i32) -> i32 { a + 1 } // sum";
    let spans = &whole("x.rs", line)[0];
    let got = tags(spans, line);
    assert!(got.contains(&("fn".to_owned(), Tag::Keyword)), "{got:?}");
    assert!(got.contains(&("1".to_owned(), Tag::Number)), "{got:?}");
    assert!(got.contains(&("// sum".to_owned(), Tag::Comment)), "{got:?}");
}

#[test]
fn a_chunk_sees_a_comment_opened_above_it() {
    let mut text = String::from("/* open\n");
    for _ in 0..10 {
        text.push_str("still comment\n");
    }
    text.push_str("*/ fn x() {}\n");
    let rope = Rope::from_str(&text);
    let mut h = Highlights::for_path("a.rs");
    let spans = h.chunk(&rope, 5..8).to_vec();
    assert_eq!(spans.len(), 3);
    assert_eq!(tags(&spans[0], "still comment"), [("still comment".to_owned(), Tag::Comment)]);
}

#[test]
fn plain_files_have_no_spans() {
    let rope = Rope::from_str("package main\n");
    let mut h = Highlights::for_path("main.go");
    assert!(h.chunk(&rope, 0..1).is_empty());
    assert_eq!(h.language(), None);
}
