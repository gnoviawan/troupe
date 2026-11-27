//! The automation graph on a `troupe-flow` canvas (`flow-graph.ts` `toFlow`, `NodeCard.tsx`,
//! `PortEdgeView.tsx`): a card per step plus the trigger card, an edge per drawable edge in its
//! port's colour, and on a run each card's status and how each edge took part.

use std::collections::HashMap;

use model::automations::format::{
    self, format_date_time, format_duration, node_key_detail, trigger_label,
};
use model::automations::layout::{
    INPUT_PORT_Y, drawable_edges, has_slots, output_port_y, place_nodes,
};
use model::automations::ops::output_ports;
use model::automations::run::{EdgeRun, Overlay};
use model::automations::{
    Graph, Issue, Layout, NodeSpec, Port, RunTrigger, TRIGGER, Trigger, kinds,
};
use rune::text;
use theme::Tokens;
use troupe_flow::canvas::{Card, Look, OutPort, Selection, Wire};
use ui::automations::{CARD_H, CARD_W, Face, card, glyph, kind_icon, port_look};
use ui::type_::sans;

/// What the canvas draws beside the graph.
pub struct Marks<'a> {
    pub issues: &'a [Issue],
    pub run: Option<&'a Overlay>,
    pub selection: Option<&'a Selection>,
    /// A step's card that cannot take the edge being drawn.
    pub dimmed: &'a dyn Fn(&str) -> bool,
    pub now_ms: f64,
    pub offset_min: i64,
}

pub fn look(t: &Tokens) -> Look {
    let c = &t.colors;
    Look {
        background: c.background,
        handle: c.card,
        handle_border: c.muted_foreground,
        preview: c.primary,
    }
}

