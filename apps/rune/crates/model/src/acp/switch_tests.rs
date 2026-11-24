//! The chain edges a history open heals (`healChainEdges`). The recorder
//! cannot show them: a payload with switch markers or a continuation runs
//! the switch redirect and re-splice, which are not ported.

use serde_json::{Value, json};

use super::State;

fn state_with_index(ids: &[(&str, &str)]) -> State {
    let session_index = ids
        .iter()
        .map(|(id, project)| {
            let Value::Object(row) = json!({ "id": id, "projectId": project }) else {
                unreachable!()
            };
            row
        })
        .collect();
    State {
        session_index,
        ..State::default()
    }
}

fn edges(state: &State) -> Vec<(String, Option<String>)> {
    state
        .session_index
        .iter()
        .map(|row| {
            let id = row["id"].as_str().unwrap_or_default().to_owned();
            let from = row
                .get("continuedFrom")
                .and_then(Value::as_str)
                .map(str::to_owned);
            (id, from)
        })
        .collect()
}

#[test]
fn a_history_install_stamps_its_own_edge_and_its_markers_edges() {
    let mut state = state_with_index(&[("s0", "p1"), ("s1", "p1"), ("s3", "p1")]);
    let payload = json!({
        "metadata": { "id": "s1", "agentId": "a", "cwd": "/w", "continuedFrom": "s0", "createdAt": 1 },
        "messages": [],
        "switches": [
            { "id": "switch:seq-9", "toConfigId": "codex", "newSessionId": "s3", "seq": 9 },
            { "id": "switch:seq-10", "toConfigId": "codex", "newSessionId": "", "seq": 10 }
        ]
    });
    let mut fx = Vec::new();
    assert!(state.install_history("s1", &payload, &mut fx).is_some());
    assert_eq!(
        edges(&state),
        vec![
            ("s3".into(), Some("s1".into())),
            ("s1".into(), Some("s0".into())),
            ("s0".into(), None),
        ]
    );
    assert_eq!(state.book.history_watermarks.get("s1"), Some(&10.0));
}
