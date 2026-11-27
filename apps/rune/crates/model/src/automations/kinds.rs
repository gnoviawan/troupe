//! The kinds the add menu offers (`canvas/palette/node-kinds.ts`): each kind's group and the
//! spec a fresh node starts with, which always deserializes on the host, so a new node saves
//! and its empty fields come back as issues.

use serde_json::json;

use super::ops::output_ports;
use super::types::{
    AutomationNode, ConditionOp, Graph, HttpMethod, MergeMode, NodeKind, NodeSpec, Port,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Actions,
    Logic,
    Data,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Actions, Group::Logic, Group::Data];

    pub fn label(self) -> &'static str {
        match self {
            Group::Actions => "Actions",
            Group::Logic => "Logic",
            Group::Data => "Data",
        }
    }
}

pub fn group(kind: NodeKind) -> Group {
    match kind {
        NodeKind::Http | NodeKind::Agent | NodeKind::McpTool => Group::Actions,
        NodeKind::Transform => Group::Data,
        _ => Group::Logic,
    }
}

/// What a card and the menus call the kind (`NODE_KIND_PRESENTATION`).
pub fn label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Http => "HTTP",
        NodeKind::Agent => "Agent",
        NodeKind::Transform => "Transform",
        NodeKind::Condition => "Condition",
        NodeKind::Fail => "Fail",
        NodeKind::ForEach => "For each",
        NodeKind::Merge => "Merge",
        NodeKind::Wait => "Wait",
        NodeKind::Approval => "Approval",
        NodeKind::McpTool => "MCP tool",
    }
}

pub fn fresh_spec(kind: NodeKind) -> NodeSpec {
    match kind {
        NodeKind::Http => NodeSpec::Http {
            method: HttpMethod::Get,
            url: String::new(),
            headers: Vec::new(),
            body: None,
            allowed_hosts: Vec::new(),
        },
        NodeKind::Agent => NodeSpec::Agent {
            agent_id: String::new(),
            model: None,
            prompt: String::new(),
            output_schema: json!({ "type": "string" }),
            timeout_ms: 120_000,
            max_attempts: 1,
            tools: Vec::new(),
            mcp_servers: None,
        },
        NodeKind::McpTool => NodeSpec::McpTool {
            server_id: String::new(),
            tool: String::new(),
            arguments: Vec::new(),
            timeout_ms: 60_000,
        },
        NodeKind::Condition => NodeSpec::Condition {
            left: String::new(),
            op: ConditionOp::Eq,
            right: Some(String::new()),
        },
        NodeKind::ForEach => NodeSpec::ForEach {
            items: String::new(),
            max_iterations: 10,
        },
        NodeKind::Merge => NodeSpec::Merge {
            mode: MergeMode::Any,
        },
        NodeKind::Wait => NodeSpec::Wait {
            duration_ms: 60_000,
        },
        NodeKind::Fail => NodeSpec::Fail {
            message: "Stopped".to_owned(),
        },
        // The host accepts 1 minute to 7 days; a day leaves time to answer.
        NodeKind::Approval => NodeSpec::Approval {
            message: "Approve this run?".to_owned(),
            timeout_ms: 86_400_000,
        },
        NodeKind::Transform => NodeSpec::Transform { fields: Vec::new() },
    }
}

/// The port a step continues on when it is chained in: `out`, or the first non-error port.
pub fn chain_port(kind: NodeKind) -> Option<Port> {
    let ports = output_ports(Some(kind));
    if ports.contains(&Port::Out) {
        return Some(Port::Out);
    }
    ports.iter().copied().find(|p| *p != Port::Error)
}

/// A fresh node of `kind` with an id and a label no other step of `graph` uses.
pub fn new_node(kind: NodeKind, graph: &Graph) -> AutomationNode {
    let id_of = |i: usize| {
        if i == 1 {
            kind.as_str().to_owned()
        } else {
            format!("{}_{i}", kind.as_str())
        }
    };
    let label_of = |i: usize| {
        if i == 1 {
            label(kind).to_owned()
        } else {
            format!("{} {i}", label(kind))
        }
    };
    let mut n = 1;
    while graph
        .nodes
        .iter()
        .any(|node| node.id == id_of(n) || node.label == label_of(n))
    {
        n += 1;
    }
    AutomationNode {
        id: id_of(n),
        label: label_of(n),
        spec: fresh_spec(kind),
    }
}