/// The short detail a 192 px card has room for (`cardDetail`).
fn card_detail(spec: &NodeSpec) -> String {
    match spec {
        NodeSpec::Http { .. } => node_key_detail(spec),
        NodeSpec::Agent { agent_id, .. } => agent_id.clone(),
        NodeSpec::Transform { fields } => {
            if fields.len() == 1 {
                "1 field".to_owned()
            } else {
                format!("{} fields", fields.len())
            }
        }
        NodeSpec::Condition { op, right, .. } => {
            [op.as_str(), right.as_deref().unwrap_or_default()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        }
        NodeSpec::Fail { message } => message.clone(),
        NodeSpec::ForEach { max_iterations, .. } => format!("max {max_iterations}"),
        NodeSpec::Merge { mode } => match mode {
            model::automations::MergeMode::All => "every branch".to_owned(),
            model::automations::MergeMode::Any => "any branch".to_owned(),
        },
        NodeSpec::Wait { duration_ms } => format_duration(*duration_ms as f64),
        NodeSpec::Approval { timeout_ms, .. } => {
            format!("times out after {}", format_duration(*timeout_ms as f64))
        }
        NodeSpec::McpTool { server_id, .. } => server_id.clone(),
    }
}

fn slots(spec: &NodeSpec) -> Vec<(&'static str, String)> {
    match spec {
        NodeSpec::Agent { model, tools, .. } => vec![
            (
                "Model",
                model
                    .clone()
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| "Default".to_owned()),
            ),
            (
                "Tools",
                if tools.is_empty() {
                    "None".to_owned()
                } else {
                    tools
                        .iter()
                        .map(|t| {
                            serde_json::to_value(t)
                                .ok()
                                .and_then(|v| v.as_str().map(str::to_owned))
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            ),
        ],
        NodeSpec::McpTool {
            server_id, tool, ..
        } => vec![("Server", server_id.clone()), ("Tool", tool.clone())],
        _ => Vec::new(),
    }
}

/// Issues a card names in place of its detail (`CARD_PROBLEMS`).
fn problem(code: &str) -> Option<&'static str> {
    match code {
        "unknown_mcp_server" => Some("Server not configured"),
        "mcp_servers_without_other" => Some("MCP servers need MCP tools"),
        "unknown_mcp_tool" => Some("Tool not found"),
        _ => None,
    }
}

pub fn cards_and_wires(
    t: &Tokens,
    graph: &Graph,
    trigger: &Trigger,
    layout: &Layout,
    marks: &Marks,
) -> (Vec<Card>, Vec<Wire>) {
    let c = &t.colors;
    let places = place_nodes(graph, layout);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut problems: HashMap<&str, &str> = HashMap::new();
    for issue in marks.issues {
        let id = issue.node_id.as_deref().unwrap_or(TRIGGER);
        *counts.entry(id).or_default() += 1;
        if let (Some(node), Some(p)) = (issue.node_id.as_deref(), problem(&issue.code)) {
            problems.entry(node).or_insert(p);
        }
    }
    let selected = |id: &str| marks.selection == Some(&Selection::Card(id.to_owned()));
    let status = |id: &str| marks.run.and_then(|o| o.cards.get(id));
    let at = |id: &str| {
        places
            .get(id)
            .map_or([0.0, 0.0], |p| [p.x as f32, p.y as f32])
    };
    let ports = |kind| {
        let list = output_ports(kind);
        list.iter()
            .enumerate()
            .map(|(i, port)| {
                let (color, _) = port_look(t, *port);
                OutPort {
                    key: port.as_str().to_owned(),
                    y: output_port_y(i, list.len()) as f32,
                    color,
                    label: (*port == Port::Error).then(|| {
                        text("error")
                            .font(sans(12.0, 400.0, 16.0))
                            .color(c.destructive)
                            .a11y_hidden()
                    }),
                }
            })
            .collect::<Vec<_>>()
    };

    let (icon, title, subtitle) = match marks.run {
        Some(run) if run.fired.by == RunTrigger::Manual => (
            glyph("Zap"),
            "Manual".to_owned(),
            "Started by hand".to_owned(),
        ),
        Some(run) => (
            glyph("Clock"),
            trigger_label(run.fired.by).to_owned(),
            format!(
                "Due {}",
                format_date_time(run.fired.at, marks.now_ms, marks.offset_min)
            ),
        ),
        None => {
            let manual = matches!(trigger, Trigger::Manual);
            (
                glyph(if manual { "Zap" } else { "Clock" }),
                if manual { "Manual" } else { "Schedule" }.to_owned(),
                format::trigger_summary(trigger, marks.now_ms, marks.offset_min),
            )
        }
    };
    let face = Face {
        icon,
        title,
        subtitle,
        problem: false,
        issues: counts.get(TRIGGER).copied().unwrap_or(0),
        status: status(TRIGGER).map(|r| r.status),
        selected: selected(TRIGGER),
        loop_: None,
        slots: Vec::new(),
        dimmed: false,
    };
    let mut cards = vec![Card {
        id: TRIGGER.to_owned(),
        at: at(TRIGGER),
        size: [CARD_W, CARD_H],
        body: card(t, face),
        input_y: None,
        outputs: ports(None),
    }];
    for node in &graph.nodes {
        let kind = node.spec.kind();
        let issue = problems.get(node.id.as_str()).copied();
        let run = status(&node.id);
        let face = Face {
            icon: kind_icon(kind),
            title: node.label.clone(),
            subtitle: issue.map_or_else(
                || format!("{} · {}", kinds::label(kind), card_detail(&node.spec)),
                str::to_owned,
            ),
            problem: issue.is_some(),
            issues: counts.get(node.id.as_str()).copied().unwrap_or(0),
            status: run.map(|r| r.status),
            selected: selected(&node.id),
            loop_: run.and_then(|r| r.loop_),
            slots: slots(&node.spec),
            dimmed: (marks.dimmed)(&node.id),
        };
        let height = CARD_H
            + if has_slots(&node.spec) {
                ui::automations::SLOT_DEPTH
            } else {
                0.0
            };
        cards.push(Card {
            id: node.id.clone(),
            at: at(&node.id),
            size: [CARD_W, height.max(CARD_H)],
            body: card(t, face),
            input_y: Some(INPUT_PORT_Y as f32),
            outputs: ports(Some(kind)),
        });
    }

    let wires = drawable_edges(graph)
        .into_iter()
        .map(|edge| {
            let (mut color, dashed) = port_look(t, edge.port);
            let key = edge.key();
            let run = marks.run.and_then(|o| o.edges.get(&key)).copied();
            let mut width = 1.5;
            let mut dash = dashed.then_some([5.0, 4.0]);
            match run {
                Some(EdgeRun::Taken) => width = 2.25,
                Some(EdgeRun::Live) => {
                    color = c.primary;
                    width = 2.25;
                    dash = Some([6.0, 5.0]);
                }
                Some(EdgeRun::NotTaken) => color = color.alpha(0.3),
                None => {}
            }
            let labelled = matches!(edge.port, Port::True | Port::False | Port::Item);
            let label = labelled.then(|| {
                let (tone, _) = port_look(t, edge.port);
                rune::row().px(4.0).radius(2.0).fill(c.background).child(
                    text(edge.port.as_str())
                        .font(sans(12.0, 400.0, 16.0))
                        .color(if run == Some(EdgeRun::NotTaken) {
                            tone.alpha(0.3)
                        } else {
                            tone
                        }),
                )
            });
            Wire {
                key,
                from: edge.from.clone(),
                port: edge.port.as_str().to_owned(),
                to: edge.to.clone(),
                color,
                width,
                dash,
                label,
            }
        })
        .collect();
    (cards, wires)
}
