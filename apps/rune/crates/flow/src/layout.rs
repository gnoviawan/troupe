//! Where each node sits. [`place`] keeps every saved position exactly and lays out the rest
//! left to right; an unsaved node keeps its laid-out offset from its first saved parent, so a
//! step added elsewhere lands beside the step it follows (the web client's `placeNodes`).
//!
//! [`auto_place`] is a small layered (Sugiyama) layout standing in for dagre's `rankdir: LR`:
//! cycles broken by reversing depth-first back edges, longest-path ranks, long edges split by
//! dummy points, barycentre ordering sweeps that keep the order with the fewest crossings, and
//! positions across each rank from alternating isotonic fits toward each node's neighbours.
//! It never fails: a cycle, a self edge or an edge to a missing node still lays out.

use std::collections::HashMap;

/// A node's top-left corner in canvas points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
}

/// Gaps in points: between nodes of one rank, and between ranks (dagre's `nodesep`, `ranksep`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub node: f64,
    pub rank: f64,
}

/// dagre's `edgesep`: the room an edge passing through a rank keeps from its neighbours.
const EDGE_SEP: f64 = 20.0;
const ORDER_SWEEPS: usize = 8;
const POSITION_PASSES: usize = 6;

#[derive(Clone, Copy, Debug)]
pub struct Node<'a> {
    pub id: &'a str,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Link<'a> {
    pub from: &'a str,
    pub to: &'a str,
}

/// Top-left of every node, in `nodes` order: `stored[i]` when set, else a laid-out place.
/// An unplaced node keeps its laid-out offset from the source of its first link from a placed
/// node, else from the first placed node; unplaced nodes resolve left to right, so a parent
/// is placed before its children.
pub fn place(nodes: &[Node], links: &[Link], stored: &[Option<Pos>], spacing: Spacing) -> Vec<Pos> {
    let mut placed: Vec<Option<Pos>> = (0..nodes.len())
        .map(|i| stored.get(i).copied().flatten())
        .collect();
    if placed.iter().all(Option::is_some) {
        return placed.into_iter().flatten().collect();
    }
    let auto = auto_place(nodes, links, spacing);
    let Some(first) = placed.iter().position(Option::is_some) else {
        return auto;
    };
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut pending: Vec<usize> = (0..nodes.len()).filter(|&i| placed[i].is_none()).collect();
    pending.sort_by(|&a, &b| auto[a].x.total_cmp(&auto[b].x));
    for id in pending {
        let anchor = links
            .iter()
            .filter(|l| index.get(l.to) == Some(&id))
            .filter_map(|l| index.get(l.from).copied())
            .find(|&from| placed[from].is_some())
            .unwrap_or(first);
        let from = placed[anchor].unwrap_or(auto[anchor]);
        placed[id] = Some(Pos {
            x: from.x + auto[id].x - auto[anchor].x,
            y: from.y + auto[id].y - auto[anchor].y,
        });
    }
    placed
        .into_iter()
        .map(|p| p.unwrap_or(Pos { x: 0.0, y: 0.0 }))
        .collect()
}

/// One point of the layered graph: a node, or a dummy where a long edge crosses a rank.
struct Point {
    rank: usize,
    /// Across the ranks (a node's height); 0 for a dummy.
    across: f64,
    real: bool,
    preds: Vec<usize>,
    succs: Vec<usize>,
}

