//! Troupe Comu (`comu_*` + `comu_guide`) agent surface in the host `troupe` MCP child.
//!
//! Tool definitions are fetched from the parent via `FrameKind::DescribeComu` so
//! the desktop serves the live platform driver schemas while headless
//! `troupe-server` serves lightweight fail-closed schemas without linking X11,
//! Wayland, or desktop toolkit symbols.
//!
//! Tool calls travel over `FrameKind::Comu` with a 64 MiB reply limit so
//! base64 PNG screenshots from `comu_get_window_state`, `comu_get_desktop_state`,
//! and `comu_zoom` never truncate.

use rmcp::model::{CallToolResult, ContentBlock, Tool};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::acp::host_mcp::child::ChildConfig;
use crate::acp::host_mcp::{FrameKind, FrameReply, FrameRequest};
use crate::comu::{ComuCallOutput, ComuContentBlock};

/// Maximum reply payload size for `FrameKind::Comu` (64 MiB covers multi-display PNG captures).
pub const MAX_COMU_REPLY_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum round-trip duration for a Comu call: matches the single shared
/// [`crate::comu::consent::CONSENT_WAIT_TIMEOUT`] deadline across all interactive
/// consent prompts plus 30s of driver execution headroom.
pub const COMU_ROUND_TRIP: std::time::Duration =
    crate::comu::consent::CONSENT_WAIT_TIMEOUT.saturating_add(std::time::Duration::from_secs(30));

/// Input schema for `comu_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ReadComuGuideInput {
    /// Optional guide topic (`overview`, `workflow`, `macos`, `windows`, `linux`,
    /// or `troubleshooting`). Omit to read the top-level Troupe Comu index and topic list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
}

/// Convert raw JSON tool definitions into strict `rmcp::model::Tool` entries
/// accepted by every provider (including Gemini's typed-array-items check).
#[must_use]
pub fn parse_tool_definitions(raw_definitions: Vec<Value>) -> Vec<Tool> {
    raw_definitions
        .into_iter()
        .map(|mut definition| {
            if let Some(schema) = definition.get_mut("inputSchema") {
                sanitize_schema_arrays(schema);
            }
            definition
        })
        .filter_map(|definition| match serde_json::from_value::<Tool>(definition) {
            Ok(tool) => Some(tool),
            Err(error) => {
                eprintln!("[host-mcp-child] skipped a comu tool definition: {error}");
                None
            }
        })
        .collect()
}

/// Ensure every `"type": "array"` schema node carries a typed `"items"` schema
/// so strict MCP clients (such as Gemini) never reject the session.
pub fn sanitize_schema_arrays(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if is_array_type(map) && !map.get("items").is_some_and(is_typed_schema) {
                map.insert("items".to_string(), json!({ "type": "string" }));
            }
            for child in map.values_mut() {
                sanitize_schema_arrays(child);
            }
        }
        Value::Array(items) => {
            for child in items.iter_mut() {
                sanitize_schema_arrays(child);
            }
        }
        _ => {}
    }
}

fn is_array_type(schema: &serde_json::Map<String, Value>) -> bool {
    match schema.get("type") {
        Some(Value::String(kind)) => kind == "array",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "array"),
        _ => false,
    }
}

fn is_typed_schema(schema: &Value) -> bool {
    let Some(map) = schema.as_object() else {
        return false;
    };
    if map.contains_key("type") || map.contains_key("$ref") {
        return true;
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(options) = map.get(key).and_then(Value::as_array) {
            return !options.is_empty() && options.iter().all(is_typed_schema);
        }
    }
    map.get("allOf")
        .and_then(Value::as_array)
        .is_some_and(|parts| parts.iter().any(is_typed_schema))
}

