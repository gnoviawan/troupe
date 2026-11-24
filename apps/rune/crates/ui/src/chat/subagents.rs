//! Subagents in the transcript (`SubagentGroup`, `SubagentRow`) and the panel a row opens
//! (`SubagentPanel`, docked on the right of a desktop chat), with its Stop
//! (`useStopSubagent`). The host's record rides on the dispatch tool call as `subagent`.

use rune::{El, col, el, icon, row, text};
use serde_json::Value;
use theme::Tokens;
use theme::icons as i;

use super::Lenses;
use crate::type_::sans;
use crate::{Button, ButtonSize, ButtonVariant, IconActionButton, IconActionSize};

/// `SubagentStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Done,
    Failed,
    Stopped,
}

impl Status {
    /// `SUBAGENT_STATUS_WORD`.
    pub fn word(self) -> &'static str {
        match self {
            Status::Running => "Working",
            Status::Done => "Done",
            Status::Failed => "Failed",
            Status::Stopped => "Stopped",
        }
    }

    fn count_word(self) -> &'static str {
        match self {
            Status::Running => "working",
            Status::Done => "done",
            Status::Failed => "failed",
            Status::Stopped => "stopped",
        }
    }
}

fn record(tool: &model::acp::types::Object) -> Option<&Value> {
    tool.get("subagent").filter(|v| !v.is_null())
}

fn record_str<'a>(tool: &'a model::acp::types::Object, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(record(tool)?, |v, k| v.get(*k))?.as_str()
}

/// `isSubagentRunning` then `subagentStatus`, for a dispatch whose turn is still running
/// (`turn_active`) or has ended.
pub fn status(tool: &model::acp::types::Object, turn_active: bool) -> Status {
    let Some(rec) = record(tool) else { return Status::Stopped };
    let activity = rec.get("activity").and_then(Value::as_str).unwrap_or("unknown");
    let live = rec.pointer("/caps/liveProgress").and_then(Value::as_bool).unwrap_or(false);
    let background = rec.get("background").and_then(Value::as_bool).unwrap_or(false);
    let completed = tool.get("status").and_then(Value::as_str) == Some("completed");
    if matches!(activity, "running" | "requires_action") && (live || turn_active || (background && completed)) {
        return Status::Running;
    }
    match activity {
        "idle" => match rec.pointer("/end/reason").and_then(Value::as_str) {
            Some("completed") => Status::Done,
            Some("failed") => Status::Failed,
            _ => Status::Stopped,
        },
        "running" | "requires_action" if background => Status::Done,
        _ => Status::Stopped,
    }
}

/// `subagentTitle`.
pub fn title(tool: &model::acp::types::Object) -> String {
    record_str(tool, &["title"])
        .or_else(|| tool.get("title").and_then(Value::as_str))
        .unwrap_or("Subagent")
        .to_owned()
}

/// The record's id, which `stop_subagent` names.
pub fn subagent_id(tool: &model::acp::types::Object) -> Option<String> {
    record_str(tool, &["id"]).map(str::to_owned)
}

/// The agent offers Troupe a stop for this subagent (`caps.stop === 'session-cancel'`).
pub fn can_stop(tool: &model::acp::types::Object, status: Status) -> bool {
    status == Status::Running && record_str(tool, &["caps", "stop"]) == Some("session-cancel")
}

fn tool_id(tool: &model::acp::types::Object) -> String {
    tool.get("toolCallId").and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn waiting(tool: &model::acp::types::Object, status: Status) -> bool {
    status == Status::Running && record_str(tool, &["activity"]) == Some("requires_action")
}

fn tone(t: &Tokens, status: Status, waiting: bool) -> rune::Color {
    let c = &t.colors;
    if waiting {
        return c.warning;
    }
    match status {
        Status::Done => c.success,
        Status::Failed => c.destructive,
        Status::Running | Status::Stopped => c.muted_foreground,
    }
}

/// `SubagentStatusIcon`: a still loader while it runs (a turning glyph freezes the web
/// client's CPU canvas at the pin), a check when done, a cross otherwise.
fn status_icon(t: &Tokens, status: Status, waiting: bool, size: f32) -> El {
    let glyph: &'static rune::Icon = match status {
        Status::Running => &i::LOADER_CIRCLE,
        Status::Done => &i::CHECK,
        Status::Failed | Status::Stopped => &i::X,
    };
    icon(glyph, size).color(tone(t, status, waiting))
}

