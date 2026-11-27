//! The local mirror of the host's `graph::apply_ops` (`canvas/draft/apply-op.ts`): the same op
//! semantics and the same rejections, so a gesture draws before the host answers. The host
//! stays the authority; its dry run and Save decide. `inverse_ops` is `inverse.ts`, and the
//! port table is `ports.ts`.

use serde::{Deserialize, Serialize};

use super::types::{Edge, Graph, Isolation, NodeKind, Op, Port, TRIGGER, Trigger};

/// What an op batch reads and writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DraftDoc {
    pub name: String,
    pub trigger: Trigger,
    pub graph: Graph,
    pub isolation: Isolation,
}

/// Why the host refuses a batch, as `OpError` names it; `index` counts from 0 in the batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum OpRejection {
    EmptyBatch,
    NodeExists { index: usize, id: String },
    NodeNotFound { index: usize, id: String },
    InvalidNodeId { index: usize, id: String },
    EdgeExists { index: usize },
    EdgeNotFound { index: usize },
    EmptyName { index: usize },
}

impl OpRejection {
    pub fn code(&self) -> &'static str {
        match self {
            OpRejection::EmptyBatch => "empty_batch",
            OpRejection::NodeExists { .. } => "node_exists",
            OpRejection::NodeNotFound { .. } => "node_not_found",
            OpRejection::InvalidNodeId { .. } => "invalid_node_id",
            OpRejection::EdgeExists { .. } => "edge_exists",
            OpRejection::EdgeNotFound { .. } => "edge_not_found",
            OpRejection::EmptyName { .. } => "empty_name",
        }
    }
}

/// Legal output ports per kind (`graph.rs` `legal_ports`), top to bottom on the card.
pub fn output_ports(kind: Option<NodeKind>) -> &'static [Port] {
    match kind {
        None => &[Port::Out],
        Some(
            NodeKind::Http
            | NodeKind::Agent
            | NodeKind::Transform
            | NodeKind::Approval
            | NodeKind::McpTool,
        ) => &[Port::Out, Port::Error],
        Some(NodeKind::Condition) => &[Port::Error, Port::True, Port::False],
        Some(NodeKind::ForEach) => &[Port::Error, Port::Out, Port::Item],
        Some(NodeKind::Merge | NodeKind::Wait) => &[Port::Out],
        Some(NodeKind::Fail) => &[],
    }
}

/// `^[a-z][a-z0-9_]{0,31}$`, never `trigger`.
pub fn is_node_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
        && value != TRIGGER
}

/// Every node id an op names, where the host parses it.
fn named_ids(op: &Op) -> Vec<&str> {
    match op {
        Op::AddNode { node } => vec![&node.id],
        Op::UpdateNode { id, .. } | Op::RemoveNode { id } => vec![id],
        Op::Connect { edge } | Op::Disconnect { edge } => {
            if edge.from == TRIGGER {
                vec![&edge.to]
            } else {
                vec![&edge.from, &edge.to]
            }
        }
        _ => Vec::new(),
    }
}

fn apply_one(doc: &DraftDoc, op: &Op, index: usize) -> Result<DraftDoc, OpRejection> {
    if let Some(bad) = named_ids(op).into_iter().find(|id| !is_node_id(id)) {
        return Err(OpRejection::InvalidNodeId {
            index,
            id: bad.to_owned(),
        });
    }
    let mut next = doc.clone();
    let graph = &mut next.graph;
    let has = |id: &str| doc.graph.nodes.iter().any(|n| n.id == id);
    match op {
        Op::AddNode { node } => {
            if has(&node.id) {
                return Err(OpRejection::NodeExists {
                    index,
                    id: node.id.clone(),
                });
            }
            graph.nodes.push(node.clone());
        }
        Op::UpdateNode { id, label, spec } => {
            let Some(node) = graph.nodes.iter_mut().find(|n| n.id == *id) else {
                return Err(OpRejection::NodeNotFound {
                    index,
                    id: id.clone(),
                });
            };
            if let Some(label) = label {
                node.label = label.clone();
            }
            if let Some(spec) = spec {
                node.spec = spec.clone();
            }
        }
        Op::RemoveNode { id } => {
            if !has(id) {
                return Err(OpRejection::NodeNotFound {
                    index,
                    id: id.clone(),
                });
            }
            graph.nodes.retain(|n| n.id != *id);
            graph.edges.retain(|e| e.from != *id && e.to != *id);
        }
        Op::Connect { edge } => {
            if graph.edges.contains(edge) {
                return Err(OpRejection::EdgeExists { index });
            }
            graph.edges.push(edge.clone());
        }
        Op::Disconnect { edge } => {
            let Some(at) = graph.edges.iter().position(|e| e == edge) else {
                return Err(OpRejection::EdgeNotFound { index });
            };
            graph.edges.remove(at);
        }
        Op::SetTrigger { trigger } => next.trigger = trigger.clone(),
        Op::Rename { name } => {
            if name.trim().is_empty() {
                return Err(OpRejection::EmptyName { index });
            }
            next.name = name.clone();
        }
        Op::SetIsolation { isolation } => next.isolation = *isolation,
    }
    Ok(next)
}

