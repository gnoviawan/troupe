//! `PlanPanel`: the agent's plan docked over the transcript. A bar ("Plan", done of total, the
//! chevron) that folds the list of entries, each with its status mark, priority badge and text;
//! an entry with a detail opens to it. A finished plan starts folded, as does the phone's.

use model::acp::types::PlanEntry;
use rune::{El, col, el, icon, row, text};
use serde_json::Value;
use theme::Tokens;
use theme::icons as i;

use super::Lenses;
use crate::type_::{MICRO, sans};

/// `isSettledStatus`.
fn settled(status: Option<&str>) -> bool {
    matches!(status, Some("completed" | "cancelled"))
}

/// `isSettledPlan`.
pub fn is_settled(entries: &[PlanEntry]) -> bool {
    !entries.is_empty() && entries.iter().all(|e| settled(e.text("status")))
}

/// `getPlanDetail`: `detail`, else `_meta.detail`, when not blank.
fn detail(entry: &PlanEntry) -> Option<&str> {
    let blank = |s: &&str| !s.trim().is_empty();
    entry.text("detail").filter(blank).or_else(|| entry.get("_meta").and_then(|m| m.get("detail")).and_then(Value::as_str).filter(blank))
}

fn status_icon(t: &Tokens, status: Option<&str>) -> El {
    let c = &t.colors;
    match status {
        Some("completed") => icon(&i::CHECK_CIRCLE2, 13.0).color(c.success),
        // A still loader: a turning glyph stops the web client's CPU canvas at the pin.
        Some("in_progress") => icon(&i::LOADER_CIRCLE, 13.0).color(c.muted_foreground),
        _ => icon(&i::CIRCLE, 13.0).color(c.muted_foreground),
    }
    .a11y_hidden()
}

fn priority_badge(t: &Tokens, priority: Option<&str>) -> El {
    let c = &t.colors;
    let (label, fill, ink) = match priority {
        Some("high") => ("High", c.destructive.alpha(0.15), c.destructive),
        Some("medium") => ("Medium", c.warning.alpha(0.15), c.warning),
        _ => ("Low", c.secondary, c.muted_foreground),
    };
    row().px(4.0).py(1.0).radius(4.0).fill(fill).child(text(label).font(sans(MICRO, 500.0, 18.0)).color(ink))
}

fn entry_label(t: &Tokens, e: &PlanEntry) -> Vec<El> {
    let c = &t.colors;
    let status = e.text("status");
    let done = status == Some("completed");
    let mut words = text(e.content().to_owned()).font(sans(12.0, 400.0, 16.0)).color(if done { c.muted_foreground } else { c.foreground }).wrap().min_w(0.0).grow(1.0);
    if done {
        words = words.strike(0, e.content().len(), Some(c.muted_foreground));
    }
    vec![status_icon(t, status), priority_badge(t, e.text("priority")), words]
}

/// The panel for `session`'s `entries`, folded when `collapsed`; `opened` says which entries
/// show their detail.
pub fn panel<S: 'static>(t: &'static Tokens, session: &str, entries: &[PlanEntry], collapsed: bool, opened: &dyn Fn(usize) -> bool, phone: bool, lenses: Lenses<S>) -> El {
    let c = &t.colors;
    let total = entries.len();
    let done = entries.iter().filter(|e| e.text("status") == Some("completed")).count();
    let running = entries.iter().filter(|e| e.text("status") == Some("in_progress")).count();
    let mut label = format!("Plan, {done} of {total} {}", if total == 1 { "task" } else { "tasks" });
    if running > 0 {
        label.push_str(&if running == 1 { ", task in progress".to_owned() } else { format!(", {running} tasks in progress") });
    }
    let ink = c.muted_foreground;
    let bold = sans(MICRO, 600.0, 18.0);
    let mut bar = row()
        .key(format!("plan-toggle-{session}"))
        .role("button")
        .label(label)
        .w_full()
        .min_h(if phone { 44.0 } else { 40.0 })
        .gap(6.0)
        .px(12.0)
        .py(8.0)
        .items_center()
        .hover(c.muted.alpha(0.3))
        .focusable(true)
        .child(icon(&i::LIST_CHECKS, 12.0).color(ink))
        .child(text("Plan").font(bold).color(ink));
    if running > 0 {
        bar = bar.child(icon(&i::LOADER_CIRCLE, 12.0).color(ink).ml(4.0));
    }
    let (toggled, finished) = (session.to_owned(), is_settled(entries));
    bar = bar
        .child(el().grow(1.0))
        .child(
            row()
                .child(text(done.to_string()).font(bold).color(ink))
                .child(text("/").font(bold).color(ink.alpha(0.4)))
                .child(text(total.to_string()).font(bold).color(ink)),
        )
        .child(icon(if collapsed { &i::CHEVRON_DOWN } else { &i::CHEVRON_UP }, 14.0).color(ink).a11y_hidden())
        .on_press(move |s: &mut S| (lenses.reader_mut)(s).toggle_plan(&toggled, collapsed, finished));
    let mut card = col()
        .key(format!("plan-{session}"))
        .role("region")
        .label("Execution plan")
        .w_full()
        .max_w(super::COLUMN_MAX_W)
        .radius(8.0)
        .clip_rounded(8.0)
        .border(1.0, c.border.alpha(0.5))
        .fill(c.card.alpha(0.3))
        .child(bar);
    if !collapsed {
        let rows = entries.iter().enumerate().map(|(n, e)| {
            let mut r = row().w_full().min_h(28.0).gap(6.0).px(6.0).py(2.0).items_center().radius(6.0).children(entry_label(t, e));
            let Some(more) = detail(e) else { return r };
            let open = opened(n);
            let session = session.to_owned();
            r = r
                .key(format!("plan-entry-{n}"))
                .role("button")
                .focusable(true)
                .child(icon(if open { &i::CHEVRON_UP } else { &i::CHEVRON_DOWN }, 14.0).color(c.muted_foreground).a11y_hidden())
                .on_press(move |s: &mut S| (lenses.reader_mut)(s).toggle_plan_entry(&session, n));
            let mut item = col().w_full().child(r);
            if open {
                item = item.child(text(more.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap().pl(32.0).pr(8.0).pb(4.0));
            }
            item
        });
        card = card.child(
            col()
                .w_full()
                .max_h(240.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], c.border.alpha(0.4))
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(col().w_full().px(8.0).pb(6.0).pt(2.0).children(rows.collect::<Vec<_>>())),
        );
    }
    row().key(format!("plan-dock-{session}")).w_full().justify_center().px(if phone { 12.0 } else { 20.0 }).py(8.0).child(card)
}
