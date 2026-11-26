//! The command editor's session calls on `/terminal/ws` (`lib/command-editor-transport.ts`):
//! the request each one sends and how its reply reads. They are WebSocket requests, not HTTP
//! routes, so no `X-Command-Id` applies; the host keys them by the frame's own id.

use serde_json::{Value, json};

use super::completion::{Completion, Reply, Request};
use super::store::RpcResult;

/// One session call and its payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    Submit { terminal_id: String, draft: String },
    RequestCompletion { terminal_id: String, request: Request },
    CancelCompletion { terminal_id: String, request_id: String },
    RestorePrompt { terminal_id: String },
}

impl Call {
    /// The frame's `type`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Call::Submit { .. } => "command_editor_submit",
            Call::RequestCompletion { .. } => "command_editor_request_completion",
            Call::CancelCompletion { .. } => "command_editor_cancel_completion",
            Call::RestorePrompt { .. } => "command_editor_restore_prompt",
        }
    }

    /// The frame's `payload`, keys in the order the web transport writes them.
    #[must_use]
    pub fn payload(&self) -> Value {
        match self {
            Call::Submit { terminal_id, draft } => json!({ "terminalId": terminal_id, "draft": draft }),
            Call::RequestCompletion { terminal_id, request } => {
                let mut payload = serde_json::to_value(request).unwrap_or_else(|_| json!({}));
                if let Some(map) = payload.as_object_mut() {
                    map.insert("terminalId".into(), json!(terminal_id));
                }
                payload
            }
            Call::CancelCompletion { terminal_id, request_id } => json!({ "terminalId": terminal_id, "requestId": request_id }),
            Call::RestorePrompt { terminal_id } => json!({ "terminalId": terminal_id }),
        }
    }

    /// Whether the request is sent at all: a completion's numbers must be safe integers.
    #[must_use]
    pub fn sendable(&self) -> bool {
        const MAX_SAFE: u64 = (1 << 53) - 1;
        match self {
            Call::RequestCompletion { request, .. } => [request.buffer_revision, request.cursor, request.prompt_generation].iter().all(|&n| n <= MAX_SAFE),
            _ => true,
        }
    }
}

/// A failed request: its error code, `NETWORK_ERROR` when it was sent and no reply came.
pub type Failure = String;

/// What a reply to a session call means. A reply that does not read, or a request sent without
/// one, may have written: `Partial`. A request that never left: `PreWrite`.
fn fallback(reply: &Result<Value, Failure>) -> RpcResult {
    match reply {
        Ok(_) => RpcResult::Partial,
        Err(code) if code == "NETWORK_ERROR" => RpcResult::Partial,
        Err(_) => RpcResult::PreWrite,
    }
}

fn rpc_result(value: &Value) -> Option<RpcResult> {
    serde_json::from_value(value.clone()).ok()
}

/// The reply to a submit, cancel or restore.
#[must_use]
pub fn read_result(reply: &Result<Value, Failure>) -> RpcResult {
    reply.as_ref().ok().and_then(rpc_result).unwrap_or_else(|| fallback(reply))
}

fn safe(value: &Value) -> Option<u64> {
    value.as_u64().filter(|&n| n < (1 << 53))
}

fn read_completion_value(value: &Value) -> Option<Reply> {
    if let Some(result) = rpc_result(value) {
        return Some(Reply { result, outcome: None, completion: None });
    }
    let reply = value.as_object()?;
    let result = rpc_result(reply.get("result")?)?;
    if !reply.contains_key("outcome") && !reply.contains_key("completion") {
        return Some(Reply { result, outcome: None, completion: None });
    }
    match reply.get("outcome").and_then(Value::as_str) {
        Some("Completed") => {
            let c = reply.get("completion")?.as_object()?;
            let text = |k: &str| c.get(k).and_then(Value::as_str).map(str::to_owned);
            let completion = Completion {
                request_id: text("request_id")?,
                buffer_revision: safe(c.get("buffer_revision")?)?,
                cursor: safe(c.get("cursor")?)?,
                prompt_generation: safe(c.get("prompt_generation")?)?,
                cwd: text("cwd")?,
                output: text("output")?,
            };
            Some(Reply { result, outcome: Some("Completed".into()), completion: Some(completion) })
        }
        Some(outcome @ ("TimedOut" | "Cancelled" | "Dropped")) if !reply.contains_key("completion") => {
            Some(Reply { result, outcome: Some(outcome.into()), completion: None })
        }
        _ => None,
    }
}

/// The reply to a completion request.
#[must_use]
pub fn read_completion(reply: &Result<Value, Failure>) -> Reply {
    reply.as_ref().ok().and_then(read_completion_value).unwrap_or_else(|| Reply { result: fallback(reply), outcome: None, completion: None })
}
