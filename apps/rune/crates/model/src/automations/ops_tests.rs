//! `apply_ops`, `inverse_ops` and `refuse_connect` against `automation-ops.json`, the fixture
//! the host's `graph_fixture_tests.rs` and the web client's `apply-op.test.ts` also check.

use serde_json::{Value, json};

use super::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../../src/shared/types/__fixtures__/automation-ops.json"
    ))
    .expect("automation-ops.json")
}

fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).expect("fixture shape")
}

#[test]
fn every_apply_case_matches_the_host() {
    let f = fixture();
    let start: DraftDoc = parse(&f["start"]);
    let cases = f["apply"].as_array().expect("apply");
    assert_eq!(cases.len(), 27);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let ops: Vec<Op> = parse(&case["ops"]);
        let result = apply_ops(&start, &ops);
        if let Some(expect) = case.get("expect") {
            let doc = result.unwrap_or_else(|r| panic!("{name}: refused {r:?}"));
            assert_eq!(serde_json::to_value(&doc).expect("doc"), *expect, "{name}");
            let undo = inverse_ops(&start, &ops).unwrap_or_else(|| panic!("{name}: no inverse"));
            let back =
                apply_ops(&doc, &undo).unwrap_or_else(|r| panic!("{name}: inverse refused {r:?}"));
            let sorted = |d: &DraftDoc| {
                let mut nodes: Vec<String> =
                    d.graph.nodes.iter().map(|n| json!(n).to_string()).collect();
                let mut edges: Vec<String> =
                    d.graph.edges.iter().map(|e| json!(e).to_string()).collect();
                nodes.sort();
                edges.sort();
                (d.name.clone(), json!(d.trigger), d.isolation, nodes, edges)
            };
            assert_eq!(
                sorted(&back),
                sorted(&start),
                "{name}: the inverse restores the start"
            );
        } else {
            let rejection = result.expect_err(name);
            let got = serde_json::to_value(&rejection).expect("rejection");
            for (key, want) in case["reject"].as_object().expect("reject") {
                assert_eq!(&got[key], want, "{name}: {key}");
            }
        }
    }
}

#[test]
fn every_connect_case_matches_the_host() {
    let f = fixture();
    let start: DraftDoc = parse(&f["start"]);
    for case in f["connect"].as_array().expect("connect") {
        let name = case["name"].as_str().expect("name");
        let doc = match case.get("ops") {
            Some(ops) => apply_ops(&start, &parse::<Vec<Op>>(ops)).expect(name),
            None => start.clone(),
        };
        let edge: Edge = parse(&case["edge"]);
        let got =
            refuse_connect(&doc.graph, &edge).map(|r| serde_json::to_value(r).expect("refusal"));
        assert_eq!(got.unwrap_or(Value::Null), case["refuse"], "{name}");
    }
}

#[test]
fn node_ids_follow_the_host_pattern() {
    for ok in ["a", "fetch_2", "a234567890123456789012345678901"] {
        assert!(is_node_id(ok), "{ok}");
    }
    for bad in [
        "",
        "Shape",
        "2a",
        "trigger",
        "a-b",
        "a2345678901234567890123456789012x",
    ] {
        assert!(!is_node_id(bad), "{bad}");
    }
}
