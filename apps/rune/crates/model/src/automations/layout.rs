//! Where each card sits and the order steps read in (`canvas/layout.ts`, `canvas/ports.ts`).
//! The graph may carry validation issues (a cycle, an unreachable node, an edge to a missing
//! node), so every input lays out.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use troupe_flow::{Link, Node, Pos, Spacing};

use super::ops::output_ports;
use super::types::{
    AutomationNode, Edge, Graph, Layout, NodeKind, NodePosition, NodeSpec, Port, TRIGGER,
};

/// A card's fixed size; edges attach to handles laid out against it.
pub const CARD_WIDTH: f64 = 192.0;
pub const CARD_HEIGHT: f64 = 64.0;
/// Agent and MCP tool cards hang two config slots below the card.
pub const SLOT_DEPTH: f64 = 56.0;
pub const HANDLE_LABEL_WIDTH: f64 = 40.0;
const PORT_PITCH: f64 = 18.0;
const LAST_PORT_CENTRE: f64 = 49.0;
/// dagre's `nodesep` and `ranksep` on the automation canvas.
pub const SPACING: Spacing = Spacing {
    node: 40.0,
    rank: 64.0,
};

pub fn has_slots(spec: &NodeSpec) -> bool {
    matches!(spec.kind(), NodeKind::Agent | NodeKind::McpTool)
}

/// Vertical centre of output `index` of `count`; the last port sits lowest.
pub fn output_port_y(index: usize, count: usize) -> f64 {
    LAST_PORT_CENTRE - PORT_PITCH * (count - 1 - index) as f64
}

pub const INPUT_PORT_Y: f64 = LAST_PORT_CENTRE - PORT_PITCH;

/// The kind of a card id, `None` for the trigger; unknown ids read as having no ports.
fn ports_of(graph: &Graph, id: &str) -> Option<&'static [Port]> {
    if id == TRIGGER {
        return Some(output_ports(None));
    }
    graph.node(id).map(|n| output_ports(Some(n.spec.kind())))
}

/// Edges whose ends both exist and whose port the source has. Others are validation issues
/// the issue list names; the drawing and the step list leave them out.
pub fn drawable_edges(graph: &Graph) -> Vec<&Edge> {
    graph
        .edges
        .iter()
        .filter(|e| {
            e.to != TRIGGER
                && ports_of(graph, &e.to).is_some()
                && ports_of(graph, &e.from).is_some_and(|p| p.contains(&e.port))
        })
        .collect()
}

/// The card ids in canvas order: the trigger, then the nodes.
pub fn card_ids(graph: &Graph) -> Vec<&str> {
    std::iter::once(TRIGGER)
        .chain(graph.nodes.iter().map(|n| n.id.as_str()))
        .collect()
}

/// Top-left of every card: its stored position, else a left-to-right layered placement. An
/// unplaced card keeps its laid-out offset from its first placed parent.
pub fn place_nodes(graph: &Graph, stored: &Layout) -> BTreeMap<String, NodePosition> {
    let ids = card_ids(graph);
    let nodes: Vec<Node> = ids
        .iter()
        .map(|&id| {
            let slots = graph.node(id).is_some_and(|n| has_slots(&n.spec));
            Node {
                id,
                width: CARD_WIDTH,
                height: CARD_HEIGHT + if slots { SLOT_DEPTH } else { 0.0 },
            }
        })
        .collect();
    let links: Vec<Link> = drawable_edges(graph)
        .into_iter()
        .map(|e| Link {
            from: &e.from,
            to: &e.to,
        })
        .collect();
    let saved: Vec<Option<Pos>> = ids
        .iter()
        .map(|id| stored.get(*id).map(|p| Pos { x: p.x, y: p.y }))
        .collect();
    let placed = troupe_flow::place(&nodes, &links, &saved, SPACING);
    ids.iter()
        .zip(placed)
        .map(|(id, p)| ((*id).to_owned(), NodePosition { x: p.x, y: p.y }))
        .collect()
}

