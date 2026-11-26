//! The inspector's text fields (`canvas/inspector/*`): what each one holds for a card's kind,
//! read from the draft and written back into a spec, and the choices that change a spec at once.

use model::automations::draft::Draft;
use model::automations::{
    ConditionOp, HttpMethod, MergeMode, NodeSpec, Schedule, TRIGGER, Trigger,
};
use rune::TextEdit;

/// The inspector's text fields for one card; what each holds depends on the card's kind.
#[derive(Debug, Default)]
pub struct Fields {
    pub card: String,
    pub label: TextEdit,
    pub a: TextEdit,
    pub b: TextEdit,
    pub c: TextEdit,
}

fn ms_text(ms: u64, unit: f64) -> String {
    let v = ms as f64 / unit;
    if v.fract() == 0.0 {
        format!("{}", v as u64)
    } else {
        format!("{v}")
    }
}

/// The text fields for `id` as the draft holds it.
pub(crate) fn fields_of(draft: &Draft, id: &str) -> Fields {
    let mut f = Fields {
        card: id.to_owned(),
        ..Fields::default()
    };
    let set = |e: &mut TextEdit, s: &str| *e = TextEdit::new(s);
    if id == TRIGGER {
        match &draft.doc.trigger {
            Trigger::Schedule {
                schedule: Schedule::Every { every_ms, .. },
            } => set(&mut f.a, &ms_text(*every_ms as u64, 60_000.0)),
            Trigger::Schedule {
                schedule: Schedule::Cron { expr, timezone },
            } => {
                set(&mut f.a, expr);
                set(&mut f.b, timezone);
            }
            _ => {}
        }
        return f;
    }
    let Some(node) = draft.doc.graph.node(id) else {
        return f;
    };
    set(&mut f.label, &node.label);
    match &node.spec {
        NodeSpec::Http {
            url,
            allowed_hosts,
            body,
            ..
        } => {
            set(&mut f.a, url);
            set(&mut f.b, &allowed_hosts.join(", "));
            set(&mut f.c, body.as_deref().unwrap_or_default());
        }
        NodeSpec::Agent {
            agent_id,
            prompt,
            model,
            ..
        } => {
            set(&mut f.a, agent_id);
            set(&mut f.b, prompt);
            set(&mut f.c, model.as_deref().unwrap_or_default());
        }
        NodeSpec::Condition { left, right, .. } => {
            set(&mut f.a, left);
            set(&mut f.b, right.as_deref().unwrap_or_default());
        }
        NodeSpec::Fail { message } => set(&mut f.a, message),
        NodeSpec::ForEach {
            items,
            max_iterations,
        } => {
            set(&mut f.a, items);
            set(&mut f.b, &max_iterations.to_string());
        }
        NodeSpec::Wait { duration_ms } => set(&mut f.a, &ms_text(*duration_ms, 1000.0)),
        NodeSpec::Approval {
            message,
            timeout_ms,
        } => {
            set(&mut f.a, message);
            set(&mut f.b, &ms_text(*timeout_ms, 3_600_000.0));
        }
        NodeSpec::McpTool {
            server_id, tool, ..
        } => {
            set(&mut f.a, server_id);
            set(&mut f.b, tool);
        }
        NodeSpec::Transform { .. } | NodeSpec::Merge { .. } => {}
    }
    f
}

/// `spec` with the inspector's text fields read into it; a number that does not parse keeps
/// the old value.
pub(crate) fn spec_from(spec: &NodeSpec, f: &Fields) -> NodeSpec {
    let (a, b, c) = (f.a.text.clone(), f.b.text.clone(), f.c.text.clone());
    let num = |text: &str, unit: f64, old: u64| {
        text.trim()
            .parse::<f64>()
            .map_or(old, |v| (v * unit).round() as u64)
    };
    let mut spec = spec.clone();
    match &mut spec {
        NodeSpec::Http {
            url,
            allowed_hosts,
            body,
            ..
        } => {
            *url = a;
            *allowed_hosts = b
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(str::to_owned)
                .collect();
            *body = (!c.is_empty()).then_some(c);
        }
        NodeSpec::Agent {
            agent_id,
            prompt,
            model,
            ..
        } => {
            *agent_id = a;
            *prompt = b;
            *model = (!c.trim().is_empty()).then_some(c);
        }
        NodeSpec::Condition { left, right, .. } => {
            *left = a;
            *right = Some(b);
        }
        NodeSpec::Fail { message } => *message = a,
        NodeSpec::ForEach {
            items,
            max_iterations,
        } => {
            *items = a;
            *max_iterations = num(&b, 1.0, *max_iterations);
        }
        NodeSpec::Wait { duration_ms } => *duration_ms = num(&a, 1000.0, *duration_ms),
        NodeSpec::Approval {
            message,
            timeout_ms,
        } => {
            *message = a;
            *timeout_ms = num(&b, 3_600_000.0, *timeout_ms);
        }
        NodeSpec::McpTool {
            server_id, tool, ..
        } => {
            *server_id = a;
            *tool = b;
        }
        NodeSpec::Transform { .. } | NodeSpec::Merge { .. } => {}
    }
    spec
}

/// The trigger kinds the trigger inspector switches between.
pub fn trigger_of(kind: &str) -> Trigger {
    match kind {
        "every" => Trigger::Schedule {
            schedule: Schedule::Every {
                every_ms: 3_600_000.0,
                anchor_ms: 0.0,
            },
        },
        "cron" => Trigger::Schedule {
            schedule: Schedule::Cron {
                expr: "0 9 * * 1-5".to_owned(),
                timezone: "UTC".to_owned(),
            },
        },
        _ => Trigger::Manual,
    }
}

pub fn set_method(spec: &mut NodeSpec, to: HttpMethod) {
    if let NodeSpec::Http { method, .. } = spec {
        *method = to;
    }
}

pub fn set_op(spec: &mut NodeSpec, to: ConditionOp) {
    if let NodeSpec::Condition { op, .. } = spec {
        *op = to;
    }
}

pub fn set_mode(spec: &mut NodeSpec, to: MergeMode) {
    if let NodeSpec::Merge { mode } = spec {
        *mode = to;
    }
}