/// Fetch the `comu_*` tool definitions from the parent host.
pub async fn describe_tools(config: &ChildConfig) -> Vec<Tool> {
    let request = FrameRequest {
        token: config.token.clone(),
        session_id: config.session_id.clone(),
        kind: FrameKind::DescribeComu,
        ..FrameRequest::default()
    };
    let budget = crate::browser_automation::budgets::DESCRIBE;
    match tokio::time::timeout(budget, exchange_comu_frame(config, request)).await {
        Ok(Ok(FrameReply {
            ok: true,
            result: Some(Value::Array(defs)),
            ..
        })) => parse_tool_definitions(defs),
        _ => {
            eprintln!("[host-mcp-child] comu tools not served: describe_comu failed");
            Vec::new()
        }
    }
}

/// Execute one `comu_*` tool call against the parent host.
pub async fn call_tool(config: ChildConfig, name: String, arguments: Value) -> CallToolResult {
    let request = FrameRequest {
        token: config.token.clone(),
        session_id: config.session_id.clone(),
        kind: FrameKind::Comu,
        comu_tool: Some(name.clone()),
        comu_input: Some(arguments),
        ..FrameRequest::default()
    };
    let exchange =
        tokio::time::timeout(COMU_ROUND_TRIP, exchange_comu_frame(&config, request)).await;
    match exchange {
        Err(_) => CallToolResult::error(vec![ContentBlock::text(
            "the Troupe Comu call timed out waiting for the host",
        )]),
        Ok(Err(error)) => {
            eprintln!("[host-mcp-child] {name}: {error}");
            CallToolResult::error(vec![ContentBlock::text(format!(
                "the Troupe host is unreachable: {error}"
            ))])
        }
        Ok(Ok(reply)) => decode_comu_reply(reply),
    }
}

/// Convert a [`FrameReply`] carrying [`ComuCallOutput`] into an MCP [`CallToolResult`],
/// preserving inline `ContentBlock::image` screenshots alongside text and structured output.
#[must_use]
pub fn decode_comu_reply(reply: FrameReply) -> CallToolResult {
    if let Some(result_val) = reply.result {
        if let Ok(output) = serde_json::from_value::<ComuCallOutput>(result_val.clone()) {
            return output_to_call_result(output);
        }
        let text = match result_val {
            Value::String(s) => s,
            other => other.to_string(),
        };
        return if reply.ok {
            CallToolResult::success(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::error(vec![ContentBlock::text(text)])
        };
    }
    let msg = reply
        .error
        .unwrap_or_else(|| "Computer Use call failed".to_string());
    CallToolResult::error(vec![ContentBlock::text(msg)])
}

#[must_use]
pub fn output_to_call_result(output: ComuCallOutput) -> CallToolResult {
    let mut blocks: Vec<ContentBlock> = output
        .blocks
        .into_iter()
        .map(|block| match block {
            ComuContentBlock::Text { text } => ContentBlock::text(text),
            ComuContentBlock::Image { data, mime_type } => ContentBlock::image(data, mime_type),
        })
        .collect();

    if blocks.is_empty() {
        if let Some(structured) = &output.structured {
            blocks.push(ContentBlock::text(structured.to_string()));
        } else {
            blocks.push(ContentBlock::text(if output.ok { "ok" } else { "error" }));
        }
    }

    let mut result = if output.ok {
        CallToolResult::success(blocks)
    } else {
        CallToolResult::error(blocks)
    };
    if let Some(structured) = output.structured {
        result.structured_content = Some(structured);
    }
    result
}

async fn exchange_comu_frame(
    config: &ChildConfig,
    request: FrameRequest,
) -> Result<FrameReply, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", config.port))
        .await
        .map_err(|e| format!("connect to parent failed: {e}"))?;
    let mut buf = serde_json::to_vec(&request).map_err(|e| format!("encode frame: {e}"))?;
    buf.push(b'\n');
    stream
        .write_all(&buf)
        .await
        .map_err(|e| format!("write frame: {e}"))?;
    let mut reader = BufReader::new(stream.take(MAX_COMU_REPLY_BYTES));
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("read reply: {e}"))?;
    serde_json::from_str(&line).map_err(|e| format!("decode reply: {e}"))
}
