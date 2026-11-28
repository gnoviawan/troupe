//! The MCP server dialog's JSON view: the pasted-config parser (`lib/mcp-json-import.ts`) and
//! the server as the view shows it (`serverToJson`, `jsonEnabled` and `isBlankDraft` in
//! `McpServerDialog.tsx`).

use serde_json::{Map, Value, json};

use super::mcp::{Draft, Issue, Object, transport_of, trim, validate};
use crate::acp::js::string_of;

/// `MCP_VALIDATION_MESSAGES`.
fn issue_text(issue: Issue) -> &'static str {
    match issue {
        Issue::NameRequired => "Name is required.",
        Issue::CommandRequired => "Command is required for stdio.",
        Issue::UrlRequired => "URL is required.",
        Issue::UrlInvalid => "URL is invalid.",
    }
}

/// `FORM_NEEDS_ONE_SERVER`.
pub const FORM_NEEDS_ONE_SERVER: &str = "The form holds one valid server. Fix the JSON, or save it from here.";
pub const NO_SERVERS: &str = "No MCP servers found in the JSON.";
pub const EDIT_WRAPPER: &str = "Edit expects a single server object — remove the \"mcpServers\" wrapper.";
pub const EDIT_ONE: &str = "Edit expects exactly one server object.";
/// The textarea's placeholder.
pub const PLACEHOLDER: &str = r#"{"mcpServers": {"dokploy": {"command": "npx", "args": ["-y", "@dokploy/mcp"], "env": {"DOKPLOY_URL": "..."}}}}"#;

/// `parseMcpJsonImport`'s answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Import {
    pub servers: Vec<Object>,
    pub errors: Vec<String>,
}

/// `Object.entries` order: array-index keys first, ascending, then the rest as inserted.
fn js_entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let index = |k: &str| (k == "0" || (!k.starts_with('0') && k.bytes().all(|b| b.is_ascii_digit()))).then(|| k.parse::<u32>().ok().filter(|n| *n < u32::MAX)).flatten();
    let mut indexed: Vec<_> = map.iter().filter_map(|(k, v)| index(k).map(|n| (n, k, v))).collect();
    indexed.sort_by_key(|(n, _, _)| *n);
    indexed.into_iter().map(|(_, k, v)| (k, v)).chain(map.iter().filter(|(k, _)| index(k).is_none())).collect()
}

fn pair(name: &str, value: &str) -> Value {
    json!({ "name": name, "value": value })
}

