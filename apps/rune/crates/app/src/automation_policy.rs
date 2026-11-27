//! The run policy card (`RunPolicyCard.tsx`): what the automation will do once armed, as the
//! host derived it, and the approve footer.

use model::automations::Detail;
use model::automations::format::{self, State as AState, format_date_time, format_duration};
use rune::{El, col, icon, row, text};
use theme::Tokens;
use ui::automations::glyph;
use ui::type_::{TEXT_SM, TEXT_XS, sans};
use ui::{Button, ButtonSize};

use crate::State;
use crate::automations_view::{now_ms, offset_min};

fn policy_row(t: &Tokens, term: &str, value: El) -> El {
    row()
        .w_full()
        .px(16.0)
        .py(10.0)
        .gap(12.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .child(
            text(term.to_owned())
                .font(TEXT_XS)
                .color(t.colors.muted_foreground)
                .w(128.0),
        )
        .child(col().grow(1.0).min_w(0.0).child(value))
}

pub(crate) fn policy_card(t: &Tokens, detail: &Detail, approving: bool) -> El {
    let c = &t.colors;
    let (a, p) = (&detail.automation, &detail.policy);
    let label = |id: &str| {
        a.graph
            .node(id)
            .map_or_else(|| id.to_owned(), |node| node.label.clone())
    };
    let plain = |words: String| text(words).font(TEXT_SM).color(c.foreground);
    let none = || text("None").font(TEXT_SM).color(c.muted_foreground);
    let next = if p.next_runs.is_empty() {
        text("None scheduled")
            .font(TEXT_SM)
            .color(c.muted_foreground)
    } else {
        col().children(
            p.next_runs
                .iter()
                .take(3)
                .map(|at| plain(format_date_time(*at, now_ms(), offset_min()))),
        )
    };
    let agents = if p.agents.is_empty() {
        none()
    } else {
        col().gap(4.0).children(p.agents.iter().map(|agent| {
            let mut words = format!(" · {}", agent.agent_id);
            if let Some(model) = &agent.model {
                words.push_str(&format!(" ({model})"));
            }
            words.push_str(&format!(
                " · times out after {}",
                format_duration(agent.timeout_ms as f64)
            ));
            words.push_str(&format!(" · {}", format::agent_tools_label(&agent.tools)));
            if agent.tools.contains(&model::automations::AgentTool::Other) {
                let servers: Vec<&str> =
                    agent.mcp_servers.iter().map(|m| m.name.as_str()).collect();
                words.push_str(&if servers.is_empty() {
                    " · no MCP servers".to_owned()
                } else {
                    format!(" · MCP servers offered: {}", servers.join(", "))
                });
            }
            row()
                .wrap()
                .child(plain(label(&agent.node_id)))
                .child(text(words).font(TEXT_SM).color(c.muted_foreground))
        }))
    };
    let per_node = |entries: Vec<(&str, &Vec<String>)>| {
        if entries.is_empty() {
            return none();
        }
        col()
            .gap(4.0)
            .children(entries.into_iter().map(|(node, items)| {
                row()
                    .wrap()
                    .gap(4.0)
                    .items_center()
                    .child(plain(label(node)))
                    .children(items.iter().map(|item| {
                        row().px(6.0).py(2.0).radius(6.0).fill(c.muted).child(
                            text(item.clone())
                                .font(ui::type_::MONO_XS)
                                .color(c.secondary_foreground),
                        )
                    }))
            }))
    };
    let mut rows = col()
        .w_full()
        .child(policy_row(t, "Trigger", plain(p.trigger_summary.clone())))
        .child(policy_row(t, "Next runs", next))
        .child(policy_row(t, "Agents", agents))
        .child(policy_row(
            t,
            "Secrets",
            per_node(
                p.secrets
                    .iter()
                    .map(|e| (e.node_id.as_str(), &e.names))
                    .collect(),
            ),
        ))
        .child(policy_row(
            t,
            "Outbound hosts",
            per_node(
                p.outbound_hosts
                    .iter()
                    .map(|e| (e.node_id.as_str(), &e.hosts))
                    .collect(),
            ),
        ));
    if !p.mcp_tools.is_empty() {
        rows = rows.child(policy_row(
            t,
            "MCP tools",
            col().gap(4.0).children(p.mcp_tools.iter().map(|call| {
                let on = match &call.server_name {
                    None => format!(" on {} (not configured)", call.server_id),
                    Some(name) => format!(" on {name} ({})", call.server_id),
                };
                row().wrap().child(plain(label(&call.node_id))).child(
                    text(format!(" · calls MCP tool {}{on}", call.tool))
                        .font(TEXT_SM)
                        .color(c.muted_foreground),
                )
            })),
        ));
    }
    rows = rows.child(policy_row(t, "On failure", plain(p.failure_policy.clone())));
    let mut card = col()
        .key("automation-run-policy")
        .w_full()
        .radius(12.0)
        .border(1.0, c.border)
        .fill(c.card)
        .child(
            row()
                .w_full()
                .px(16.0)
                .py(12.0)
                .gap(8.0)
                .items_center()
                .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
                .child(icon(glyph("ShieldCheck"), 16.0).color(c.muted_foreground))
                .child(
                    text("Run policy")
                        .font(sans(14.0, 500.0, 20.0))
                        .color(c.foreground)
                        .grow(1.0),
                )
                .child(
                    text(format!("Revision {}", a.revision))
                        .font(TEXT_XS)
                        .color(c.muted_foreground),
                ),
        )
        .child(rows);
    if p.untrusted_flow {
        card = card.child(
            row().mx(16.0).mb(12.0).px(12.0).py(8.0).gap(8.0).radius(8.0).fill(c.warning.alpha(0.1)).child(icon(glyph("AlertTriangle"), 14.0).color(c.warning)).child(
                text("An agent reads content fetched from the web or returned by an MCP tool, and a later step sends data out or calls a tool. Fetched pages can carry instructions, so check the outbound hosts and MCP tools before you approve.")
                    .font(TEXT_XS)
                    .color(c.warning)
                    .grow(1.0),
            ),
        );
    }
    if format::state(a) == AState::NeedsApproval {
        let why = if !detail.issues.is_empty() {
            "Fix the issues above before approving. Ask the agent that built it to fix them."
                .to_owned()
        } else if let Some(approved) = a.approved_revision {
            format!(
                "Revision {approved} was approved. This revision needs your approval before it runs again."
            )
        } else {
            "Not approved yet. It will not run until you approve this revision.".to_owned()
        };
        let label = if approving {
            "Approving…"
        } else if a.enabled {
            "Approve and arm"
        } else {
            "Approve"
        };
        card = card.child(
            row()
                .w_full()
                .px(16.0)
                .py(12.0)
                .gap(8.0)
                .items_center()
                .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
                .child(
                    text(why)
                        .font(TEXT_XS)
                        .color(c.muted_foreground)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Button::new(label)
                        .size(ButtonSize::Sm)
                        .disabled(!detail.issues.is_empty() || approving)
                        .build(t)
                        .key("automation-approve")
                        .on_press(|s: &mut State| s.automation_approve()),
                ),
        );
    }
    card
}
