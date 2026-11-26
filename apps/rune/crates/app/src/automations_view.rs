//! The Automations tab (`AutomationsPanel.tsx`): the list beside the selected automation on
//! desktop, a list then detail stack on the phone; the detail (`AutomationDetail.tsx`,
//! `RunPolicyCard.tsx`) with its read-only diagram and runs; and a run filling the pane
//! (`AutomationRunDetail.tsx`) with each step's status on its own graph and the step list.

use model::automations::format::{self, format_date_time, format_duration, run_presentation};
use model::automations::{Automation, Detail, Run};
use model::workspace::Tab;
use rune::{El, Tx, col, el, icon, row, text};
use theme::Tokens;
use troupe_flow::canvas::{Canvas, FlowEvent, FlowState};
use ui::automations::{glyph, state_pill, tone};
use ui::overlay::{ConfirmDialog, ConfirmVariant};
use ui::type_::{TEXT_SM, TEXT_XS, sans};
use ui::{Button, ButtonSize, ButtonVariant, Switch};

use crate::State;
use crate::automation_canvas::{Marks, cards_and_wires, look};
use crate::automations::DELETE_CONFIRM;

/// `EMPTY_EXAMPLE`.
const EMPTY_EXAMPLE: &str = "every weekday at 9, summarize open PRs";
/// The detail diagram's height (`AutomationCanvas`: `h-[440px]`, `h-80` on a phone).
const DIAGRAM_H: [f32; 2] = [440.0, 320.0];

/// The local zone's offset from UTC, in minutes.
pub(crate) fn offset_min() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        -(js_sys::Date::new_0().get_timezone_offset() as i64)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

pub(crate) fn now_ms() -> f64 {
    crate::editor_host::epoch_ms()
}

pub fn tab_body(s: &State, t: &'static Tokens, tab: &Tab, phone: bool) -> Option<El> {
    let Tab::Automations { project_id, .. } = tab else {
        return None;
    };
    if s.automations.project.as_deref() != Some(project_id) {
        return Some(message(t, "Loading…"));
    }
    Some(panel(s, t, phone))
}

fn message(t: &Tokens, words: &str) -> El {
    col()
        .px(16.0)
        .py(24.0)
        .child(text(words).font(TEXT_SM).color(t.colors.muted_foreground))
}

fn panel(s: &State, t: &'static Tokens, phone: bool) -> El {
    let ui = &s.automations;
    let c = &t.colors;
    if let Some(run) = ui.open_run() {
        return crate::automation_run_view::run_view(s, t, run, phone);
    }
    let list = list_view(s, t, phone);
    let detail = detail_view(s, t, phone);
    if phone {
        let mut page = col()
            .key("automations")
            .w_full()
            .h_full()
            .fill(c.background);
        let Some(detail) = detail.filter(|_| ui.chosen.is_some()) else {
            return page.child(scroll(t, "automations-list-scroll", list));
        };
        page = page.child(
            row()
                .w_full()
                .h(52.0)
                .px(4.0)
                .items_center()
                .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
                .child(
                    Button::new("Automations")
                        .variant(ButtonVariant::Ghost)
                        .icon(glyph("ArrowLeft"))
                        .build(t)
                        .key("automations-back")
                        .h(44.0)
                        .on_press(|s: &mut State| s.automation_select(None)),
                ),
        );
        return page.child(scroll(
            t,
            "automations-detail-scroll",
            col().w_full().padding(16.0).child(detail),
        ));
    }
    if ui.list.data.as_ref().is_some_and(Vec::is_empty) {
        return col()
            .key("automations")
            .w_full()
            .h_full()
            .fill(c.background)
            .items_center()
            .justify_center()
            .child(empty(t));
    }
    let aside = col()
        .w(288.0)
        .h_full()
        .border_sides([0.0, 1.0, 0.0, 0.0], c.border)
        .child(
            row()
                .w_full()
                .h(40.0)
                .px(16.0)
                .gap(8.0)
                .items_center()
                .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
                .child(icon(glyph("CalendarClock"), 14.0).color(c.muted_foreground))
                .child(
                    text("Automations")
                        .font(sans(14.0, 500.0, 20.0))
                        .color(c.foreground),
                ),
        )
        .child(scroll(t, "automations-list-scroll", list));
    let main = col()
        .w_full()
        .max_w(768.0)
        .padding(24.0)
        .gap(16.0)
        .children(detail);
    row()
        .key("automations")
        .w_full()
        .h_full()
        .fill(c.background)
        .child(aside)
        .child(
            scroll(
                t,
                "automations-detail-scroll",
                row().w_full().justify_center().child(main),
            )
            .grow(1.0)
            .min_w(0.0),
        )
}

