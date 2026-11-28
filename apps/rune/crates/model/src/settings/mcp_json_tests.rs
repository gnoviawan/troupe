use serde_json::json;

use super::*;
use crate::settings::mcp::{Draft, draft_from_server, unchecked_config};

#[test]
fn a_claude_desktop_map_imports_each_server_in_entry_order() {
    let text = r#"{"mcpServers": {"b": {"command": " npx ", "args": [" -y ", "pkg"], "env": {"TOKEN": "x", "N": 3}, "alwaysAllow": []}, "1": {"url": "https://mcp.example.com/mcp"}}}"#;
    let out = parse_import(text);
    assert_eq!(out.errors, Vec::<String>::new());
    let servers: Vec<_> = out.servers.into_iter().map(serde_json::Value::Object).collect();
    assert_eq!(
        servers,
        [
            json!({ "type": "http", "name": "1", "url": "https://mcp.example.com/mcp" }),
            json!({ "type": "stdio", "name": "b", "command": "npx", "args": ["-y", "pkg"], "env": [{ "name": "TOKEN", "value": "x" }, { "name": "N", "value": "3" }] }),
        ]
    );
}

#[test]
fn a_bad_server_is_reported_and_the_rest_still_import() {
    let text = r#"{"mcpServers": {"ok": {"command": "x"}, "bad": {"env": 3}, "noname": {"type": "sse"}, "list": []}}"#;
    let out = parse_import(text);
    assert_eq!(out.servers.len(), 1);
    assert_eq!(out.errors, ["bad: env must be an object map or name/value pairs", "noname: URL is required.", "list: expected a server object"]);
    assert_eq!(parse_import("[1]").errors, ["Invalid JSON: expected a top-level object"]);
    assert_eq!(parse_import(r#"{"mcpServers": 1}"#).errors, ["Invalid JSON: \"mcpServers\" must be an object"]);
    assert_eq!(parse_import(r#"{"command": "x"}"#).errors, ["<unnamed>: Name is required."]);
    assert!(parse_import("{").errors[0].starts_with("Invalid JSON: "));
}

#[test]
fn the_json_view_round_trips_with_the_form() {
    let server = json!({ "id": "s1", "type": "http", "name": "docs", "url": "https://mcp.example.com/sse", "headers": [{ "name": "Authorization", "value": "Bearer t" }], "enabled": false });
    let server = server.as_object().unwrap().clone();
    let text = server_to_json(&server);
    assert_eq!(text, "{\n  \"type\": \"http\",\n  \"name\": \"docs\",\n  \"url\": \"https://mcp.example.com/sse\",\n  \"headers\": [\n    {\n      \"name\": \"Authorization\",\n      \"value\": \"Bearer t\"\n    }\n  ],\n  \"enabled\": false\n}");
    assert_eq!(json_enabled(&text), Some(false));
    let back = parse_import(&text).servers.remove(0);
    let draft = draft_from_server(&back);
    let mut config = unchecked_config(&draft);
    config.insert("enabled".into(), json!(false));
    assert_eq!(server_to_json(&config), text);
}

#[test]
fn a_stdio_server_shows_env_as_a_map_and_drops_empty_lists() {
    let server = json!({ "type": "stdio", "name": "fs", "command": "npx", "args": [], "env": [{ "name": "ROOT", "value": "/tmp" }] });
    let text = server_to_json(server.as_object().unwrap());
    assert_eq!(text, "{\n  \"type\": \"stdio\",\n  \"name\": \"fs\",\n  \"command\": \"npx\",\n  \"env\": {\n    \"ROOT\": \"/tmp\"\n  },\n  \"enabled\": true\n}");
}

#[test]
fn edit_refuses_the_wrapper_and_a_blank_form_starts_empty() {
    assert!(has_wrapper(r#"{"mcpServers": null}"#));
    assert!(!has_wrapper(r#"{"command": "x"}"#));
    assert!(is_blank_draft(&Draft::default()));
    let mut draft = Draft::default();
    draft.local.command_line = "npx".into();
    assert!(!is_blank_draft(&draft));
}
