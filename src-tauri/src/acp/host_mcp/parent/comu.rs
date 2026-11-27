//! `Comu` and `DescribeComu` frames handled by [`HostPlanServer`].

use std::future::Future;

use serde_json::Value;

use super::HostPlanServer;
use crate::acp::host_mcp::FrameReply;

impl HostPlanServer {
    pub(super) async fn comu_call(
        &self,
        session_id: &str,
        agent_id: &str,
        tool: Option<String>,
        input: Option<Value>,
        hung_up: impl Future<Output = ()>,
    ) -> FrameReply {
        let Some(tool_name) = tool.filter(|s| !s.trim().is_empty()) else {
            return FrameReply::err("comu frame requires `comu_tool`");
        };
        let args = input.unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        let output = tokio::select! {
            biased;
            () = hung_up => {
                log::info!(
                    "[host-mcp] comu caller disconnected before completion: tool={tool_name} agent={agent_id} session={}",
                    crate::logging::redact_session_id(session_id)
                );
                return FrameReply::err_code("cancelled", "caller disconnected");
            }
            out = crate::comu::dispatch(session_id, agent_id, &tool_name, args) => out,
        };
        let ok = output.ok;
        match serde_json::to_value(&output) {
            Ok(serialized) => {
                if ok {
                    FrameReply::ok_with(serialized)
                } else {
                    let code = output
                        .structured
                        .as_ref()
                        .and_then(|v| v.pointer("/refusal/code"))
                        .and_then(Value::as_str)
                        .unwrap_or("comu_error");
                    let msg = output
                        .structured
                        .as_ref()
                        .and_then(|v| v.pointer("/refusal/message"))
                        .and_then(Value::as_str)
                        .unwrap_or("Computer Use call failed");
                    FrameReply {
                        result: Some(serialized),
                        ..FrameReply::err_code(code, msg)
                    }
                }
            }
            Err(error) => {
                log::error!(
                    "[host-mcp] failed to serialize comu output for session {}: {error}",
                    crate::logging::redact_session_id(session_id)
                );
                FrameReply::err("failed to encode Computer Use output")
            }
        }
    }
}

pub(super) fn describe_comu_reply() -> FrameReply {
    FrameReply::ok_with(Value::Array(crate::comu::describe_tools()))
}
