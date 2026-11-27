use model::github::{CheckRollup, ItemKind, ItemState, Label, ListItem};
use rune::Color;

use super::*;

fn item() -> ListItem {
    ListItem {
        kind: ItemKind::Pr,
        number: 7,
        title: "Fix it".into(),
        state: ItemState::Draft,
        author: "ann".into(),
        labels: vec![Label { name: "bug".into(), color: "d73a4a".into() }, Label { name: "p1".into(), color: "zz".into() }],
        assignees: vec![],
        comment_count: 2,
        updated_at: String::new(),
        url: String::new(),
        checks: Some(CheckRollup::Failure),
    }
}

#[test]
fn a_rows_name_repeats_checks_comments_and_labels() {
    assert_eq!(row_name(&item()), "Draft pull request #7: Fix it, checks failed, 2 comments, labels bug, p1");
}

#[test]
fn issues_never_read_as_merged_or_draft() {
    assert_eq!(item_look(ItemKind::Issue, ItemState::Merged).0, "Closed");
    assert_eq!(item_look(ItemKind::Issue, ItemState::Draft).0, "Open");
    assert_eq!(item_look(ItemKind::Pr, ItemState::Closed).2, Tone::Destructive);
}

#[test]
fn a_dark_label_on_a_dark_surface_gets_readable_ink() {
    let surface = rune::color::hex("#18181b").unwrap();
    let (ink, _, _) = label_colors("1d1d5a", surface).unwrap();
    let chip = surface.mix(rune::color::hex("#1d1d5a").unwrap(), 0.14);
    assert!(contrast(ink, chip) >= 4.5, "contrast {}", contrast(ink, chip));
    assert_eq!(label_colors("zz", surface), None);
    assert_eq!(label_colors("1d1d5a", Color::WHITE).map(|(_, wash, _)| wash.a), Some(0.14));
}