/// Steps in the order they first run: a step after all its parents, siblings in input order,
/// and a loop's whole body before what follows the loop. Unreachable steps follow; steps a
/// cycle holds back come last, in input order.
pub fn step_order(graph: &Graph) -> Vec<&AutomationNode> {
    let edges = drawable_edges(graph);
    let index: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut indegree: HashMap<&str, i64> = HashMap::new();
    for e in &edges {
        *indegree.entry(e.to.as_str()).or_default() += 1;
    }
    let mut order: Vec<&str> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    type Queue<'g> = std::collections::VecDeque<&'g str>;

    fn release<'g>(
        edge: &'g Edge,
        into: &mut Queue<'g>,
        indegree: &mut HashMap<&'g str, i64>,
        seen: &mut BTreeSet<&'g str>,
    ) {
        let left = indegree.get(edge.to.as_str()).copied().unwrap_or(0) - 1;
        indegree.insert(edge.to.as_str(), left);
        if left == 0 && !seen.contains(edge.to.as_str()) {
            seen.insert(edge.to.as_str());
            into.push_back(edge.to.as_str());
        }
    }

    // Breadth first, except that a for_each drains its `item` body before releasing its
    // other children.
    fn drain<'g>(
        queue: &mut Queue<'g>,
        edges: &[&'g Edge],
        index: &HashMap<&str, usize>,
        indegree: &mut HashMap<&'g str, i64>,
        seen: &mut BTreeSet<&'g str>,
        order: &mut Vec<&'g str>,
    ) {
        while let Some(id) = queue.pop_front() {
            order.push(id);
            let mut children: Vec<&'g Edge> =
                edges.iter().copied().filter(|e| e.from == id).collect();
            children.sort_by_key(|e| index.get(e.to.as_str()).copied().unwrap_or(0));
            let mut body = Queue::new();
            for e in children.iter().filter(|e| e.port == Port::Item) {
                release(e, &mut body, indegree, seen);
            }
            drain(&mut body, edges, index, indegree, seen, order);
            for e in children.iter().filter(|e| e.port != Port::Item) {
                release(e, queue, indegree, seen);
            }
        }
    }

    seen.insert(TRIGGER);
    drain(
        &mut [TRIGGER].into(),
        &edges,
        &index,
        &mut indegree,
        &mut seen,
        &mut order,
    );
    for node in &graph.nodes {
        if seen.contains(node.id.as_str())
            || indegree.get(node.id.as_str()).copied().unwrap_or(0) > 0
        {
            continue;
        }
        seen.insert(node.id.as_str());
        drain(
            &mut [node.id.as_str()].into(),
            &edges,
            &index,
            &mut indegree,
            &mut seen,
            &mut order,
        );
    }
    for node in &graph.nodes {
        if !seen.contains(node.id.as_str()) {
            order.push(node.id.as_str());
        }
    }
    order.into_iter().filter_map(|id| graph.node(id)).collect()
}

/// Every node a path of edges reaches from `starts` (the starts not included unless reached).
fn reach<'g>(graph: &'g Graph, starts: &[&'g str]) -> BTreeSet<&'g str> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = starts.to_vec();
    while let Some(id) = stack.pop() {
        for e in graph.edges.iter().filter(|e| e.from == id) {
            if seen.insert(e.to.as_str()) {
                stack.push(&e.to);
            }
        }
    }
    seen
}

/// Each for_each's body: the nodes its `item` edges reach.
pub fn loop_bodies(graph: &Graph) -> BTreeMap<String, BTreeSet<String>> {
    let mut bodies = BTreeMap::new();
    for node in &graph.nodes {
        if node.spec.kind() != NodeKind::ForEach {
            continue;
        }
        let entries: Vec<&str> = graph
            .edges
            .iter()
            .filter(|e| e.from == node.id && e.port == Port::Item && graph.node(&e.to).is_some())
            .map(|e| e.to.as_str())
            .collect();
        let mut body: BTreeSet<String> = entries.iter().map(|s| (*s).to_owned()).collect();
        body.extend(reach(graph, &entries).into_iter().map(str::to_owned));
        bodies.insert(node.id.clone(), body);
    }
    bodies
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