fn scroll(t: &Tokens, key: &str, content: El) -> El {
    ui::base::scroll_area::scroll_area(t, key, content)
        .grow(1.0)
        .min_h(0.0)
}

fn empty(t: &Tokens) -> El {
    let c = &t.colors;
    col()
        .key("automations-empty")
        .px(24.0)
        .py(48.0)
        .gap(12.0)
        .items_center()
        .child(icon(glyph("CalendarClock"), 24.0).color(c.muted_foreground))
        .child(
            col()
                .max_w(320.0)
                .gap(4.0)
                .items_center()
                .child(text("No automations yet").font(sans(14.0, 500.0, 20.0)).color(c.foreground))
                .child(
                    text(format!(
                        "Automations are built in chat. Ask an agent, for example: “{EMPTY_EXAMPLE}”. It shows up here for you to review and approve."
                    ))
                    .font(TEXT_XS)
                    .color(c.muted_foreground)
                    .align(rune::Align::Center),
                ),
        )
}

fn list_view(s: &State, t: &'static Tokens, phone: bool) -> El {
    let ui = &s.automations;
    let Some(list) = ui.list.data.as_ref() else {
        let words = ui.list.error.as_ref().map_or_else(
            || "Loading…".to_owned(),
            |e| format!("Could not load automations: {e}"),
        );
        return message(t, &words);
    };
    if list.is_empty() {
        return empty(t);
    }
    let selected = if phone {
        ui.chosen.as_deref()
    } else {
        ui.selected()
    };
    col()
        .w_full()
        .padding(8.0)
        .gap(4.0)
        .role("list")
        .label("Automations")
        .children(
            list.iter()
                .map(|a| list_row(t, a, selected == Some(a.id.as_str()))),
        )
}

fn list_row(t: &Tokens, a: &Automation, selected: bool) -> El {
    let c = &t.colors;
    let next = a.next_run_at.map_or_else(
        || "No next run".to_owned(),
        |at| format!("Next {}", format_date_time(at, now_ms(), offset_min())),
    );
    let mut meta = row()
        .w_full()
        .gap(12.0)
        .wrap()
        .child(text(next).font(TEXT_XS).color(c.muted_foreground));
    meta = meta.child(match &a.last_run {
        Some(last) => {
            let (label, tn) = format::run_status(last.status);
            row()
                .child(text("Last run ").font(TEXT_XS).color(c.muted_foreground))
                .child(text(label.to_lowercase()).font(TEXT_XS).color(tone(t, tn)))
        }
        None => row().child(text("Never run").font(TEXT_XS).color(c.muted_foreground)),
    });
    if a.unseen_failures > 0 {
        let n = a.unseen_failures;
        meta = meta.child(
            row()
                .h(20.0)
                .px(6.0)
                .radius(10.0)
                .items_center()
                .fill(c.destructive.alpha(0.2))
                .child(
                    text(format!(
                        "{n} new {}",
                        if n == 1 { "failure" } else { "failures" }
                    ))
                    .font(sans(11.0, 500.0, 20.0))
                    .color(c.destructive),
                ),
        );
    }
    let id = a.id.clone();
    col()
        .key(format!("automation-row-{}", a.id))
        .role("button")
        .label(a.name.clone())
        .w_full()
        .min_h(44.0)
        .px(12.0)
        .py(10.0)
        .gap(4.0)
        .radius(8.0)
        .fill(if selected {
            c.keycap_fill
        } else {
            c.background
        })
        .hover(c.foreground.alpha(0.03))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_center()
                .child(
                    text(a.name.clone())
                        .font(sans(14.0, 500.0, 20.0))
                        .color(c.foreground)
                        .truncate()
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(state_pill(t, format::state(a))),
        )
        .child(
            text(format::trigger_summary(&a.trigger, now_ms(), offset_min()))
                .font(TEXT_XS)
                .color(c.muted_foreground)
                .truncate(),
        )
        .child(meta)
        .on_press(move |s: &mut State| s.automation_select(Some(id.clone())))
}

fn detail_view(s: &State, t: &'static Tokens, phone: bool) -> Option<El> {
    let ui = &s.automations;
    let id = ui.selected()?;
    let Some(detail) = ui.detail.of(id) else {
        let words = ui.detail.error.as_ref().map_or_else(
            || "Loading…".to_owned(),
            |e| format!("Could not load it: {e}"),
        );
        return Some(message(t, &words));
    };
    Some(detail_body(s, t, detail, phone))
}

