//! What a run says about each card and edge of its own graph (`canvas/run/run-overlay.ts`).
//! Pure: the run view recomputes it from the step rows on every poll.

use std::collections::BTreeMap;

use serde::Serialize;

use super::layout::{drawable_edges, loop_bodies};
use super::types::{
    AutomationNode, NodeKind, Port, Run, RunStatus, RunTrigger, Step, StepStatus, TRIGGER,
};

/// A step as the run shows it: `pending` before it has a row, `cancelled` when a cancel
/// stopped it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepView {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
    Waiting,
    Cancelled,
}

impl StepView {
    /// `STEP_VIEW` labels.
    pub fn label(self) -> &'static str {
        match self {
            StepView::Pending => "Not reached",
            StepView::Running => "Running",
            StepView::Succeeded => "Succeeded",
            StepView::Failed => "Failed",
            StepView::Skipped => "Skipped",
            StepView::Waiting => "Waiting",
            StepView::Cancelled => "Cancelled",
        }
    }

    fn settled(self) -> bool {
        !matches!(
            self,
            StepView::Pending | StepView::Running | StepView::Waiting
        )
    }
}

/// How an edge took part: it fired, it fired into a step still at work, or it did not fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRun {
    Taken,
    Live,
    NotTaken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LoopCount {
    pub iterations: usize,
    pub failed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CardRun {
    pub status: StepView,
    #[serde(rename = "loop", skip_serializing_if = "Option::is_none")]
    pub loop_: Option<LoopCount>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LoopRun {
    pub iterations: Vec<StepView>,
    pub failed: usize,
    /// The iteration body cards show: the one picked, else the latest.
    pub picked: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FiredBy {
    pub by: RunTrigger,
    pub at: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Overlay {
    pub fired: FiredBy,
    pub cards: BTreeMap<String, CardRun>,
    /// Keyed like the canvas edges, `from:port->to`. An edge with no entry is undecided.
    pub edges: BTreeMap<String, EdgeRun>,
    pub loops: BTreeMap<String, LoopRun>,
    /// The for_each whose body holds each body step.
    #[serde(rename = "loopOf")]
    pub loop_of: BTreeMap<String, String>,
    /// The row each card shows: a body step's row in its loop's picked iteration.
    pub rows: BTreeMap<String, Step>,
}

/// `executor.rs` `CANCELLED`: the error a step stopped by a cancel records.
const CANCELLED: &str = "cancelled";

fn view_of(row: Option<&Step>) -> StepView {
    let Some(row) = row else {
        return StepView::Pending;
    };
    match row.status {
        StepStatus::Failed if row.error.as_deref() == Some(CANCELLED) => StepView::Cancelled,
        StepStatus::Running => StepView::Running,
        StepStatus::Succeeded => StepView::Succeeded,
        StepStatus::Failed => StepView::Failed,
        StepStatus::Skipped => StepView::Skipped,
        StepStatus::Waiting => StepView::Waiting,
    }
}

/// The port a settled row fired, as the host replays a row: failed fires `error`, a
/// condition fires `true` or `false` from its result, the rest fire `out`.
fn fired_port(row: &Step, kind: Option<NodeKind>) -> Option<Port> {
    match view_of(Some(row)) {
        StepView::Failed => Some(Port::Error),
        StepView::Succeeded if kind == Some(NodeKind::Condition) => Some(
            if row.output.get("result") == Some(&serde_json::Value::Bool(true)) {
                Port::True
            } else {
                Port::False
            },
        ),
        StepView::Succeeded => Some(Port::Out),
        _ => None,
    }
}

fn iteration_view(rows: &[&Step], latest_of_running_loop: bool) -> StepView {
    let views: Vec<StepView> = rows.iter().map(|r| view_of(Some(r))).collect();
    for view in [
        StepView::Running,
        StepView::Waiting,
        StepView::Cancelled,
        StepView::Failed,
    ] {
        if views.contains(&view) {
            return view;
        }
    }
    if latest_of_running_loop {
        StepView::Running
    } else {
        StepView::Succeeded
    }
}

pub fn overlay(run: &Run, steps: &[Step], picks: &BTreeMap<String, u64>) -> Overlay {
    let graph = &run.graph;
    let row = |node: &str, iteration: Option<u64>| {
        steps
            .iter()
            .find(|s| s.node_id == node && s.iteration == iteration)
    };
    let mut loop_of = BTreeMap::new();
    for (lp, body) in loop_bodies(graph) {
        for id in body {
            loop_of.insert(id, lp.clone());
        }
    }

    let mut loops = BTreeMap::new();
    for node in graph
        .nodes
        .iter()
        .filter(|n| n.spec.kind() == NodeKind::ForEach)
    {
        let body: Vec<&Step> = steps
            .iter()
            .filter(|s| loop_of.get(&s.node_id) == Some(&node.id))
            .collect();
        let count = body
            .iter()
            .filter_map(|s| s.iteration)
            .map(|i| i + 1)
            .max()
            .unwrap_or(0);
        let running = !view_of(row(&node.id, None)).settled();
        let iterations: Vec<StepView> = (0..count)
            .map(|index| {
                let rows: Vec<&Step> = body
                    .iter()
                    .copied()
                    .filter(|s| s.iteration == Some(index))
                    .collect();
                iteration_view(&rows, running && index + 1 == count)
            })
            .collect();
        let picked = match picks.get(&node.id) {
            _ if count == 0 => None,
            Some(&pick) if pick < count => Some(pick),
            _ => Some(count - 1),
        };
        let failed = iterations
            .iter()
            .filter(|v| **v == StepView::Failed)
            .count();
        loops.insert(
            node.id.clone(),
            LoopRun {
                iterations,
                failed,
                picked,
            },
        );
    }

    let mut rows = BTreeMap::new();
    let mut cards = BTreeMap::new();
    let trigger = if run.status == RunStatus::Skipped {
        StepView::Skipped
    } else {
        StepView::Succeeded
    };
    cards.insert(
        TRIGGER.to_owned(),
        CardRun {
            status: trigger,
            loop_: None,
        },
    );
    for node in &graph.nodes {
        let picked = loop_of
            .get(&node.id)
            .and_then(|lp| loops.get(lp))
            .and_then(|l: &LoopRun| l.picked);
        // A loop that ran no iteration leaves one `skipped` row per body step, outside any iteration.
        let shown = row(&node.id, picked).or_else(|| row(&node.id, None));
        if let Some(r) = shown {
            rows.insert(node.id.clone(), r.clone());
        }
        let own = loops.get(&node.id).map(|l| LoopCount {
            iterations: l.iterations.len(),
            failed: l.failed,
        });
        cards.insert(
            node.id.clone(),
            CardRun {
                status: view_of(shown),
                loop_: own,
            },
        );
    }

    let mut edges = BTreeMap::new();
    for edge in drawable_edges(graph) {
        let fired = if edge.from == TRIGGER {
            Some(run.status != RunStatus::Skipped)
        } else if edge.port == Port::Item {
            let ran = loops
                .get(&edge.from)
                .is_some_and(|l| !l.iterations.is_empty());
            let view = cards
                .get(&edge.from)
                .map_or(StepView::Pending, |c| c.status);
            if ran {
                Some(true)
            } else if view.settled() {
                Some(false)
            } else {
                None
            }
        } else {
            let r = rows.get(&edge.from);
            let view = view_of(r);
            match r {
                Some(r) if view.settled() => Some(
                    fired_port(r, graph.node(&edge.from).map(|n| n.spec.kind())) == Some(edge.port),
                ),
                _ => None,
            }
        };
        let Some(fired) = fired else { continue };
        let target = cards.get(&edge.to).map(|c| c.status);
        let mark = if !fired {
            EdgeRun::NotTaken
        } else if matches!(target, Some(StepView::Running | StepView::Waiting)) {
            EdgeRun::Live
        } else {
            EdgeRun::Taken
        };
        edges.insert(edge.key(), mark);
    }

    Overlay {
        fired: FiredBy {
            by: run.trigger,
            at: run.scheduled_for,
        },
        cards,
        edges,
        loops,
        loop_of,
        rows,
    }
}

/// The step a run opens on: the first failed one, else the first at work, in `order`. A
/// for_each only when no step in its body explains it. None: it opens on the trigger.
pub fn opening_step(overlay: &Overlay, order: &[&AutomationNode]) -> Option<String> {
    let groups: [&[StepView]; 2] = [&[StepView::Failed], &[StepView::Running, StepView::Waiting]];
    for wanted in groups {
        let hits: Vec<&&AutomationNode> = order
            .iter()
            .filter(|n| {
                wanted.contains(
                    &overlay
                        .cards
                        .get(&n.id)
                        .map_or(StepView::Pending, |c| c.status),
                )
            })
            .collect();
        if let Some(hit) = hits
            .iter()
            .find(|n| n.spec.kind() != NodeKind::ForEach)
            .or(hits.first())
        {
            return Some(hit.id.clone());
        }
    }
    None
}
