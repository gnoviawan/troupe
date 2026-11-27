//! The ops and positions each canvas gesture makes (`canvas/draft/gestures.ts`).

use std::collections::BTreeMap;

use super::draft::LayoutPatch;
use super::layout::place_nodes;
use super::types::{AutomationNode, Edge, Graph, Layout, NodePosition, Op, Port};

/// Stored positions for every card the canvas placed itself. A stored position moves the
/// layout anchor of unplaced cards, so a gesture that stores one freezes the rest where
/// they are drawn.
pub fn freeze_patch(graph: &Graph, layout: &Layout) -> LayoutPatch {
    place_nodes(graph, layout)
        .into_iter()
        .filter(|(id, _)| !layout.contains_key(id))
        .map(|(id, at)| (id, Some(at)))
        .collect()
}

/// The positions a move stores, or none when no card actually moved.
pub fn move_patch(
    graph: &Graph,
    layout: &Layout,
    moved: &BTreeMap<String, NodePosition>,
) -> Option<LayoutPatch> {
    let drawn = place_nodes(graph, layout);
    let changed: Vec<(&String, &NodePosition)> = moved
        .iter()
        .filter(|(id, at)| drawn.get(*id).is_some_and(|was| was != *at))
        .collect();
    if changed.is_empty() {
        return None;
    }
    let mut patch = freeze_patch(graph, layout);
    for (id, at) in changed {
        patch.insert(id.clone(), Some(*at));
    }
    Some(patch)
}

/// Deleting cards and edges. `remove_node` drops a card's own edges, so only other edges
/// disconnect; the remaining cards stay where they are drawn.
pub fn remove_gesture(
    graph: &Graph,
    layout: &Layout,
    nodes: &[String],
    edges: &[Edge],
) -> (Vec<Op>, LayoutPatch) {
    let mut ops: Vec<Op> = nodes
        .iter()
        .map(|id| Op::RemoveNode { id: id.clone() })
        .collect();
    ops.extend(
        edges
            .iter()
            .filter(|e| !nodes.contains(&e.from) && !nodes.contains(&e.to))
            .filter(|e| graph.edges.contains(e))
            .map(|edge| Op::Disconnect { edge: edge.clone() }),
    );
    let mut patch = freeze_patch(graph, layout);
    for id in nodes {
        patch.insert(id.clone(), None);
    }
    (ops, patch)
}

/// Where a new step goes and what it hangs off.
#[derive(Clone, Debug, PartialEq)]
pub struct AddPlacement {
    pub at: NodePosition,
    /// Chain the new step after this card's port.
    pub from: Option<(String, Port)>,
    /// Split this edge around the new step.
    pub split: Option<Edge>,
}

/// Adding `node` at `at`: after `from` when given, or into the middle of `split` (the source
/// keeps its port, the new step continues on its chain port).
pub fn add_gesture(
    graph: &Graph,
    layout: &Layout,
    node: &AutomationNode,
    chain: Option<Port>,
    placement: &AddPlacement,
) -> (Vec<Op>, LayoutPatch) {
    let mut ops = vec![Op::AddNode { node: node.clone() }];
    if let Some(split) = &placement.split {
        ops.push(Op::Disconnect {
            edge: split.clone(),
        });
        ops.push(Op::Connect {
            edge: Edge {
                from: split.from.clone(),
                port: split.port,
                to: node.id.clone(),
            },
        });
        if let Some(port) = chain {
            ops.push(Op::Connect {
                edge: Edge {
                    from: node.id.clone(),
                    port,
                    to: split.to.clone(),
                },
            });
        }
    } else if let Some((from, port)) = &placement.from {
        ops.push(Op::Connect {
            edge: Edge {
                from: from.clone(),
                port: *port,
                to: node.id.clone(),
            },
        });
    }
    let mut patch = freeze_patch(graph, layout);
    patch.insert(node.id.clone(), Some(placement.at));
    (ops, patch)
}
