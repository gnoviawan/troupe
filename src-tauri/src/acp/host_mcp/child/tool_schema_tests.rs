//! Every tool the host `troupe` MCP server advertises has an input schema
//! strict providers accept. Gemini refuses a whole session when one array
//! in any tool's parameters has no typed `items` (issue #1981).

use serde_json::Value;

use super::tests::list_tools;
use super::ChildConfig;

/// Whether `schema` says what it is: a `type`, a `$ref`, an `anyOf` /
/// `oneOf` whose every alternative is typed (one bare `{}` accepts any
/// value), or an `allOf` with at least one typed member.
fn is_typed(schema: &Value) -> bool {
    let Some(map) = schema.as_object() else {
        return false;
    };
    if map.contains_key("type") || map.contains_key("$ref") {
        return true;
    }
    let alternatives = |key: &str| map.get(key).and_then(Value::as_array);
    for key in ["anyOf", "oneOf"] {
        if let Some(options) = alternatives(key) {
            return !options.is_empty() && options.iter().all(is_typed);
        }
    }
    alternatives("allOf").is_some_and(|parts| parts.iter().any(is_typed))
}

fn is_array_type(schema: &serde_json::Map<String, Value>) -> bool {
    match schema.get("type") {
        Some(Value::String(kind)) => kind == "array",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "array"),
        _ => false,
    }
}

/// Every array schema under `value` (properties, items, `$defs`,
/// `definitions`, combinators and anything else nested) whose `items` is
/// missing or untyped, by JSON pointer.
fn untyped_arrays(value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if is_array_type(map) && !map.get("items").is_some_and(is_typed) {
                out.push(if path.is_empty() {
                    "/".to_string()
                } else {
                    path.to_string()
                });
            }
            for (key, child) in map {
                untyped_arrays(child, &format!("{path}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                untyped_arrays(child, &format!("{path}/{index}"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn the_walk_flags_untyped_items_anywhere() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "ok": { "type": "array", "items": { "type": "string" } },
            "ref": { "type": "array", "items": { "$ref": "#/$defs/A" } },
            "bare": { "type": "array" },
            "empty": { "type": ["array", "null"], "items": {} },
            "nested": { "anyOf": [{ "type": "array", "items": true }, { "type": "null" }] },
            "open_any": { "type": "array", "items": { "anyOf": [{}, { "type": "string" }] } },
            "open_one": { "type": "array", "items": { "oneOf": [{ "type": "string" }, true] } },
            "str_or_null": {
                "type": "array",
                "items": { "anyOf": [{ "type": "string" }, { "type": "null" }] }
            },
            "all_of": { "type": "array", "items": { "allOf": [{}, { "$ref": "#/$defs/A" }] } }
        },
        "$defs": { "A": { "type": "object", "properties": { "x": { "type": "array" } } } }
    });
    let mut found = Vec::new();
    untyped_arrays(&schema, "", &mut found);
    found.sort();
    assert_eq!(
        found,
        [
            "/$defs/A/properties/x",
            "/properties/bare",
            "/properties/empty",
            "/properties/nested/anyOf/0",
            "/properties/open_any",
            "/properties/open_one",
        ]
    );
}

/// The full tool list a session sees: the static host tools, automation,
/// chat, install and subagent tools, figcraft's canvas tools and the
/// `browser` tool the parent describes.
#[test]
fn every_host_tool_input_schema_types_its_array_items() {
    let _guard = crate::browser_automation::TEST_HOST_LOCK.lock().unwrap();
    crate::browser_automation::clear_browser_host();
    let parent = crate::acp::host_mcp::parent::HostPlanServer::start(vec![], None);
    let (port, token, provisional) = parent.register_session("agent-1");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let tools = runtime.block_on(list_tools(ChildConfig {
        port,
        token,
        session_id: provisional,
        agent_id: "agent-1".to_string(),
    }));

    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "plan",
        "ask_user",
        "automation_apply_ops",
        "read_chat",
        "start_chat",
        "mcp_install",
        "subagent_start",
        "create_node",
        "browser",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} not served: {names:?}"
        );
    }

    let mut bad = Vec::new();
    for tool in &tools {
        let name = tool["name"].as_str().unwrap_or("?");
        let schema = &tool["inputSchema"];
        assert!(schema.is_object(), "{name} has no inputSchema");
        let mut found = Vec::new();
        untyped_arrays(schema, "", &mut found);
        bad.extend(found.into_iter().map(|path| format!("{name}: {path}")));
    }
    assert!(bad.is_empty(), "arrays without typed items: {bad:#?}");
}
