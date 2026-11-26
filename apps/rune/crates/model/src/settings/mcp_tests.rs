use serde_json::json;

use super::*;

fn obj(value: serde_json::Value) -> Object {
    value.as_object().unwrap().clone()
}

#[test]
fn a_command_line_with_an_env_prefix_saves_the_prefix_as_env_rows() {
    let draft = Draft {
        kind: Kind::Local,
        name: " docs ".into(),
        local: Local { command_line: "API_KEY=abc npx -y 'my server'".into(), env: vec![Row::default()] },
        ..Draft::default()
    };
    let config = draft_to_config(&draft).unwrap();
    assert_eq!(
        serde_json::Value::Object(config),
        json!({"type": "stdio", "name": "docs", "command": "npx", "args": ["-y", "my server"], "env": [{"name": "API_KEY", "value": "abc"}]})
    );
}

#[test]
fn an_open_quote_and_a_half_filled_row_name_their_fields() {
    let draft = Draft {
        kind: Kind::Local,
        name: String::new(),
        local: Local { command_line: "npx 'oops".into(), env: vec![Row::new("TOKEN", "")] },
        ..Draft::default()
    };
    let errors = draft_to_config(&draft).unwrap_err();
    assert_eq!(
        errors,
        vec![
            (Field::Env, "Needs a value.".to_owned()),
            (Field::CommandLine, "Close the quote in the command.".to_owned()),
            (Field::Name, "Give the server a name.".to_owned()),
        ]
    );
}

#[test]
fn an_edit_round_trips_a_remote_server_with_a_token() {
    let server = obj(json!({"id": "s1", "type": "sse", "name": "Linear", "url": "https://mcp.linear.app/sse", "headers": [{"name": "Authorization", "value": "Bearer x"}], "enabled": true}));
    let draft = draft_from_server(&server);
    assert_eq!(draft.remote.auth, Auth::Token);
    assert!(!draft.remote.transport_pinned);
    let config = draft_to_config(&draft).unwrap();
    assert!(same_launch(&server, &config));
}

#[test]
fn urls_parse_as_the_url_constructor_does() {
    for (url, ok) in [
        ("https://mcp.example.com/mcp", true),
        ("http://localhost:3000/sse", true),
        ("localhost:3000", true),
        ("mcp.example.com", false),
        ("https://", false),
        ("http://a b/", false),
        ("http://host:99999/", false),
        ("http://[::1]:8080/mcp", true),
    ] {
        assert_eq!(url_parses(url), ok, "{url}");
    }
    assert_eq!(transport_for_url("https://sse"), Transport::Http);
    assert_eq!(transport_for_url("https://x.dev/SSE/?a=1"), Transport::Sse);
}

#[test]
fn the_status_word_follows_the_probe() {
    assert_eq!(status(false, true, Some("connected")).label, "Off");
    assert_eq!(status(true, true, None).label, "Checking…");
    assert_eq!(status(true, false, Some("authRequired")).tone, Tone::Warning);
    assert_eq!(tool_count(1), "1 tool");
}
