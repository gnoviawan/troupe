use troupe_editor::Rope;

use super::{Block, Kind, at_line, blocks, properties, source};

fn split(text: &str) -> Vec<(Kind, std::ops::Range<usize>)> {
    blocks(&Rope::from_str(text)).into_iter().map(|b| (b.kind, b.lines)).collect()
}

#[test]
fn headings_prose_and_fences_split_on_their_own_lines() {
    let text = "# Title\nSome text\nmore text\n\n- a\n- b\n## Next\n```rust\nfn a() {}\n\n```\n> quote\n";
    assert_eq!(
        split(text),
        [
            (Kind::Heading(1), 0..1),
            (Kind::Prose, 1..3),
            (Kind::Prose, 4..6),
            (Kind::Heading(2), 6..7),
            (Kind::Fence { mermaid: false }, 7..11),
            (Kind::Prose, 11..12),
        ]
    );
}

#[test]
fn front_matter_only_counts_at_the_top_and_mermaid_fences_are_diagrams() {
    let text = "---\ntitle: A\ntags: [x]\n---\n\n~~~ Mermaid\ngraph TD\n  A-->B\n~~~\n\n---\n";
    assert_eq!(
        split(text),
        [(Kind::Frontmatter, 0..4), (Kind::Fence { mermaid: true }, 5..9), (Kind::Prose, 10..11)]
    );
}

#[test]
fn an_unclosed_fence_runs_to_the_end_and_a_shorter_closer_does_not_close_it() {
    assert_eq!(split("````\na\n```\nb\n"), [(Kind::Fence { mermaid: false }, 0..5)]);
    assert_eq!(split("#hashtag\n####### seven\n"), [(Kind::Prose, 0..2)]);
}

#[test]
fn source_and_lookup_cover_exactly_the_block_lines() {
    let rope = Rope::from_str("a\nb\n\n# c\n");
    let all = blocks(&rope);
    assert_eq!(source(&rope, &all[0]), "a\nb");
    assert_eq!(source(&rope, &all[1]), "# c");
    assert_eq!(at_line(&all, 1), Some(0));
    assert_eq!(at_line(&all, 2), None);
    assert_eq!(at_line(&all, 3), Some(1));
    assert_eq!(all[1], Block { kind: Kind::Heading(1), lines: 3..4 });
}

#[test]
fn front_matter_properties_read_top_level_keys() {
    let props = properties("---\ntitle: \"Plan\"\ntags:\n  - a\nurl: http://x:1\n---");
    assert_eq!(
        props,
        [("title".into(), "Plan".into()), ("tags".into(), String::new()), ("url".into(), "http://x:1".into())]
    );
}
