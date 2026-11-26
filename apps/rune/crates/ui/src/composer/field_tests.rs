use rune::TextEdit;

use super::field::{TriggerKind, doc_of, edit_of, put_pill, trigger};
use super::{Doc, Part, Pill};

fn typed(text: &str) -> TextEdit {
    let mut edit = TextEdit::new("").rich();
    edit.insert(text);
    edit
}

fn skill() -> Pill {
    Pill::Skill {
        name: "matrix-review".into(),
        path: "/w/SKILL.md".into(),
    }
}

#[test]
fn a_document_survives_the_field() {
    let doc = Doc {
        parts: vec![
            Part::Text("Use ".into()),
            Part::Pill(skill()),
            Part::Text(" on\n".into()),
            Part::Pill(Pill::File {
                display: "a.ts".into(),
                abs_path: "/w/a.ts".into(),
            }),
            Part::Pill(Pill::Tool { id: "browser".into() }),
        ],
    };
    let edit = edit_of(&doc);
    assert_eq!(edit.text, "Use matrix-review on\na.tsBrowser");
    assert_eq!(doc_of(&edit), doc);
}

#[test]
fn a_slash_opens_only_at_a_word_start() {
    let t = trigger(&typed("hi /rev")).expect("slash");
    assert_eq!((t.kind, t.range, t.query.as_str()), (TriggerKind::Slash, 3..7, "rev"));
    assert_eq!(trigger(&typed("a/b")), None);
    assert_eq!(trigger(&typed("/rev ")), None);
    assert_eq!(trigger(&typed("/")).map(|t| t.query), Some(String::new()));
}

#[test]
fn an_at_sign_opens_the_mention_menu() {
    let t = trigger(&typed("see @al")).expect("mention");
    assert_eq!((t.kind, t.range, t.query.as_str()), (TriggerKind::Mention, 4..7, "al"));
    assert_eq!(trigger(&typed("me@x.com")), None);
}

#[test]
fn a_pick_swaps_the_trigger_for_a_pill_and_a_space() {
    let mut edit = typed("run /mat");
    let t = trigger(&edit).expect("slash");
    put_pill(&mut edit, t.range, &skill());
    assert_eq!(
        doc_of(&edit),
        Doc {
            parts: vec![Part::Text("run ".into()), Part::Pill(skill()), Part::Text(" ".into())]
        }
    );
    edit.insert("/x");
    let t = trigger(&edit).expect("a slash after the pill's space");
    assert_eq!(t.query, "x");
}

#[test]
fn a_trigger_right_after_a_pill_opens() {
    let mut edit = edit_of(&Doc {
        parts: vec![Part::Pill(skill())],
    });
    edit.insert("@q");
    assert_eq!(
        trigger(&edit).map(|t| (t.kind, t.query)),
        Some((TriggerKind::Mention, "q".into()))
    );
}
