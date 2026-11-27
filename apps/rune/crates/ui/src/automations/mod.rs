//! The automation parts (`components/automations/`): the state pill, status tones and glyphs,
//! and a step's card face on the canvas (`canvas/nodes/NodeCard.tsx`). The app assembles the
//! list, the detail, the run view and the editor from these.

use model::automations::format::{State, Tone};
use model::automations::run::{LoopCount, StepView};
use model::automations::{NodeKind, Port};
use rune::{Color, El, Icon, col, el, icon, row, text};
use theme::Tokens;

use crate::type_::sans;

/// A card's size on the canvas (`ports.ts` `CARD`), and the slots agent and MCP tool cards
/// hang below it.
pub const CARD_W: f32 = 192.0;
pub const CARD_H: f32 = 64.0;
pub const SLOT_DEPTH: f32 = 56.0;

/// A `components/icons` export by name.
pub fn glyph(name: &str) -> &'static Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

pub fn kind_icon(kind: NodeKind) -> &'static Icon {
    glyph(match kind {
        NodeKind::Http => "Globe",
        NodeKind::Agent => "Bot",
        NodeKind::Transform => "Braces",
        NodeKind::Condition => "GitBranch",
        NodeKind::Fail => "XCircle",
        NodeKind::ForEach => "Repeat",
        NodeKind::Merge => "GitMerge",
        NodeKind::Wait => "Hourglass",
        NodeKind::Approval => "Hand",
        NodeKind::McpTool => "Wrench",
    })
}

pub fn tone(t: &Tokens, tone: Tone) -> Color {
    let c = &t.colors;
    match tone {
        Tone::Primary => c.primary,
        Tone::Success => c.success,
        Tone::Destructive => c.destructive,
        Tone::Muted => c.muted_foreground,
        Tone::Warning => c.warning,
    }
}

pub fn step_tone(view: StepView) -> Tone {
    match view {
        StepView::Pending | StepView::Skipped | StepView::Cancelled => Tone::Muted,
        StepView::Running => Tone::Primary,
        StepView::Succeeded => Tone::Success,
        StepView::Failed => Tone::Destructive,
        StepView::Waiting => Tone::Warning,
    }
}

/// A port's colour and whether its edge is dashed (`PORT_PRESENTATION`).
pub fn port_look(t: &Tokens, port: Port) -> (Color, bool) {
    let c = &t.colors;
    match port {
        Port::Error => (c.destructive, true),
        Port::True => (c.success, false),
        Port::Out | Port::False | Port::Item => (c.muted_foreground, false),
    }
}

/// The 11 px pill the list rows and headers show: Armed, Needs approval or Disabled.
pub fn state_pill(t: &Tokens, state: State) -> El {
    let c = &t.colors;
    let (fill, ink) = match state {
        State::Armed => (c.success.alpha(0.2), c.success),
        State::NeedsApproval => (c.warning.alpha(0.2), c.warning),
        State::Disabled => (c.muted, c.muted_foreground),
    };
    row()
        .h(20.0)
        .px(8.0)
        .radius(10.0)
        .items_center()
        .fill(fill)
        .child(text(state.label()).font(sans(11.0, 500.0, 20.0)).color(ink))
}

/// A step status glyph (`StepGlyph`).
pub fn step_glyph(t: &Tokens, view: StepView, size: f32) -> El {
    let name = match view {
        StepView::Pending => "CircleDashed",
        StepView::Running => "Loader2",
        StepView::Succeeded => "CircleCheck",
        StepView::Failed => "XCircle",
        StepView::Skipped => "CircleMinus",
        StepView::Waiting => "Hourglass",
        StepView::Cancelled => "Square",
    };
    icon(glyph(name), size)
        .color(tone(t, step_tone(view)))
        .label(view.label())
}

