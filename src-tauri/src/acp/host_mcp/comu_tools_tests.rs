use serde_json::{json, Value};
use troupe_comu_engine::{ComuCallOutput, ComuContentBlock};

use super::comu_tools::{output_to_call_result, parse_tool_definitions, sanitize_schema_arrays};

#[test]
fn real_engine_tool_definitions_parse_with_strict_typed_array_items() {
    let raw = troupe_comu_engine::tool_definitions();
    assert_eq!(raw.len(), troupe_comu_engine::COMU_TOOL_PAIRS.len());

    let tools = parse_tool_definitions(raw);
    assert_eq!(tools.len(), troupe_comu_engine::COMU_TOOL_PAIRS.len());

    for tool in &tools {
        let value = serde_json::to_value(tool).unwrap();
        let name = value["name"].as_str().unwrap_or("?");
        assert!(name.starts_with("comu_"), "unprefixed tool name: {name}");
        assert!(
            !value["inputSchema"]["properties"]
                .as_object()
                .is_some_and(|p| p.contains_key("session")),
            "{name} leaked internal `session` property"
        );
    }
}

#[test]
fn sanitize_schema_arrays_fills_missing_or_empty_items() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "bare": { "type": "array" },
            "empty": { "type": "array", "items": {} },
            "already_ok": { "type": "array", "items": { "type": "integer" } }
        }
    });
    sanitize_schema_arrays(&mut schema);
    assert_eq!(schema["properties"]["bare"]["items"]["type"], "string");
    assert_eq!(schema["properties"]["empty"]["items"]["type"], "string");
    assert_eq!(
        schema["properties"]["already_ok"]["items"]["type"],
        "integer"
    );
}

#[test]
fn output_to_call_result_preserves_text_and_inline_image_blocks() {
    let output = ComuCallOutput {
        ok: true,
        blocks: vec![
            ComuContentBlock::Text {
                text: "window captured".to_string(),
            },
            ComuContentBlock::Image {
                data: "iVBORw0KGgo=".to_string(),
                mime_type: "image/png".to_string(),
            },
        ],
        structured: Some(json!({ "status": "completed" })),
    };

    let result = output_to_call_result(output);
    let serialized: Value = serde_json::to_value(&result).unwrap();
    assert_ne!(serialized["isError"], json!(true));
    let content = serialized["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert_eq!(content[1]["data"], "iVBORw0KGgo=");
}