/// Top-left of every node, in `nodes` order, with the content's top-left at the origin.
pub fn auto_place(nodes: &[Node], links: &[Link], spacing: Spacing) -> Vec<Pos> {
    let n = nodes.len();
    if n == 0 {
        return Vec::new();
    }
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for link in links {
        let (Some(&u), Some(&v)) = (index.get(link.from), index.get(link.to)) else {
            continue;
        };
        if u != v && !edges.contains(&(u, v)) {
            edges.push((u, v));
        }
    }
    let edges = acyclic(n, &edges);
    let rank = ranks(n, &edges);

    let mut points: Vec<Point> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| Point {
            rank: rank[i],
            across: node.height,
            real: true,
            preds: Vec::new(),
            succs: Vec::new(),
        })
        .collect();
    for &(u, v) in &edges {
        let mut prev = u;
        for r in rank[u] + 1..rank[v] {
            points.push(Point {
                rank: r,
                across: 0.0,
                real: false,
                preds: Vec::new(),
                succs: Vec::new(),
            });
            let dummy = points.len() - 1;
            link_points(&mut points, prev, dummy);
            prev = dummy;
        }
        link_points(&mut points, prev, v);
    }

    let layers = order(&points, n);
    let across = positions(&points, &layers, spacing);

    let ranks_count = layers.len();
    let mut rank_width = vec![0.0f64; ranks_count];
    for (i, node) in nodes.iter().enumerate() {
        rank_width[rank[i]] = rank_width[rank[i]].max(node.width);
    }
    let mut rank_centre = vec![0.0f64; ranks_count];
    let mut left = 0.0;
    for r in 0..ranks_count {
        rank_centre[r] = left + rank_width[r] / 2.0;
        left += rank_width[r] + spacing.rank;
    }

    let centres: Vec<(f64, f64)> = (0..n).map(|i| (rank_centre[rank[i]], across[i])).collect();
    let min_x = (0..n)
        .map(|i| centres[i].0 - nodes[i].width / 2.0)
        .fold(f64::INFINITY, f64::min);
    let min_y = (0..n)
        .map(|i| centres[i].1 - nodes[i].height / 2.0)
        .fold(f64::INFINITY, f64::min);
    (0..n)
        .map(|i| Pos {
            x: centres[i].0 - nodes[i].width / 2.0 - min_x,
            y: centres[i].1 - nodes[i].height / 2.0 - min_y,
        })
        .collect()
}

fn link_points(points: &mut [Point], from: usize, to: usize) {
    points[from].succs.push(to);
    points[to].preds.push(from);
}

/// `edges` with every depth-first back edge reversed (nodes and their edges in input order).
fn acyclic(n: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, &(u, _)) in edges.iter().enumerate() {
        out[u].push(i);
    }
    let mut reversed = vec![false; edges.len()];
    let mut state = vec![0u8; n];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0usize)];
        while let Some(&(v, k)) = stack.last() {
            if let Some(&e) = out[v].get(k) {
                stack.last_mut().expect("top").1 += 1;
                let w = edges[e].1;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => reversed[e] = true,
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
    let mut result: Vec<(usize, usize)> = Vec::new();
    for (i, &(u, v)) in edges.iter().enumerate() {
        let e = if reversed[i] { (v, u) } else { (u, v) };
        if !result.contains(&e) {
            result.push(e);
        }
    }
    result
}

/// Longest path from the sources: a node one rank after its furthest parent.
fn ranks(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut indegree = vec![0usize; n];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(u, v) in edges {
        indegree[v] += 1;
        out[u].push(v);
    }
    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        for &v in &out[u] {
            rank[v] = rank[v].max(rank[u] + 1);
            indegree[v] -= 1;
            if indegree[v] == 0 {
                queue.push(v);
            }
        }
    }
    rank
}