fn detail_body(s: &State, t: &'static Tokens, detail: &Detail, phone: bool) -> El {
    let c = &t.colors;
    let ui = &s.automations;
    let a = &detail.automation;
    let busy = ui.busy.as_ref();
    let header = col()
        .w_full()
        .gap(12.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_center()
                .child(
                    text(a.name.clone())
                        .font(sans(16.0, 500.0, 24.0))
                        .color(c.foreground)
                        .truncate()
                        .min_w(0.0),
                )
                .child(state_pill(t, format::state(a)))
                .child(el().grow(1.0))
                .child(
                    Button::new("Delete")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .icon(glyph("Trash2"))
                        .ink(c.destructive)
                        .disabled(busy == Some(&crate::automations::Act::Delete))
                        .build(t)
                        .key("automation-delete")
                        .on_press(|s: &mut State| s.automation_ask_delete()),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .wrap()
                .items_center()
                .child(
                    row()
                        .key("automation-enabled")
                        .role("switch")
                        .label("Enabled")
                        .min_h(if phone { 44.0 } else { 36.0 })
                        .pr(8.0)
                        .gap(8.0)
                        .items_center()
                        .child(
                            Switch::new(a.enabled)
                                .disabled(busy == Some(&crate::automations::Act::Enable))
                                .build(t),
                        )
                        .child(text("Enabled").font(TEXT_SM).color(c.foreground))
                        .on_press({
                            let enabled = a.enabled;
                            move |s: &mut State| s.automation_set_enabled(!enabled)
                        }),
                )
                .child(
                    Button::new("Run now")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .disabled(busy == Some(&crate::automations::Act::RunNow))
                        .build(t)
                        .key("automation-run-now")
                        .on_press(|s: &mut State| s.automation_run_now()),
                )
                .child(
                    Button::new("Secrets")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .icon(glyph("KeyRound"))
                        .build(t)
                        .key("automation-secrets")
                        .on_press(|s: &mut State| s.toast_later("Automation secrets")),
                ),
        );
    let runs = ui.runs.of(&a.id);
    let awaiting = runs.and_then(|r| r.iter().find(|run| format::parked_approval(run).is_some()));
    let mut body = col().w_full().gap(20.0).child(header);
    if let Some(run) = awaiting {
        let run_id = run.id.clone();
        body = body.child(
            row()
                .w_full()
                .px(12.0)
                .py(8.0)
                .gap(8.0)
                .radius(8.0)
                .items_center()
                .fill(c.warning.alpha(0.1))
                .child(icon(glyph("Hand"), 14.0).color(c.warning))
                .child(
                    text("A run is waiting for your approval.")
                        .font(TEXT_XS)
                        .color(c.warning)
                        .grow(1.0),
                )
                .child(
                    Button::new("Review")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .build(t)
                        .key("automation-review")
                        .on_press(move |s: &mut State| s.automation_open_run(run_id.clone())),
                ),
        );
    }
    if !detail.issues.is_empty() {
        body = body.child(issues(t, detail));
    }
    body = body.child(crate::automation_policy::policy_card(
        t,
        detail,
        busy == Some(&crate::automations::Act::Approve),
    ));
    body = body.child(
        col()
            .w_full()
            .gap(8.0)
            .child(diagram_header(t, phone))
            .child(diagram(s, t, detail, phone)),
    );
    body = body.child(
        col()
            .w_full()
            .gap(8.0)
            .child(
                text("Runs")
                    .font(sans(12.0, 500.0, 16.0))
                    .color(c.muted_foreground),
            )
            .child(run_list(t, runs)),
    );
    body
}

/// The diagram's heading; the editor's Edit button joins it with the editor.
fn diagram_header(t: &Tokens, _phone: bool) -> El {
    row().w_full().h(32.0).items_center().child(
        text("Diagram")
            .font(sans(12.0, 500.0, 16.0))
            .color(t.colors.muted_foreground),
    )
}

fn issues(t: &Tokens, detail: &Detail) -> El {
    let c = &t.colors;
    let n = detail.issues.len();
    let label = |id: &str| {
        detail
            .automation
            .graph
            .node(id)
            .map_or_else(|| id.to_owned(), |node| node.label.clone())
    };
    let heading = if n == 1 {
        "1 issue".to_owned()
    } else {
        format!("{n} issues")
    };
    col()
        .w_full()
        .gap(6.0)
        .child(
            text(format!("{heading} to fix before approval"))
                .font(sans(12.0, 500.0, 16.0))
                .color(c.destructive),
        )
        .children(detail.issues.iter().map(|issue| {
            let mut line = row().grow(1.0).min_w(0.0).wrap();
            if let Some(node) = &issue.node_id {
                line = line.child(
                    text(format!("{}: ", label(node)))
                        .font(sans(12.0, 500.0, 16.0))
                        .color(c.destructive),
                );
            }
            row()
                .w_full()
                .px(12.0)
                .py(8.0)
                .gap(8.0)
                .radius(8.0)
                .fill(c.destructive.alpha(0.1))
                .child(icon(glyph("AlertCircle"), 14.0).color(c.destructive))
                .child(
                    line.child(
                        text(issue.message.clone())
                            .font(TEXT_XS)
                            .color(c.destructive),
                    ),
                )
        }))
}

fn diagram_lens(s: &mut State) -> &mut FlowState {
    &mut s.automations.diagram
}

fn ignore(_: &mut State, _: FlowEvent, _: &mut Tx) {}

fn diagram(s: &State, t: &'static Tokens, detail: &Detail, phone: bool) -> El {
    let a = &detail.automation;
    let marks = Marks {
        issues: &detail.issues,
        run: None,
        selection: s.automations.diagram.selection.as_ref(),
        dimmed: &|_| false,
        now_ms: now_ms(),
        offset_min: offset_min(),
    };
    let (cards, wires) = cards_and_wires(t, &a.graph, &a.trigger, &a.layout, &marks);
    let canvas = Canvas::new("automation-diagram", diagram_lens, ignore, look(t))
        .readable(if phone { 0.85 } else { 0.75 })
        .cards(cards)
        .wires(wires)
        .build(&s.automations.diagram);
    el().w_full()
        .h(DIAGRAM_H[usize::from(phone)])
        .radius(12.0)
        .border(1.0, t.colors.border)
        .clip_rounded(12.0)
        .child(canvas)
}

fn run_list(t: &Tokens, runs: Option<&Vec<Run>>) -> El {
    let c = &t.colors;
    let Some(runs) = runs else {
        return text("Loading runs…")
            .font(TEXT_XS)
            .color(c.muted_foreground);
    };
    if runs.is_empty() {
        return text("No runs yet. Use Run now to try it.")
            .font(TEXT_XS)
            .color(c.muted_foreground);
    }
    col()
        .key("automation-runs")
        .w_full()
        .radius(8.0)
        .border(1.0, c.border)
        .fill(c.card)
        .children(runs.iter().enumerate().map(|(i, run)| {
            let (label, tn) = run_presentation(run);
            let id = run.id.clone();
            let took = run
                .finished_at
                .map(|end| format_duration(end - run.started_at))
                .unwrap_or_default();
            row()
                .key(format!("automation-run-row-{}", run.id))
                .role("button")
                .label(format!(
                    "{label}, {}",
                    format_date_time(run.started_at, now_ms(), offset_min())
                ))
                .w_full()
                .min_h(44.0)
                .px(12.0)
                .py(8.0)
                .gap(12.0)
                .items_center()
                .border_sides([if i == 0 { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0], c.border)
                .hover(c.foreground.alpha(0.03))
                .child(
                    text(label)
                        .font(sans(12.0, 500.0, 16.0))
                        .color(tone(t, tn))
                        .w(80.0),
                )
                .child(
                    text(format_date_time(run.started_at, now_ms(), offset_min()))
                        .font(TEXT_XS)
                        .color(c.foreground)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(text(took).font(TEXT_XS).color(c.muted_foreground))
                .on_press(move |s: &mut State| s.automation_open_run(id.clone()))
        }))
}

/// The delete confirm (`AutomationDetail`'s `ConfirmDialog`).
pub fn dialogs(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    if !s.overlays.is_open(DELETE_CONFIRM) {
        return;
    }
    let name = s
        .automations
        .deleting
        .as_ref()
        .map_or("", |(_, name)| name.as_str());
    ConfirmDialog::new(
        DELETE_CONFIRM,
        "Delete automation",
        format!("Delete \"{name}\"? It stops running and its run history is removed. This cannot be undone."),
    )
    .variant(ConfirmVariant::Danger)
    .confirm_label("Delete")
    .open(true)
    .on_cancel(|s: &mut State| s.automation_cancel_delete())
    .on_confirm(|s: &mut State| s.automation_confirm_delete())
    .build(t, layer);
}