/// `normalizeNameValuePairs`: a map or `[{name, value}]`; `Err` for any other shape.
fn pairs(value: Option<&Value>) -> Result<Option<Vec<Value>>, ()> {
    match value {
        None => Ok(None),
        Some(Value::Object(map)) => Ok(Some(js_entries(map).into_iter().map(|(k, v)| pair(k, &string_of(Some(v)))).collect())),
        Some(Value::Array(items)) => items
            .iter()
            .map(|e| match (e.get("name"), e.get("value")) {
                (Some(Value::String(n)), Some(Value::String(v))) if e.is_object() => Ok(pair(n, v)),
                _ => Err(()),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(()),
    }
}

/// `buildServer`: the known fields only; `None` for a wrong-shaped command, url or args.
fn build(raw: &Map<String, Value>, name: &str, env: Option<Vec<Value>>, headers: Option<Vec<Value>>) -> Option<Object> {
    let text = |k: &str| raw.get(k).and_then(Value::as_str);
    let filled = |k: &str| text(k).is_some_and(|s| !trim(s).is_empty());
    let kind = match text("type") {
        Some(t @ ("stdio" | "http" | "sse")) => t,
        _ if filled("command") => "stdio",
        _ if filled("url") => "http",
        _ => "stdio",
    };
    let mut server = Map::new();
    server.insert("type".into(), json!(kind));
    server.insert("name".into(), json!(name));
    if kind == "stdio" {
        if raw.get("command").is_some_and(|c| !c.is_string()) {
            return None;
        }
        let args = match raw.get("args") {
            None => None,
            Some(Value::Array(items)) => Some(items.iter().map(|a| a.as_str().map(|s| json!(trim(s)))).collect::<Option<Vec<_>>>()?),
            Some(_) => return None,
        };
        if let Some(command) = text("command") {
            server.insert("command".into(), json!(trim(command)));
        }
        if let Some(args) = args.filter(|a| !a.is_empty()) {
            server.insert("args".into(), Value::Array(args));
        }
        if let Some(env) = env.filter(|e| !e.is_empty()) {
            server.insert("env".into(), Value::Array(env));
        }
        return Some(server);
    }
    if raw.get("url").is_some_and(|u| !u.is_string()) {
        return None;
    }
    if let Some(url) = text("url") {
        server.insert("url".into(), json!(trim(url)));
    }
    if let Some(headers) = headers.filter(|h| !h.is_empty()) {
        server.insert("headers".into(), Value::Array(headers));
    }
    Some(server)
}

/// `parseMcpJsonImport`: a Claude Desktop `mcpServers` map or one bare server. Each server that
/// fails is reported and skipped; the rest import.
#[must_use]
pub fn parse_import(text: &str) -> Import {
    let fail = |e: String| Import { servers: vec![], errors: vec![e] };
    let parsed: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return fail(format!("Invalid JSON: SyntaxError: {e}")),
    };
    let Value::Object(top) = &parsed else { return fail("Invalid JSON: expected a top-level object".into()) };
    let entries: Vec<(String, &Value)> = match top.get("mcpServers") {
        Some(Value::Object(map)) => js_entries(map).into_iter().map(|(k, v)| (k.clone(), v)).collect(),
        Some(_) => return fail("Invalid JSON: \"mcpServers\" must be an object".into()),
        None => vec![(String::new(), &parsed)],
    };
    let mut out = Import::default();
    for (key, raw) in entries {
        let name = match raw {
            Value::Object(m) if key.is_empty() => m.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
            _ => key,
        };
        let shown = if name.is_empty() { "<unnamed>" } else { name.as_str() };
        let Value::Object(raw) = raw else {
            out.errors.push(format!("{}: expected a server object", if name.is_empty() { "<unknown>" } else { name.as_str() }));
            continue;
        };
        let Ok(env) = pairs(raw.get("env")) else {
            out.errors.push(format!("{shown}: env must be an object map or name/value pairs"));
            continue;
        };
        let Ok(headers) = pairs(raw.get("headers")) else {
            out.errors.push(format!("{shown}: headers must be an object map or name/value pairs"));
            continue;
        };
        let Some(server) = build(raw, &name, env, headers) else {
            out.errors.push(format!("{shown}: invalid server configuration"));
            continue;
        };
        let issues = validate(&server);
        if !issues.is_empty() {
            out.errors.push(format!("{shown}: {}", issues.into_iter().map(issue_text).collect::<Vec<_>>().join(" ")));
            continue;
        }
        out.servers.push(server);
    }
    out
}

/// `serverToJson`: what the JSON view starts with. `enabled` is shown unless it is `false`.
#[must_use]
pub fn server_to_json(server: &Object) -> String {
    let enabled = server.get("enabled") != Some(&json!(false));
    let list = |k: &str| server.get(k).and_then(Value::as_array).filter(|a| !a.is_empty());
    let mut out = Map::new();
    let kind = transport_of(server).to_owned();
    if kind == "stdio" {
        out.insert("type".into(), json!("stdio"));
        out.insert("name".into(), server.get("name").cloned().unwrap_or(Value::Null));
        out.insert("command".into(), server.get("command").cloned().unwrap_or(Value::Null));
        if let Some(args) = list("args") {
            out.insert("args".into(), Value::Array(args.clone()));
        }
        let mut env = Map::new();
        for p in server.get("env").and_then(Value::as_array).into_iter().flatten() {
            env.insert(p["name"].as_str().unwrap_or_default().to_owned(), p.get("value").cloned().unwrap_or(Value::Null));
        }
        if !env.is_empty() {
            out.insert("env".into(), Value::Object(env));
        }
    } else {
        out.insert("type".into(), json!(kind));
        out.insert("name".into(), server.get("name").cloned().unwrap_or(Value::Null));
        out.insert("url".into(), server.get("url").cloned().unwrap_or(Value::Null));
        if let Some(headers) = list("headers") {
            out.insert("headers".into(), Value::Array(headers.clone()));
        }
    }
    out.insert("enabled".into(), json!(enabled));
    let out: Map<String, Value> = out.into_iter().filter(|(_, v)| !v.is_null()).collect();
    serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default()
}

/// `jsonEnabled`: a top-level boolean `enabled`, if the text has one.
#[must_use]
pub fn json_enabled(text: &str) -> Option<bool> {
    serde_json::from_str::<Value>(text).ok()?.as_object()?.get("enabled")?.as_bool()
}

/// Whether the text is an object with an `mcpServers` key (`saveEdit`'s wrapper check).
#[must_use]
pub fn has_wrapper(text: &str) -> bool {
    serde_json::from_str::<Value>(text).ok().is_some_and(|v| v.get("mcpServers").is_some())
}

/// `isBlankDraft`: nothing typed yet, so Import JSON starts empty.
#[must_use]
pub fn is_blank_draft(draft: &Draft) -> bool {
    let has_text = |rows: &[super::mcp::Row]| rows.iter().any(|r| !trim(&r.name).is_empty() || !trim(&r.value).is_empty());
    trim(&draft.name).is_empty() && trim(&draft.remote.url).is_empty() && trim(&draft.local.command_line).is_empty() && !has_text(&draft.remote.headers) && !has_text(&draft.local.env)
}

#[cfg(test)]
#[path = "mcp_json_tests.rs"]
mod tests;
