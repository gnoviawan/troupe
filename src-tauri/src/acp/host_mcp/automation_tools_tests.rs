use serde_json::{json, Value};

use super::automation_tools::{AutomationApplyOpsInput, GUIDE};
use crate::automation::model::{IssueCode, NodeSpec, Port};

/// The wire names a unit enum's JSON schema lists.
fn variants<T: schemars::JsonSchema>() -> Vec<String> {
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    let mut names = Vec::new();
    collect_enum_names(&schema, &mut names);
    names
}

fn collect_enum_names(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("enum") {
                out.extend(items.iter().filter_map(|v| v.as_str().map(str::to_string)));
            }
            if let Some(Value::String(name)) = map.get("const") {
                out.push(name.clone());
            }
            map.values().for_each(|v| collect_enum_names(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_enum_names(v, out)),
        _ => {}
    }
}

/// Node kinds (with their method, op and mode values), ports and issue
/// codes all come from the model, so a new variant the guide does not
/// mention fails here.
#[test]
fn the_guide_names_every_node_kind_port_and_issue_code() {
    let names: Vec<String> = [
        variants::<NodeSpec>(),
        variants::<Port>(),
        variants::<IssueCode>(),
    ]
    .concat();
    for sample in ["for_each", "wait", "item", "merge_needs_two_inputs"] {
        assert!(names.iter().any(|n| n == sample), "schema lost {sample}");
    }
    let missing: Vec<&String> = names.iter().filter(|n| !GUIDE.contains(n.as_str())).collect();
    assert!(missing.is_empty(), "automation_guide does not mention {missing:?}");
}

/// `ops` items are typed objects, so strict providers (Gemini) accept the
/// tool, and the field keeps its description (issue #1981).
#[test]
fn apply_ops_advertises_ops_as_an_array_of_objects() {
    let schema = serde_json::to_value(schemars::schema_for!(AutomationApplyOpsInput)).unwrap();
    let ops = &schema["properties"]["ops"];
    assert_eq!(ops["type"], "array");
    assert_eq!(ops["items"], json!({ "type": "object" }));
    assert!(
        ops["description"]
            .as_str()
            .is_some_and(|d| d.contains("add_node")),
        "{ops}"
    );
}

/// The schema change leaves parsing as it was: each op stays a raw value
/// for `parse_ops`.
#[test]
fn apply_ops_input_still_takes_raw_op_values() {
    let input: AutomationApplyOpsInput = serde_json::from_value(json!({
        "id": "a1",
        "base_revision": 3,
        "ops": [{ "op": "rename", "name": "Daily" }, { "op": "remove_node", "id": "n" }],
    }))
    .unwrap();
    assert_eq!(input.base_revision, 3);
    assert_eq!(input.ops[0], json!({ "op": "rename", "name": "Daily" }));
    assert_eq!(input.ops.len(), 2);
}
