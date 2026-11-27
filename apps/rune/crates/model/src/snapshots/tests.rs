use serde_json::{Value, json};

use super::{Effect, Event, Op, Read, State, age, iso};

fn list(ids: &[&str]) -> Value {
    let snapshots: Vec<Value> = ids
        .iter()
        .map(|id| json!({ "id": id, "projectId": "p1", "name": id, "createdAt": "2026-11-19T08:30:00.000Z", "terminals": [], "activeTerminalId": null }))
        .collect();
    json!({ "snapshots": snapshots, "updatedAt": "2026-11-19T09:00:00.000Z" })
}

fn loaded(ids: &[&str]) -> State {
    let mut state = State::default();
    state.step(Event::Load("p1".into()));
    state.step(Event::Loaded { project_id: "p1".into(), read: Read::Found(list(ids)) });
    state
}

fn written_ids(fx: &[Effect]) -> Vec<Vec<String>> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::WriteSnapshots { record, .. } => Some(
                record["snapshots"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_owned()).collect(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn a_rename_waits_for_the_delete_before_it_and_cannot_bring_the_snapshot_back() {
    let mut state = loaded(&["a", "b"]);
    let first = state.step(Event::Delete("a".into()));
    assert!(matches!(&first[..], [Effect::Read { .. }]));
    let second = state.step(Event::Rename { id: "b".into(), name: "B".into() });
    assert!(second.is_empty(), "the rename waits: {second:?}");

    let fx = state.step(Event::Read { project_id: "p1".into(), read: Read::Found(list(&["a", "b"])) });
    assert_eq!(written_ids(&fx), vec![vec!["b".to_owned()]]);
    let fx = state.step(Event::Written { project_id: "p1".into(), result: Ok(()) });
    assert!(fx.iter().any(|e| matches!(e, Effect::Settled { op: Op::Delete, .. })));
    assert!(matches!(fx.last(), Some(Effect::Read { .. })), "the rename reads next: {fx:?}");

    let fx = state.step(Event::Read { project_id: "p1".into(), read: Read::Found(list(&["b"])) });
    assert_eq!(written_ids(&fx), vec![vec!["b".to_owned()]]);
    state.step(Event::Written { project_id: "p1".into(), result: Ok(()) });
    let names: Vec<&str> = state.snapshots.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["B"]);
    assert!(!state.busy("p1"));
}

#[test]
fn a_delete_acts_on_the_picked_snapshot_only() {
    let mut state = loaded(&["a", "b", "c"]);
    state.step(Event::Delete("b".into()));
    let fx = state.step(Event::Read { project_id: "p1".into(), read: Read::Found(list(&["a", "b", "c"])) });
    assert_eq!(written_ids(&fx), vec![vec!["a".to_owned(), "c".to_owned()]]);
    let ids: Vec<&str> = state.snapshots.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["a", "c"]);
}

#[test]
fn a_reply_for_no_running_mutation_changes_nothing() {
    let mut state = loaded(&["a"]);
    let before = state.clone();
    assert!(state.step(Event::Read { project_id: "p1".into(), read: Read::Found(list(&[])) }).is_empty());
    assert!(state.step(Event::Written { project_id: "p1".into(), result: Ok(()) }).is_empty());
    assert_eq!(state, before);
}

#[test]
fn iso_matches_to_iso_string() {
    assert_eq!(iso(0.0), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso(1_795_168_800_000.0), "2026-11-20T10:00:00.000Z");
    assert_eq!(iso(951_868_800_123.0), "2000-03-01T00:00:00.123Z");
}

#[test]
fn ages_read_as_the_web_client_writes_them() {
    let h = 3_600_000.0;
    assert_eq!(age(0.0, 0.5 * h), "Just now");
    assert_eq!(age(0.0, 5.5 * h), "5 hours ago");
    assert_eq!(age(0.0, 30.0 * h), "Yesterday");
    assert_eq!(age(0.0, 75.0 * h), "3 days ago");
}