/// `SubagentStatusLabel`: the icon and "Working", "Needs input", "Done", "Failed" or
/// "Stopped".
fn status_label(t: &Tokens, status: Status, waiting: bool) -> El {
    let word = if waiting { "Needs input" } else { status.word() };
    row()
        .gap(4.0)
        .items_center()
        .child(status_icon(t, status, waiting, 14.0))
        .child(text(word).font(sans(12.0, 400.0, 16.0)).color(tone(t, status, waiting)))
}

/// `SubagentAvatar`: the agent's glyph in a 32-point (28 small) circle with the status mark.
fn avatar(t: &Tokens, status: Status, waiting: bool, small: bool) -> El {
    let c = &t.colors;
    let size = if small { 28.0 } else { 32.0 };
    let mark = if status == Status::Running && !waiting {
        row().size(10.0, 10.0).center().radius(5.0).fill(c.card).child(icon(&i::LOADER_CIRCLE, 10.0).color(c.muted_foreground))
    } else {
        el().size(10.0, 10.0).radius(5.0).fill(tone(t, status, waiting)).border(2.0, c.card)
    };
    row()
        .size(size, size)
        .center()
        .radius(size / 2.0)
        .border(1.0, c.border)
        .fill(c.muted)
        .child(icon(&i::BOT, 16.0).color(c.foreground))
        .child(mark.pin(None, Some(-2.0), Some(-2.0), None))
        .a11y_hidden()
}

/// `SubagentRow`: avatar, title, status, and the chevron that opens the panel.
fn subagent_row<S: 'static>(t: &'static Tokens, session: &str, tool: &model::acp::types::Object, turn_active: bool, lenses: Lenses<S>) -> El {
    let c = &t.colors;
    let st = status(tool, turn_active);
    let wait = waiting(tool, st);
    let name = title(tool);
    let (session, id) = (session.to_owned(), tool_id(tool));
    row()
        .key(format!("subagent-row-{id}"))
        .role("button")
        .label(name.clone())
        .w_full()
        .min_h(48.0)
        .gap(12.0)
        .px(12.0)
        .py(8.0)
        .items_center()
        .radius(8.0)
        .hover(c.foreground.alpha(0.03))
        .focusable(true)
        .child(avatar(t, st, wait, false))
        .child(text(name).font(sans(14.0, 500.0, 20.0)).color(c.foreground).single_line().truncate().min_w(0.0).grow(1.0))
        .child(status_label(t, st, wait))
        .child(icon(&i::CHEVRON_RIGHT, 14.0).color(c.muted_foreground))
        .on_press(move |s: &mut S| (lenses.reader_mut)(s).open_subagent(&session, &id))
}

/// `SubagentGroup`: one card per launch batch; several get the header with their avatars and
/// counts.
pub fn group<S: 'static>(t: &'static Tokens, session: &str, tools: &[&model::acp::types::Object], turn_active: bool, lenses: Lenses<S>) -> El {
    let c = &t.colors;
    let rows: Vec<El> = tools.iter().map(|tool| subagent_row(t, session, tool, turn_active, lenses)).collect();
    let card = col().w_full().gap(2.0).padding(4.0).radius(12.0).border(1.0, c.border).fill(c.card).children(rows);
    if tools.len() < 2 {
        return col().w_full().my(6.0).child(card);
    }
    let statuses: Vec<Status> = tools.iter().map(|tool| status(tool, turn_active)).collect();
    let counts = [Status::Running, Status::Done, Status::Failed, Status::Stopped]
        .into_iter()
        .filter_map(|s| {
            let n = statuses.iter().filter(|x| **x == s).count();
            (n > 0).then(|| format!("{n} {}", s.count_word()))
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let avatars = row().items_center().children(
        tools.iter().zip(&statuses).take(3).enumerate().map(|(n, (tool, st))| {
            let a = avatar(t, *st, waiting(tool, *st), true);
            if n > 0 { a.ml(-8.0) } else { a }
        }),
    );
    let header = row()
        .w_full()
        .min_h(44.0)
        .gap(12.0)
        .px(10.0)
        .py(6.0)
        .items_center()
        .child(avatars)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(format!("{} subagents", tools.len())).font(sans(14.0, 500.0, 20.0)).color(c.foreground))
                .child(text(counts).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)),
        )
        .child(icon(&i::CHEVRON_DOWN, 14.0).color(c.muted_foreground));
    col().w_full().my(6.0).child(header).child(card.mt(4.0))
}

