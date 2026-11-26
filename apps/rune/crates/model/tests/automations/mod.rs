//! Replays the automation vectors: the canvas draft with its gestures, what runs show and how
//! automations read, and the routes the web client sends.

use std::collections::BTreeMap;

use model::Effect;
use model::automations::draft::{self, Action, Draft, LayoutPatch};
use model::automations::gestures::{AddPlacement, add_gesture, move_patch, remove_gesture};
use model::automations::kinds::{chain_port, new_node};
use model::automations::layout::step_order;
use model::automations::ops::refuse_connect;
use model::automations::routes::Call;
use model::automations::run::{opening_step, overlay};
use model::automations::{
    Automation, Edge, Graph, Issue, NodeKind, NodePosition, Op, Port, Run, Step, format,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Store;
use crate::common::{Replay, parse, repo_root, to_value};

fn arg<T: serde::de::DeserializeOwned>(event: &Value, i: usize) -> Result<T, String> {
    parse(event["args"].get(i).unwrap_or(&Value::Null))
}

fn action(event: &Value) -> &str {
    event["action"].as_str().unwrap_or_default()
}

pub struct DraftStore;

#[derive(Deserialize)]
struct Placement {
    at: NodePosition,
    from: Option<From>,
    split: Option<Edge>,
}

#[derive(Deserialize)]
struct From {
    from: String,
    port: Port,
}

/// Plans and applies a gesture; the refusal, as the recorder returns it.
fn gesture(state: &mut Draft, ops: Vec<Op>, layout: LayoutPatch) -> Option<Value> {
    match draft::plan_gesture(state, ops, layout) {
        Err(rejection) => Some(to_value(&rejection)),
        Ok(g) => {
            *state = draft::reduce(state, Action::Gesture(g));
            None
        }
    }
}

impl Store for DraftStore {
    type State = Draft;
    fn setup(_: &Value, initial: &Value) -> Result<Draft, String> {
        parse(initial)
    }
    fn step(
        state: &mut Draft,
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let graph = state.doc.graph.clone();
        let reply = match action(event) {
            "gesture" => {
                let layout: Option<LayoutPatch> = arg(event, 1)?;
                gesture(state, arg(event, 0)?, layout.unwrap_or_default())
            }
            "add" => {
                let kind: NodeKind = arg(event, 0)?;
                let p: Placement = arg(event, 1)?;
                let node = new_node(kind, &graph);
                let placement = AddPlacement {
                    at: p.at,
                    from: p.from.map(|f| (f.from, f.port)),
                    split: p.split,
                };
                let (ops, layout) =
                    add_gesture(&graph, &state.layout, &node, chain_port(kind), &placement);
                gesture(state, ops, layout)
            }
            "remove" => {
                let nodes: Vec<String> = arg(event, 0)?;
                let edges: Vec<Edge> = arg(event, 1)?;
                let (ops, layout) = remove_gesture(&graph, &state.layout, &nodes, &edges);
                if ops.is_empty() {
                    None
                } else {
                    gesture(state, ops, layout)
                }
            }
            "move" => {
                let moved: BTreeMap<String, NodePosition> = arg(event, 0)?;
                match move_patch(&graph, &state.layout, &moved) {
                    Some(patch) => gesture(state, Vec::new(), patch),
                    None => None,
                }
            }
            "connect" => {
                let edge: Edge = arg(event, 0)?;
                match refuse_connect(&graph, &edge) {
                    Some(refusal) => Some(to_value(&refusal)),
                    None => gesture(state, vec![Op::Connect { edge }], LayoutPatch::new()),
                }
            }
            "previewed" => {
                let current: bool = arg(event, 1)?;
                let pending_rev = if current {
                    state.pending_rev
                } else {
                    state.pending_rev.wrapping_add(1 << 40)
                };
                *state = draft::reduce(
                    state,
                    Action::Previewed {
                        pending_rev,
                        issues: arg(event, 0)?,
                    },
                );
                None
            }
            "saved" => {
                *state = draft::reduce(
                    state,
                    Action::Saved {
                        automation: arg(event, 0)?,
                        issues: arg(event, 1)?,
                    },
                );
                None
            }
            "requeue" => {
                *state = draft::reduce(state, Action::Requeue(arg(event, 0)?));
                None
            }
            "head" => {
                *state = draft::reduce(
                    state,
                    Action::Head {
                        automation: arg(event, 0)?,
                        issues: arg(event, 1)?,
                    },
                );
                None
            }
            "rebased" => {
                let (automation, issues, by): (Automation, Vec<Issue>, Option<String>) =
                    (arg(event, 0)?, arg(event, 1)?, arg(event, 2)?);
                *state = draft::reduce(
                    state,
                    Action::Rebased {
                        automation,
                        issues,
                        by,
                    },
                );
                None
            }
            simple @ ("undo" | "redo" | "discard" | "dismiss_conflict") => {
                let a = match simple {
                    "undo" => Action::Undo,
                    "redo" => Action::Redo,
                    "discard" => Action::Discard,
                    _ => Action::DismissConflict,
                };
                *state = draft::reduce(state, a);
                None
            }
            other => return Err(format!("no draft action {other}")),
        };
        Ok((Vec::new(), reply))
    }
    fn query(state: &Draft, event: &Value) -> Result<Value, String> {
        Ok(match action(event) {
            "pendingOps" => to_value(&state.pending_ops()),
            "changedPositions" => {
                to_value(&draft::changed_positions(&state.base.layout, &state.layout))
            }
            other => return Err(format!("no draft query {other}")),
        })
    }
}

fn fixture() -> Value {
    crate::common::read_json(&repo_root().join("src/shared/types/__fixtures__/automation.json"))
}

pub struct RunStore;

impl Store for RunStore {
    type State = ();
    fn setup(_: &Value, _: &Value) -> Result<(), String> {
        Ok(())
    }
    fn step(
        _: &mut (),
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        Err(format!(
            "automation-run has only reads, not {}",
            action(event)
        ))
    }
    fn query(_: &(), event: &Value) -> Result<Value, String> {
        let f = fixture();
        let runs: Vec<Run> = parse(&f["runs"])?;
        let steps: Vec<Step> = parse(&f["steps"])?;
        let run_at = |i: usize| -> Result<(&Run, Vec<Step>), String> {
            let run = runs.get(i).ok_or("no such run")?;
            Ok((
                run,
                steps
                    .iter()
                    .filter(|s| s.run_id == run.id)
                    .cloned()
                    .collect(),
            ))
        };
        Ok(match action(event) {
            "runOverlay" => {
                let (run, rows) = run_at(arg(event, 0)?)?;
                let picks: Option<BTreeMap<String, u64>> = arg(event, 1)?;
                to_value(&overlay(run, &rows, &picks.unwrap_or_default()))
            }
            "openingStep" => {
                let (run, rows) = run_at(arg(event, 0)?)?;
                let shown = overlay(run, &rows, &BTreeMap::new());
                json!(opening_step(&shown, &step_order(&run.graph)))
            }
            "stepOrder" => {
                let graph: Graph = arg(event, 0)?;
                json!(
                    step_order(&graph)
                        .iter()
                        .map(|n| n.id.as_str())
                        .collect::<Vec<_>>()
                )
            }
            "automationState" => to_value(&format::state(&arg(event, 0)?)),
            "formatDuration" => json!(format::format_duration(arg(event, 0)?)),
            "triggerSummary" => json!(format::trigger_summary(&arg(event, 0)?, 0.0, 0)),
            "nodeKeyDetail" => json!(format::node_key_detail(&arg(event, 0)?)),
            "agentToolsLabel" => json!(format::agent_tools_label(&arg::<Vec<_>>(event, 0)?)),
            "runStatusLabel" => json!(format::run_presentation(run_at(arg(event, 0)?)?.0).0),
            "newNode" => to_value(&new_node(arg(event, 0)?, &arg(event, 1)?)),
            "chainPort" => to_value(&chain_port(arg(event, 0)?)),
            other => return Err(format!("no run query {other}")),
        })
    }
}

pub struct RoutesStore;

/// The facade's method names, as the recorder calls them, to the call they send.
fn call(event: &Value) -> Result<Call, String> {
    let s = |i| arg::<String>(event, i);
    Ok(match action(event) {
        "list" => Call::List { project_id: s(0)? },
        "get" => Call::Get { id: s(0)? },
        "create" => Call::Create {
            project_id: s(0)?,
            name: s(1)?,
        },
        "applyOps" => Call::ApplyOps {
            id: s(0)?,
            base_revision: arg(event, 1)?,
            ops: arg(event, 2)?,
        },
        "previewOps" => Call::PreviewOps {
            id: s(0)?,
            base_revision: arg(event, 1)?,
            ops: arg(event, 2)?,
        },
        "setLayout" => Call::SetLayout {
            id: s(0)?,
            layout: arg(event, 1)?,
        },
        "remove" => Call::Remove { id: s(0)? },
        "setEnabled" => Call::SetEnabled {
            id: s(0)?,
            enabled: arg(event, 1)?,
        },
        "approve" => Call::Approve {
            id: s(0)?,
            revision: arg(event, 1)?,
        },
        "runNow" => Call::RunNow { id: s(0)? },
        "cancelRun" => Call::CancelRun { run_id: s(0)? },
        "decideApproval" => Call::DecideApproval {
            run_id: s(0)?,
            node_id: s(1)?,
            approved: arg(event, 2)?,
        },
        "listRuns" => Call::ListRuns {
            id: s(0)?,
            limit: arg(event, 1)?,
        },
        "runSteps" => Call::RunSteps { run_id: s(0)? },
        "markSeen" => Call::MarkSeen { id: s(0)? },
        "listAgents" => Call::ListAgents,
        "listSecrets" => Call::ListSecrets { project_id: s(0)? },
        other => return Err(format!("no automation call {other}")),
    })
}

impl Store for RoutesStore {
    type State = ();
    fn setup(_: &Value, _: &Value) -> Result<(), String> {
        Ok(())
    }
    fn step(
        _: &mut (),
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let route = call(event)?.route();
        Ok((
            Vec::new(),
            Some(
                json!({ "kind": "http", "method": route.method, "path": route.path, "body": route.body, "commandId": false }),
            ),
        ))
    }
    fn query(_: &(), event: &Value) -> Result<Value, String> {
        Err(format!("no routes query {}", action(event)))
    }
}