/// Applies the whole batch or nothing, like the host.
pub fn apply_ops(doc: &DraftDoc, ops: &[Op]) -> Result<DraftDoc, OpRejection> {
    if ops.is_empty() {
        return Err(OpRejection::EmptyBatch);
    }
    let mut next = doc.clone();
    for (index, op) in ops.iter().enumerate() {
        next = apply_one(&next, op, index)?;
    }
    Ok(next)
}

/// Why the canvas refuses to draw an edge. The host saves any of these, but each comes back as
/// an issue (`illegal_port`, `cycle`) or an `edge_exists` rejection, so the canvas stops them
/// at the gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectRefusal {
    IllegalPort,
    DuplicateEdge,
    Cycle,
    UnknownNode,
}

impl ConnectRefusal {
    /// What the editor's toast says (`REFUSALS`).
    pub fn message(self) -> &'static str {
        match self {
            ConnectRefusal::IllegalPort => "That step has no such port.",
            ConnectRefusal::DuplicateEdge => "Those ports are already connected.",
            ConnectRefusal::Cycle => "That edge would make a loop.",
            ConnectRefusal::UnknownNode => "That step no longer exists.",
        }
    }
}

pub fn refuse_connect(graph: &Graph, edge: &Edge) -> Option<ConnectRefusal> {
    let kind_of = |id: &str| graph.node(id).map(|n| n.spec.kind());
    let source = if edge.from == TRIGGER {
        Some(None)
    } else {
        kind_of(&edge.from).map(Some)
    };
    let Some(source) = source else {
        return Some(ConnectRefusal::UnknownNode);
    };
    if kind_of(&edge.to).is_none() {
        return Some(ConnectRefusal::UnknownNode);
    }
    if !output_ports(source).contains(&edge.port) {
        return Some(ConnectRefusal::IllegalPort);
    }
    if graph.edges.contains(edge) {
        return Some(ConnectRefusal::DuplicateEdge);
    }
    if edge.from != TRIGGER && reaches(graph, &edge.to, &edge.from) {
        return Some(ConnectRefusal::Cycle);
    }
    None
}

/// Whether a path of edges leads from `start` to `goal` (a node reaches itself).
fn reaches(graph: &Graph, start: &str, goal: &str) -> bool {
    let mut seen: Vec<&str> = Vec::new();
    let mut queue = vec![start];
    let mut head = 0;
    while head < queue.len() {
        let id = queue[head];
        head += 1;
        if id == goal {
            return true;
        }
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        queue.extend(
            graph
                .edges
                .iter()
                .filter(|e| e.from == id)
                .map(|e| e.to.as_str()),
        );
    }
    false
}

/// The ops that undo `op`, read against `doc`, the state just before it.
fn inverse_of(doc: &DraftDoc, op: &Op) -> Vec<Op> {
    match op {
        Op::AddNode { node } => vec![Op::RemoveNode {
            id: node.id.clone(),
        }],
        Op::UpdateNode { id, label, spec } => {
            let Some(before) = doc.graph.node(id) else {
                return Vec::new();
            };
            vec![Op::UpdateNode {
                id: id.clone(),
                label: label.as_ref().map(|_| before.label.clone()),
                spec: spec.as_ref().map(|_| before.spec.clone()),
            }]
        }
        Op::RemoveNode { id } => {
            let Some(before) = doc.graph.node(id) else {
                return Vec::new();
            };
            let mut ops = vec![Op::AddNode {
                node: before.clone(),
            }];
            ops.extend(
                doc.graph
                    .edges
                    .iter()
                    .filter(|e| e.from == *id || e.to == *id)
                    .map(|edge| Op::Connect { edge: edge.clone() }),
            );
            ops
        }
        Op::Connect { edge } => vec![Op::Disconnect { edge: edge.clone() }],
        Op::Disconnect { edge } => vec![Op::Connect { edge: edge.clone() }],
        Op::SetTrigger { .. } => vec![Op::SetTrigger {
            trigger: doc.trigger.clone(),
        }],
        Op::Rename { .. } => vec![Op::Rename {
            name: doc.name.clone(),
        }],
        Op::SetIsolation { .. } => vec![Op::SetIsolation {
            isolation: doc.isolation,
        }],
    }
}

/// The ops that take the state after `ops` back to `doc`, last op first; none when `ops` does
/// not apply to `doc`.
pub fn inverse_ops(doc: &DraftDoc, ops: &[Op]) -> Option<Vec<Op>> {
    let mut undo: Vec<Vec<Op>> = Vec::new();
    let mut state = doc.clone();
    for op in ops {
        let next = apply_ops(&state, std::slice::from_ref(op)).ok()?;
        undo.insert(0, inverse_of(&state, op));
        state = next;
    }
    Some(undo.into_iter().flatten().collect())
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