/// What the open panel shows: the dispatch, its place among its siblings, and whether a stop
/// is on its way.
pub struct PanelSpec<'a> {
    pub session: &'a str,
    pub tool: &'a model::acp::types::Object,
    pub index: usize,
    pub total: usize,
    pub turn_active: bool,
    pub stopping: bool,
    pub agent_name: &'a str,
}

/// `SubagentPanel`: Back, "Subagent of this chat · i of n" and Close; the avatar, title and
/// status with Stop; the task and the work; "You can't message it." and Back to chat.
pub fn panel<S: 'static>(t: &'static Tokens, p: PanelSpec, lenses: Lenses<S>) -> El {
    let c = &t.colors;
    let st = status(p.tool, p.turn_active);
    let wait = waiting(p.tool, st);
    let close = |label: &'static str| {
        let session = p.session.to_owned();
        move |s: &mut S| {
            let _ = label;
            (lenses.reader_mut)(s).close_subagent(&session);
        }
    };
    let mut place = "Subagent of this chat".to_owned();
    if p.total > 1 {
        place.push_str(&format!(" · {} of {}", p.index + 1, p.total));
    }
    let top = row()
        .w_full()
        .gap(4.0)
        .px(6.0)
        .pt(6.0)
        .items_center()
        .child(IconActionButton::new(&i::ARROW_LEFT, "Back").size(IconActionSize::Sm).build(t).on_press(close("back")))
        .child(text(place).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0).grow(1.0))
        .child(IconActionButton::new(&i::X, "Close").size(IconActionSize::Sm).build(t).on_press(close("close")));
    let mut actions = row().gap(8.0).items_center().child(status_label(t, st, wait));
    if can_stop(p.tool, st) {
        let (session, id) = (p.session.to_owned(), subagent_id(p.tool).unwrap_or_default());
        let label = if p.stopping { "Stopping" } else { "Stop" };
        actions = actions.child(el().grow(1.0)).child(
            Button::new(label)
                .icon(&i::SQUARE)
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .disabled(p.stopping)
                .build(t)
                .key(format!("subagent-stop-{}", tool_id(p.tool)))
                .on_press(move |s: &mut S| (lenses.stop_subagent)(s, session.clone(), id.clone())),
        );
    }
    let head = row()
        .w_full()
        .gap(12.0)
        .px(16.0)
        .pb(12.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(avatar(t, st, wait, false))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .child(text(title(p.tool)).font(sans(14.0, 600.0, 20.0)).color(c.foreground).wrap())
                .child(actions),
        );
    let prompt = record_str(p.tool, &["prompt"]).filter(|s| !s.trim().is_empty());
    let task = match prompt {
        Some(prompt) => crate::Bubble::new(prompt.to_owned()).variant(crate::BubbleVariant::Tinted).align(crate::BubbleAlign::End).build(t),
        None => text(format!("{} did not share the task it gave this subagent.", p.agent_name))
            .font(sans(12.0, 400.0, 16.0))
            .color(c.muted_foreground)
            .wrap(),
    };
    let empty = p.tool.get("content").and_then(Value::as_array).is_none_or(|c| c.is_empty());
    let mut work = col().w_full().min_w(0.0).child(task);
    if empty {
        // The work section (`Steps`) before the subagent wrote anything.
        work = work.child(text("Nothing to show yet.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    }
    let body = col().w_full().grow(1.0).min_h(0.0).gap(20.0).px(16.0).py(16.0).child(work);
    let foot = row()
        .w_full()
        .gap(8.0)
        .px(16.0)
        .py(8.0)
        .items_center()
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .child(icon(&i::CLOCK, 14.0).color(c.muted_foreground))
        .child(text("You can't message it.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).grow(1.0))
        .child(
            Button::new("Back to chat")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .build(t)
                .on_press(close("back to chat")),
        );
    col()
        .key(format!("subagent-panel-{}", tool_id(p.tool)))
        .role("complementary")
        .label("Subagent")
        .h_full()
        .min_h(0.0)
        .fill(c.card)
        .child(top)
        .child(head)
        .child(body)
        .child(foot)
}
