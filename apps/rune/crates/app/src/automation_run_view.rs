//! A run filling the pane (`AutomationRunDetail.tsx`): the header with Cancel run, the notes,
//! each step's status on the run's own graph, and the step list.

use model::automations::format::{
    format_date_time, format_duration, run_presentation, trigger_label,
};
use model::automations::layout::step_order;
use model::automations::run::{self as arun, StepView};
use model::automations::{Run, RunStatus, kinds};
use rune::{El, Tx, col, el, row, text};
use theme::Tokens;
use troupe_flow::canvas::{Canvas, FlowEvent, FlowState, Selection};
use ui::automations::{glyph, step_glyph, tone};
use ui::type_::{TEXT_SM, TEXT_XS, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::automation_canvas::{Marks, cards_and_wires, look};
use crate::automations_view::{now_ms, offset_min};

fn run_lens(s: &mut State) -> &mut FlowState {
    &mut s.automations.run_flow
}

fn run_event(s: &mut State, event: FlowEvent, _: &mut Tx) {
    if let FlowEvent::Select(Some(Selection::Card(id))) = event {
        s.automations.inspected = Some(id);
    }
}

pub(crate) fn run_view(s: &State, t: &'static Tokens, run: &Run, phone: bool) -> El {
    let c = &t.colors;
    let ui = &s.automations;
    let steps = ui.steps.of(&run.id);
    let overlay = arun::overlay(run, steps.map_or(&[][..], Vec::as_slice), &ui.picks);
    let (label, tn) = run_presentation(run);
    let mut meta = format!(
        "{} · {}",
        trigger_label(run.trigger),
        format_date_time(run.started_at, now_ms(), offset_min())
    );
    if let Some(end) = run.finished_at {
        meta.push_str(&format!(
            " · took {}",
            format_duration(end - run.started_at)
        ));
    }
    meta.push_str(&format!(" · revision {}", run.revision));
    let titles = if phone {
        col().grow(1.0).min_w(0.0)
    } else {
        row().grow(1.0).min_w(0.0).gap(12.0).items_center()
    };
    let mut header = row()
        .w_full()
        .min_h(48.0)
        .px(8.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            Button::icon_only(
                glyph("ArrowLeft"),
                "Back to the automation",
                if phone {
                    ButtonSize::Touch
                } else {
                    ButtonSize::IconSm
                },
            )
            .variant(ButtonVariant::Ghost)
            .build(t)
            .key("automation-run-back")
            .on_press(|s: &mut State| s.automation_close_run()),
        )
        .child(
            titles
                .child(text(label).font(sans(14.0, 500.0, 20.0)).color(tone(t, tn)))
                .child(
                    text(meta)
                        .font(TEXT_XS)
                        .color(c.muted_foreground)
                        .truncate(),
                ),
        );
    if run.status == RunStatus::Running {
        header = header.child(
            Button::new("Cancel run")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .disabled(ui.cancelling)
                .build(t)
                .key("automation-cancel-run")
                .on_press(|s: &mut State| s.automation_cancel_run()),
        );
    }
    let mut notes: Vec<El> = Vec::new();
    if let Some(reason) = &run.stop_reason {
        notes.push(text(reason.clone()).font(TEXT_XS).color(c.destructive));
    }
    if let Some(summary) = &run.summary {
        notes.push(text(summary.clone()).font(TEXT_XS).color(c.foreground));
    }
    if let Some(error) = ui
        .steps
        .error
        .as_ref()
        .filter(|_| ui.steps.key.as_deref() == Some(&run.id))
    {
        notes.push(
            text(format!("Could not load the steps: {error}"))
                .font(TEXT_XS)
                .color(c.destructive),
        );
    }
    let inspected = ui.inspected.as_ref().map(|id| Selection::Card(id.clone()));
    let marks = Marks {
        issues: &[],
        run: steps.is_some().then_some(&overlay),
        selection: inspected.as_ref(),
        dimmed: &|_| false,
        now_ms: now_ms(),
        offset_min: offset_min(),
    };
    let layout = ui
        .detail
        .data
        .as_ref()
        .map(|d| d.automation.layout.clone())
        .unwrap_or_default();
    let (cards, wires) = cards_and_wires(
        t,
        &run.graph,
        &model::automations::Trigger::Manual,
        &layout,
        &marks,
    );
    let canvas = Canvas::new("automation-run-canvas", run_lens, run_event, look(t))
        .wheel_pans(!phone)
        .readable(if phone { 0.85 } else { 0.75 })
        .cards(cards)
        .wires(wires)
        .build(&ui.run_flow);
    let mut page = col()
        .key("automation-run")
        .w_full()
        .h_full()
        .fill(c.background)
        .child(header);
    if !notes.is_empty() {
        page = page.child(
            col()
                .w_full()
                .px(16.0)
                .py(8.0)
                .gap(4.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
                .children(notes),
        );
    }
    page.child(el().w_full().grow(1.0).min_h(0.0).child(canvas))
        .child(step_list(t, run, &overlay, steps.is_none(), ui, phone))
}

fn step_list(
    t: &Tokens,
    run: &Run,
    overlay: &arun::Overlay,
    loading: bool,
    ui: &crate::automations::AutomationsUi,
    phone: bool,
) -> El {
    let c = &t.colors;
    let open = !ui.steps_closed;
    let toggle = Button::icon_only(
        glyph(if open { "ChevronDown" } else { "ChevronUp" }),
        if open {
            "Collapse the steps"
        } else {
            "Expand the steps"
        },
        if phone {
            ButtonSize::Touch
        } else {
            ButtonSize::IconSm
        },
    )
    .variant(ButtonVariant::Ghost)
    .build(t)
    .key("automation-steps-toggle")
    .on_press(|s: &mut State| s.automations.steps_closed = !s.automations.steps_closed);
    let section = col()
        .key("automation-steps")
        .w_full()
        .max_h(320.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .child(
            row()
                .w_full()
                .px(8.0)
                .py(4.0)
                .items_center()
                .child(
                    text("Steps")
                        .font(sans(12.0, 500.0, 16.0))
                        .color(c.muted_foreground)
                        .px(8.0)
                        .grow(1.0),
                )
                .child(toggle),
        );
    if !open {
        return section;
    }
    let row_for = |key: String,
                   title: String,
                   kind: &str,
                   status: StepView,
                   chip: Option<String>,
                   duration: Option<String>,
                   target: String| {
        let selected = ui.inspected.as_deref() == Some(target.as_str());
        let mut line = row()
            .key(key)
            .role("button")
            .label(format!("{title}, {kind}, {}", status.label()))
            .w_full()
            .min_h(if phone { 44.0 } else { 36.0 })
            .px(8.0)
            .gap(8.0)
            .radius(8.0)
            .items_center()
            .fill(if selected { c.muted } else { c.background })
            .hover(c.foreground.alpha(0.03))
            .child(if loading {
                el().size(16.0, 16.0)
            } else {
                step_glyph(t, status, 16.0)
            })
            .child(
                text(title)
                    .font(TEXT_SM)
                    .color(c.foreground)
                    .truncate()
                    .min_w(0.0),
            )
            .child(
                text(kind.to_owned())
                    .font(TEXT_XS)
                    .color(c.muted_foreground),
            );
        if let Some(chip) = chip {
            line = line.child(
                row()
                    .px(6.0)
                    .radius(10.0)
                    .border(1.0, c.border)
                    .child(text(chip).font(sans(12.0, 500.0, 20.0)).color(c.foreground)),
            );
        }
        line.child(el().grow(1.0))
            .child(
                text(duration.unwrap_or_default())
                    .font(TEXT_XS)
                    .color(c.muted_foreground),
            )
            .on_press(move |s: &mut State| s.automations.inspected = Some(target.clone()))
    };
    let trigger_status = overlay
        .cards
        .get(model::automations::TRIGGER)
        .map_or(StepView::Succeeded, |r| r.status);
    let mut list = col()
        .w_full()
        .px(8.0)
        .pb(8.0)
        .role("list")
        .label("Steps")
        .child(row_for(
            "automation-step-trigger".to_owned(),
            trigger_label(overlay.fired.by).to_owned(),
            "Trigger",
            trigger_status,
            None,
            None,
            model::automations::TRIGGER.to_owned(),
        ));
    for node in step_order(&run.graph) {
        let status = overlay
            .cards
            .get(&node.id)
            .map_or(StepView::Pending, |r| r.status);
        let (chip, duration) = match overlay.loops.get(&node.id) {
            Some(lp) => {
                let n = lp.iterations.len();
                let failed = if lp.failed > 0 {
                    format!(" · {} failed", lp.failed)
                } else {
                    String::new()
                };
                (
                    Some(format!(
                        "{n} {}{failed}",
                        if n == 1 { "iteration" } else { "iterations" }
                    )),
                    None,
                )
            }
            None => {
                let row = overlay.rows.get(&node.id);
                let took = row.and_then(|r| Some(format_duration(r.finished_at? - r.started_at?)));
                (None, took)
            }
        };
        list = list.child(row_for(
            format!("automation-step-{}", node.id),
            node.label.clone(),
            kinds::label(node.spec.kind()),
            status,
            chip,
            duration,
            node.id.clone(),
        ));
    }
    section.child(
        ui::base::scroll_area::scroll_area(t, "automation-steps-scroll", list)
            .grow(1.0)
            .min_h(0.0),
    )
}
