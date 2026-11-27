use serde_json::Value;

use super::super::kinds::new_node;
use super::super::types::{Edge, NodeKind, Port};
use super::*;

fn head() -> Automation {
    let f: Value = serde_json::from_str(include_str!(
        "../../../../../../src/shared/types/__fixtures__/automation.json"
    ))
    .expect("automation.json");
    serde_json::from_value(f["automation"].clone()).expect("automation")
}

fn add(draft: &Draft) -> Draft {
    let node = new_node(NodeKind::Wait, &draft.doc.graph);
    let edge = Edge {
        from: "post".into(),
        port: Port::Out,
        to: node.id.clone(),
    };
    let gesture = plan_gesture(
        draft,
        vec![Op::AddNode { node }, Op::Connect { edge }],
        LayoutPatch::new(),
    )
    .expect("applies");
    reduce(draft, Action::Gesture(gesture))
}

#[test]
fn undo_back_to_the_head_leaves_a_clean_draft_and_redo_returns() {
    let opened = open(&head(), &[]);
    let edited = add(&opened);
    assert_eq!(edited.pending.len(), 1);
    assert_eq!(edited.doc.graph.nodes.len(), 7);
    let undone = reduce(&edited, Action::Undo);
    assert!(
        undone.pending.is_empty(),
        "the undone step is dropped, not inverted"
    );
    assert_eq!(undone.doc, opened.doc);
    let redone = reduce(&undone, Action::Redo);
    assert_eq!(redone.doc, edited.doc);
    assert_eq!(redone.pending.len(), 1);
}

#[test]
fn a_dry_run_for_an_older_pending_list_is_ignored() {
    let edited = add(&open(&head(), &[]));
    let issue = Issue {
        node_id: None,
        code: "cycle".into(),
        message: "m".into(),
    };
    let stale = reduce(
        &edited,
        Action::Previewed {
            pending_rev: edited.pending_rev + 1,
            issues: vec![issue.clone()],
        },
    );
    assert!(stale.issues.is_empty());
    let fresh = reduce(
        &edited,
        Action::Previewed {
            pending_rev: edited.pending_rev,
            issues: vec![issue],
        },
    );
    assert_eq!(fresh.issues.len(), 1);
}

#[test]
fn saving_our_own_revision_keeps_the_history() {
    let edited = add(&open(&head(), &[]));
    let mut saved = head();
    saved.revision += 1;
    saved.graph = edited.doc.graph.clone();
    let after = reduce(
        &edited,
        Action::Saved {
            automation: saved.clone(),
            issues: vec![],
        },
    );
    assert!(after.pending.is_empty());
    assert_eq!(after.undo.len(), 1);
    saved.revision += 1;
    let other = reduce(
        &edited,
        Action::Saved {
            automation: saved,
            issues: vec![],
        },
    );
    assert!(
        other.undo.is_empty(),
        "another author's revision drops the history"
    );
}