/// Each rank's points in order: depth first from the nodes in input order, then barycentre
/// sweeps, keeping the order with the fewest crossings.
fn order(points: &[Point], real: usize) -> Vec<Vec<usize>> {
    let ranks = points.iter().map(|p| p.rank).max().unwrap_or(0) + 1;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    let mut seen = vec![false; points.len()];
    for root in 0..real {
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            if seen[v] {
                continue;
            }
            seen[v] = true;
            layers[points[v].rank].push(v);
            for &w in points[v].succs.iter().rev() {
                if !seen[w] {
                    stack.push(w);
                }
            }
        }
    }
    let mut best = layers.clone();
    let mut best_crossings = crossings(points, &layers);
    for sweep in 0..ORDER_SWEEPS {
        let down = sweep % 2 == 0;
        let span: Vec<usize> = if down {
            (1..ranks).collect()
        } else {
            (0..ranks.saturating_sub(1)).rev().collect()
        };
        for r in span {
            let at = slots(&layers);
            let mut keyed: Vec<(f64, usize)> = layers[r]
                .iter()
                .map(|&v| {
                    let near = if down {
                        &points[v].preds
                    } else {
                        &points[v].succs
                    };
                    let bary = if near.is_empty() {
                        at[&v]
                    } else {
                        near.iter().map(|w| at[w]).sum::<f64>() / near.len() as f64
                    };
                    (bary, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[r] = keyed.into_iter().map(|(_, v)| v).collect();
        }
        let count = crossings(points, &layers);
        if count < best_crossings {
            best_crossings = count;
            best = layers.clone();
        }
    }
    best
}

/// Each point's index within its rank.
fn slots(layers: &[Vec<usize>]) -> HashMap<usize, f64> {
    let mut at = HashMap::new();
    for layer in layers {
        for (i, &v) in layer.iter().enumerate() {
            at.insert(v, i as f64);
        }
    }
    at
}

fn crossings(points: &[Point], layers: &[Vec<usize>]) -> usize {
    let at = slots(layers);
    let mut count = 0;
    for layer in layers {
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for &v in layer {
            for &w in &points[v].succs {
                spans.push((at[&v], at[&w]));
            }
        }
        for i in 0..spans.len() {
            for j in i + 1..spans.len() {
                let (a, b) = (spans[i], spans[j]);
                if (a.0 - b.0) * (a.1 - b.1) < 0.0 {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Each point's centre across the ranks: stacked in order, then pulled toward the mean of
/// its neighbours, rank by rank, keeping the order and the minimum gaps.
fn positions(points: &[Point], layers: &[Vec<usize>], spacing: Spacing) -> Vec<f64> {
    let half_gap = |v: usize| {
        if points[v].real {
            spacing.node / 2.0
        } else {
            EDGE_SEP / 2.0
        }
    };
    let gap = |a: usize, b: usize| {
        points[a].across / 2.0 + half_gap(a) + half_gap(b) + points[b].across / 2.0
    };
    let mut centre = vec![0.0f64; points.len()];
    for layer in layers {
        let mut y = 0.0;
        for (i, &v) in layer.iter().enumerate() {
            if i > 0 {
                y += gap(layer[i - 1], v);
            }
            centre[v] = y;
        }
        let mid = y / 2.0;
        for &v in layer {
            centre[v] -= mid;
        }
    }
    for pass in 0..POSITION_PASSES {
        let down = pass % 2 == 0;
        let span: Vec<usize> = if down {
            (0..layers.len()).collect()
        } else {
            (0..layers.len()).rev().collect()
        };
        for r in span {
            let layer = &layers[r];
            let desired: Vec<f64> = layer
                .iter()
                .map(|&v| {
                    let near = if down {
                        &points[v].preds
                    } else {
                        &points[v].succs
                    };
                    if near.is_empty() {
                        centre[v]
                    } else {
                        near.iter().map(|&w| centre[w]).sum::<f64>() / near.len() as f64
                    }
                })
                .collect();
            let gaps: Vec<f64> = layer.windows(2).map(|w| gap(w[0], w[1])).collect();
            for (&v, y) in layer.iter().zip(fit_with_gaps(&desired, &gaps)) {
                centre[v] = y;
            }
        }
    }
    centre
}

/// The positions closest to `desired` (least squares) that keep their order with at least
/// `gaps[i]` between `i` and `i + 1`: pool-adjacent-violators on the gap-shifted targets.
pub(crate) fn fit_with_gaps(desired: &[f64], gaps: &[f64]) -> Vec<f64> {
    let mut offset = vec![0.0; desired.len()];
    for i in 1..desired.len() {
        offset[i] = offset[i - 1] + gaps[i - 1];
    }
    // Blocks of (mean, count).
    let mut blocks: Vec<(f64, usize)> = Vec::new();
    for (i, &d) in desired.iter().enumerate() {
        blocks.push((d - offset[i], 1));
        while blocks.len() > 1 {
            let (b_mean, b_count) = blocks[blocks.len() - 1];
            let (a_mean, a_count) = blocks[blocks.len() - 2];
            if a_mean <= b_mean {
                break;
            }
            blocks.pop();
            let total = a_count + b_count;
            *blocks.last_mut().expect("two blocks") = (
                (a_mean * a_count as f64 + b_mean * b_count as f64) / total as f64,
                total,
            );
        }
    }
    let mut fitted = Vec::with_capacity(desired.len());
    for (mean, count) in blocks {
        fitted.extend(std::iter::repeat_n(mean, count));
    }
    fitted.iter().zip(&offset).map(|(q, o)| q + o).collect()
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
