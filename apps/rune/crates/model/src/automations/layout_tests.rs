use serde_json::Value;

use super::*;

fn fixture_graph() -> Graph {
    let f: Value = serde_json::from_str(include_str!(
        "../../../../../../src/shared/types/__fixtures__/automation.json"
    ))
    .expect("automation.json");
    serde_json::from_value(f["automation"]["graph"].clone()).expect("graph")
}

#[test]
fn steps_read_in_run_order() {
    let g = fixture_graph();
    let order: Vec<&str> = step_order(&g).iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        order,
        ["fetch", "extract", "alert", "check", "shape", "post"]
    );
}

#[test]
fn saved_cards_stay_where_they_were_saved() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../../../../src/shared/types/__fixtures__/automation.json"
    ))
    .expect("automation.json");
    let layout: Layout = serde_json::from_value(f["automation"]["layout"].clone()).expect("layout");
    let placed = place_nodes(&fixture_graph(), &layout);
    for (id, at) in &layout {
        assert_eq!(placed[id], *at, "{id}");
    }
    assert_eq!(placed.len(), 7);
}

#[test]
fn a_for_each_body_is_everything_its_item_edges_reach() {
    let g: Graph = serde_json::from_value(serde_json::json!({
        "nodes": [
            { "id": "each", "label": "Each", "spec": { "kind": "for_each", "items": "x", "max_iterations": 3 } },
            { "id": "a", "label": "A", "spec": { "kind": "fail", "message": "m" } },
            { "id": "b", "label": "B", "spec": { "kind": "merge", "mode": "any" } },
            { "id": "c", "label": "C", "spec": { "kind": "wait", "duration_ms": 1 } }
        ],
        "edges": [
            { "from": "trigger", "port": "out", "to": "each" },
            { "from": "each", "port": "item", "to": "b" },
            { "from": "b", "port": "out", "to": "a" },
            { "from": "each", "port": "out", "to": "c" }
        ]
    }))
    .expect("graph");
    let bodies = loop_bodies(&g);
    assert_eq!(
        bodies["each"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let order: Vec<&str> = step_order(&g).iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        order,
        ["each", "b", "a", "c"],
        "a loop's body reads before what follows it"
    );
}

#[test]
fn an_edge_from_a_port_the_step_lacks_is_not_drawn() {
    let mut g = fixture_graph();
    g.edges.push(Edge {
        from: "alert".into(),
        port: Port::Out,
        to: "post".into(),
    });
    g.edges.push(Edge {
        from: "fetch".into(),
        port: Port::Out,
        to: "gone".into(),
    });
    assert_eq!(drawable_edges(&g).len(), 6);
}
