//! The host's automation contract (`src/shared/types/automation.types.ts`, itself the mirror of
//! `src-tauri/src/automation/model.rs`). Field and tag names are the serde names on the wire.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The trigger card's id, beside the node ids.
pub const TRIGGER: &str = "trigger";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    SharedInRoot,
    WorktreePerRun,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    Cron {
        expr: String,
        timezone: String,
    },
    /// Integers on the wire: the host's `every_ms: u64, anchor_ms: i64`.
    Every {
        every_ms: u64,
        anchor_ms: i64,
    },
    At {
        at: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    Schedule { schedule: Schedule },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
    #[serde(rename = "PATCH")]
    Patch,
    #[serde(rename = "DELETE")]
    Delete,
}

impl HttpMethod {
    pub const ALL: [HttpMethod; 5] = [
        HttpMethod::Get,
        HttpMethod::Post,
        HttpMethod::Put,
        HttpMethod::Patch,
        HttpMethod::Delete,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedTemplate {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    Eq,
    Ne,
    Contains,
    Gt,
    Lt,
    Exists,
}

impl ConditionOp {
    pub const ALL: [ConditionOp; 6] = [
        ConditionOp::Eq,
        ConditionOp::Ne,
        ConditionOp::Contains,
        ConditionOp::Gt,
        ConditionOp::Lt,
        ConditionOp::Exists,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ConditionOp::Eq => "eq",
            ConditionOp::Ne => "ne",
            ConditionOp::Contains => "contains",
            ConditionOp::Gt => "gt",
            ConditionOp::Lt => "lt",
            ConditionOp::Exists => "exists",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTool {
    Read,
    Search,
    Edit,
    Execute,
    Fetch,
    Other,
}

impl AgentTool {
    pub const ALL: [AgentTool; 6] = [
        AgentTool::Read,
        AgentTool::Search,
        AgentTool::Edit,
        AgentTool::Execute,
        AgentTool::Fetch,
        AgentTool::Other,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    Any,
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeSpec {
    Http {
        method: HttpMethod,
        url: String,
        headers: Vec<NamedTemplate>,
        body: Option<String>,
        allowed_hosts: Vec<String>,
    },
    Agent {
        agent_id: String,
        model: Option<String>,
        prompt: String,
        /// `JsonSchemaSubset`, kept as sent.
        output_schema: Value,
        timeout_ms: u64,
        max_attempts: u64,
        tools: Vec<AgentTool>,
        mcp_servers: Option<Vec<String>>,
    },
    Transform {
        fields: Vec<NamedTemplate>,
    },
    Condition {
        left: String,
        op: ConditionOp,
        right: Option<String>,
    },
    Fail {
        message: String,
    },
    ForEach {
        items: String,
        max_iterations: u64,
    },
    Merge {
        mode: MergeMode,
    },
    Wait {
        duration_ms: u64,
    },
    Approval {
        message: String,
        timeout_ms: u64,
    },
    McpTool {
        server_id: String,
        tool: String,
        arguments: Vec<NamedTemplate>,
        timeout_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Http,
    Agent,
    Transform,
    Condition,
    Fail,
    ForEach,
    Merge,
    Wait,
    Approval,
    McpTool,
}

impl NodeKind {
    /// In the order `NODE_KINDS` lists them.
    pub const ALL: [NodeKind; 10] = [
        NodeKind::Http,
        NodeKind::Agent,
        NodeKind::McpTool,
        NodeKind::Condition,
        NodeKind::ForEach,
        NodeKind::Merge,
        NodeKind::Wait,
        NodeKind::Fail,
        NodeKind::Approval,
        NodeKind::Transform,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Http => "http",
            NodeKind::Agent => "agent",
            NodeKind::Transform => "transform",
            NodeKind::Condition => "condition",
            NodeKind::Fail => "fail",
            NodeKind::ForEach => "for_each",
            NodeKind::Merge => "merge",
            NodeKind::Wait => "wait",
            NodeKind::Approval => "approval",
            NodeKind::McpTool => "mcp_tool",
        }
    }
}

impl NodeSpec {
    pub fn kind(&self) -> NodeKind {
        match self {
            NodeSpec::Http { .. } => NodeKind::Http,
            NodeSpec::Agent { .. } => NodeKind::Agent,
            NodeSpec::Transform { .. } => NodeKind::Transform,
            NodeSpec::Condition { .. } => NodeKind::Condition,
            NodeSpec::Fail { .. } => NodeKind::Fail,
            NodeSpec::ForEach { .. } => NodeKind::ForEach,
            NodeSpec::Merge { .. } => NodeKind::Merge,
            NodeSpec::Wait { .. } => NodeKind::Wait,
            NodeSpec::Approval { .. } => NodeKind::Approval,
            NodeSpec::McpTool { .. } => NodeKind::McpTool,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationNode {
    pub id: String,
    pub label: String,
    pub spec: NodeSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Port {
    Out,
    Error,
    True,
    False,
    Item,
}

impl Port {
    pub fn as_str(self) -> &'static str {
        match self {
            Port::Out => "out",
            Port::Error => "error",
            Port::True => "true",
            Port::False => "false",
            Port::Item => "item",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub port: Port,
    pub to: String,
}

impl Edge {
    /// The canvas id, `from:port->to`; a run's edge marks use the same key.
    pub fn key(&self) -> String {
        format!("{}:{}->{}", self.from, self.port.as_str(), self.to)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<AutomationNode>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&AutomationNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// Card positions by node id (and `trigger`). Not part of the revision.
pub type Layout = BTreeMap<String, NodePosition>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LastRun {
    pub run_id: String,
    pub status: RunStatus,
    pub finished_at: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub revision: u64,
    pub approved_revision: Option<u64>,
    pub enabled: bool,
    pub trigger: Trigger,
    pub graph: Graph,
    pub layout: Layout,
    pub isolation: Isolation,
    pub next_run_at: Option<f64>,
    pub consecutive_errors: u64,
    pub schedule_error_count: u64,
    pub unseen_failures: u64,
    pub last_run: Option<LastRun>,
    pub created_at: f64,
    pub updated_at: f64,
    pub deleted_at: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    AddNode {
        node: AutomationNode,
    },
    UpdateNode {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spec: Option<NodeSpec>,
    },
    RemoveNode {
        id: String,
    },
    Connect {
        edge: Edge,
    },
    Disconnect {
        edge: Edge,
    },
    SetTrigger {
        trigger: Trigger,
    },
    Rename {
        name: String,
    },
    SetIsolation {
        isolation: Isolation,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub node_id: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentMcpServer {
    pub server_id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentPolicy {
    pub node_id: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub timeout_ms: u64,
    pub tools: Vec<AgentTool>,
    pub mcp_servers: Vec<AgentMcpServer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpToolPolicy {
    pub node_id: String,
    pub server_id: String,
    pub server_name: Option<String>,
    pub tool: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeSecrets {
    pub node_id: String,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeHosts {
    pub node_id: String,
    pub hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunPolicy {
    pub trigger_summary: String,
    pub next_runs: Vec<f64>,
    pub agents: Vec<AgentPolicy>,
    pub secrets: Vec<NodeSecrets>,
    pub outbound_hosts: Vec<NodeHosts>,
    pub mcp_tools: Vec<McpToolPolicy>,
    pub untrusted_flow: bool,
    pub failure_policy: String,
}

/// An automation with what its editor and approval card render.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    pub automation: Automation,
    pub issues: Vec<Issue>,
    pub policy: RunPolicy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApplyOpsResult {
    pub automation: Automation,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    Scheduled,
    CatchUp,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parked {
    pub node_id: String,
    pub iteration: Option<u64>,
    pub until: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub automation_id: String,
    pub revision: u64,
    pub graph: Graph,
    pub trigger: RunTrigger,
    pub scheduled_for: f64,
    pub status: RunStatus,
    pub started_at: f64,
    pub finished_at: Option<f64>,
    pub stop_reason: Option<String>,
    pub summary: Option<String>,
    pub conversation_id: Option<String>,
    pub worktree_id: Option<String>,
    pub parked: Option<Parked>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Succeeded,
    Failed,
    Skipped,
    Waiting,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub run_id: String,
    pub node_id: String,
    pub iteration: Option<u64>,
    pub status: StepStatus,
    #[serde(default)]
    pub output: Value,
    pub error: Option<String>,
    pub started_at: Option<f64>,
    pub finished_at: Option<f64>,
    pub wait_until: Option<f64>,
}

/// An agent this host can launch for an agent node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnownAgent {
    pub agent_id: String,
    pub name: String,
}

/// Wire event `automation_changed`; `automation_id` is null for project-wide changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Changed {
    pub project_id: String,
    pub automation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_by: Option<String>,
}