/// What one card shows.
pub struct Face {
    pub icon: &'static Icon,
    pub title: String,
    pub subtitle: String,
    /// The subtitle names a broken setting.
    pub problem: bool,
    pub issues: usize,
    pub status: Option<StepView>,
    pub selected: bool,
    pub loop_: Option<LoopCount>,
    /// Labelled values hung below an agent or MCP tool card.
    pub slots: Vec<(&'static str, String)>,
    /// Port names drawn beside their handle (`error`), with the handle's centre.
    pub dimmed: bool,
}

/// One card on the canvas, `CARD_W` wide and `CARD_H` tall, slots below it.
pub fn card(t: &Tokens, face: Face) -> El {
    let c = &t.colors;
    let live = face.status == Some(StepView::Running);
    let edge = if live {
        c.primary.alpha(0.55)
    } else if face.selected {
        c.foreground.alpha(0.4)
    } else {
        c.border
    };
    let mut title = row().w_full().gap(4.0).items_center().child(
        text(face.title)
            .font(sans(14.0, 500.0, 20.0))
            .color(c.foreground)
            .truncate()
            .grow(1.0)
            .min_w(0.0),
    );
    if face.issues > 0 {
        title = title.child(
            row()
                .gap(2.0)
                .items_center()
                .child(icon(glyph("AlertCircle"), 12.0).color(c.destructive))
                .child(
                    text(face.issues.to_string())
                        .font(sans(12.0, 500.0, 16.0))
                        .color(c.destructive),
                ),
        );
    }
    if let Some(status) = face.status.filter(|s| *s != StepView::Pending) {
        title = title.child(step_glyph(t, status, 16.0));
    }
    let body = row()
        .w(CARD_W)
        .h(CARD_H)
        .pl(10.0)
        .pr(6.0)
        .gap(8.0)
        .items_center()
        .radius(12.0)
        .border(1.0, edge)
        .fill(if face.selected { c.keycap_fill } else { c.card })
        .child(
            el().size(32.0, 32.0)
                .radius(8.0)
                .center()
                .fill(if face.selected {
                    c.foreground.alpha(0.1)
                } else {
                    c.muted
                })
                .child(icon(face.icon, 16.0).color(if face.selected {
                    c.foreground
                } else {
                    c.secondary_foreground
                })),
        )
        .child(
            col().grow(1.0).min_w(0.0).child(title).child(
                text(face.subtitle)
                    .font(sans(12.0, 400.0, 16.0))
                    .color(if face.problem {
                        c.destructive
                    } else {
                        c.muted_foreground
                    })
                    .truncate(),
            ),
        );
    let mut card = el().size(CARD_W, CARD_H).child(body.absolute(0.0, 0.0));
    if let Some(lp) = face.loop_ {
        let words = format!(
            "{} {}",
            lp.iterations,
            if lp.iterations == 1 {
                "iteration"
            } else {
                "iterations"
            }
        );
        let mut pill = row()
            .h(20.0)
            .px(8.0)
            .radius(10.0)
            .items_center()
            .border(1.0, c.border)
            .fill(c.card)
            .child(
                text(words)
                    .font(sans(12.0, 500.0, 20.0))
                    .color(c.foreground),
            );
        if lp.failed > 0 {
            pill = pill.child(
                text(format!(" · {} failed", lp.failed))
                    .font(sans(12.0, 500.0, 20.0))
                    .color(c.destructive),
            );
        }
        card = card.child(pill.pin(None, Some(-10.0), Some(12.0), None));
    }
    for (index, (label, value)) in face.slots.into_iter().enumerate() {
        let centre = if index == 0 { 0.24 } else { 0.76 } * CARD_W;
        card = card.child(
            col()
                .absolute(centre - 46.0, CARD_H)
                .w(92.0)
                .items_center()
                .child(el().size(1.0, 12.0).fill(c.field_border))
                .child(
                    row()
                        .h(18.0)
                        .px(8.0)
                        .radius(9.0)
                        .items_center()
                        .border(1.0, c.border)
                        .fill(c.card)
                        .child(
                            text(value)
                                .font(sans(12.0, 600.0, 18.0))
                                .color(c.foreground)
                                .truncate(),
                        ),
                )
                .child(
                    text(label)
                        .font(sans(12.0, 400.0, 16.0))
                        .color(c.muted_foreground),
                ),
        );
    }
    if face.dimmed || matches!(face.status, Some(StepView::Skipped | StepView::Cancelled)) {
        card = card.opacity(0.5);
    }
    card.a11y_hidden()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
