//! One transcript row: the user's message, a turn's activity disclosure, or a visible reply.

use rune::{El, col, el, icon, row, text};
use theme::icons as i;

use std::rc::Rc;

use super::timeline::{Activity, Item, Row, RowKind, message_text, tool_id};
use super::{Ctx, placeholder, tool_call};
use crate::markdown;
use crate::type_::sans;
use crate::{BubbleAlign, BubbleVariant, IconActionButton, IconActionSize};

pub(super) fn build<S: 'static>(ctx: &Ctx<S>, s: &S, r: &Row, n: usize) -> El {
    let messages = ctx.messages(s);
    let last = n + 1 == ctx.rows.len();
    let body = match &r.kind {
        RowKind::User(i) => messages.get(*i).map(|m| user(ctx, &message_text(m), last)),
        RowKind::Reply { index, tail } => messages.get(*index).map(|m| {
            let streaming = m.streaming && last;
            reply(ctx, &m.id, &message_text(m), streaming, *tail && !m.streaming, last)
        }),
        RowKind::Activity(a) => Some(activity(ctx, s, &r.key, a)),
    };
    // Rows pad inside rather than gap: virtual rows measure each row's own box.
    col().w_full().child(body.unwrap_or_else(el))
}

/// `formatTurnDuration`.
pub fn duration(ms: f64) -> String {
    let total = ((ms / 1000.0).round() as u64).max(1);
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// `MessageActions`: Copy, then Edit (the user's) or Retry (a reply). Hover reveals the row
/// unless it is pinned (the last message).
fn actions<S: 'static>(ctx: &Ctx<S>, text: String, user: bool, pinned: bool) -> El {
    let t = ctx.t;
    let mut buttons = vec![
        IconActionButton::new(&i::COPY, "Copy")
            .size(IconActionSize::Sm)
            .build(t)
            .on_press_tx(markdown::copies::<S>(text)),
    ];
    buttons.push(if user {
        IconActionButton::new(&i::EDIT, "Edit").size(IconActionSize::Sm).build(t)
    } else {
        IconActionButton::new(&i::ROTATE_CCW, "Retry").size(IconActionSize::Sm).build(t)
    });
    let mut bar = row().gap(10.0).items_center().children(buttons);
    if user {
        bar = bar.justify_end().mr(-4.0);
    } else {
        bar = bar.ml(-4.0);
    }
    if pinned { bar } else { bar.group_opacity(0.0, 1.0) }
}

/// The user's turn: a tinted bubble at the end of the row, `max-w-[85%]`, with its actions.
fn user<S: 'static>(ctx: &Ctx<S>, words: &str, last: bool) -> El {
    let (shown, spans) = pills_inline(ctx.t, words);
    let bubble = crate::Bubble::new(shown)
        .spans(spans)
        .variant(BubbleVariant::Tinted)
        .align(BubbleAlign::End)
        .build(ctx.t);
    col()
        .w_full()
        .py(8.0)
        .items_end()
        .hover_group()
        .child(col().w_frac(0.85).items_end().gap(4.0).child(bubble).child(actions(ctx, crate::composer::wire::copy_text(words), true, last)))
}

/// A sent message's composer tokens as their labels on a chip (`ChatMessage`'s inline pills).
fn pills_inline(t: &theme::Tokens, display: &str) -> (String, Vec<rune::Span>) {
    if !display.contains(|c: char| ('\u{E000}'..='\u{E00F}').contains(&c)) {
        return (display.to_owned(), Vec::new());
    }
    let mut out = String::new();
    let mut spans = Vec::new();
    for part in crate::composer::parse_display(display).parts {
        match part {
            crate::composer::Part::Text(text) => out.push_str(&text),
            crate::composer::Part::Pill(pill) => {
                let start = out.len();
                out.push_str(&pill.label());
                let ink = if matches!(pill, crate::composer::Pill::Skill { .. }) { t.colors.primary } else { t.colors.foreground };
                spans.push(rune::Span::new(start, out.len()).bg(t.colors.muted, 3.0).color(ink).weight(500.0));
            }
        }
    }
    (out, spans)
}

/// A visible reply: its prose in a ghost bubble, then the turn's actions on its tail, shown
/// at rest only on the last row.
fn reply<S: 'static>(ctx: &Ctx<S>, id: &str, words: &str, streaming: bool, tail: bool, pinned: bool) -> El {
    let prose = markdown::prose(ctx.t, &format!("md-{id}"), words, streaming, &ctx.hooks);
    let mut content = col().w_full().min_w(0.0).gap(4.0).child(prose);
    if tail {
        content = content.child(actions(ctx, words.to_owned(), false, pinned));
    }
    col().w_full().py(8.0).hover_group().child(content)
}

/// `TurnActivity`: the chevron and "Worked for 12s" (or "Working…"), and when open, the
/// turn's steps: thoughts and tool calls as card placeholders, narration as prose.
fn activity<S: 'static>(ctx: &Ctx<S>, s: &S, key: &str, a: &Activity) -> El {
    let t = ctx.t;
    let c = &t.colors;
    let open = (ctx.lenses.reader)(s).is_open(key, a);
    let label = if a.active {
        "Working…".to_owned()
    } else {
        a.duration_ms.map_or_else(|| "Worked".to_owned(), |ms| format!("Worked for {}", duration(ms)))
    };
    let ink = if a.attention && !a.active { c.destructive } else { c.muted_foreground };
    let mut header = row()
        .key(format!("activity-toggle-{key}"))
        .role("button")
        .label(label.clone())
        .w_full()
        .min_h(28.0)
        .gap(6.0)
        .items_center()
        // A down chevron, not a rotated right one: the web client's CPU canvas stops presenting
        // frames while a rotated icon is on screen (Rune at the pin).
        .child(icon(if open { &i::CHEVRON_DOWN } else { &i::CHEVRON_RIGHT }, 13.0).color(ink))
        .child(text(label).font(sans(12.0, 500.0, 16.0)).color(ink));
    if a.attention && !a.active {
        header = header.child(text("· needs attention").font(sans(12.0, 400.0, 16.0)).color(ink));
    }
    let (reader, row_key, toggled) = (ctx.lenses.reader_mut, key.to_owned(), a.clone());
    let header = header.on_press(move |s: &mut S| reader(s).toggle(&row_key, &toggled));
    let mut frame = col().w_full().my(4.0).min_w(0.0).child(header);
    if open {
        frame = frame.child(col().w_full().min_w(0.0).pl(16.0).pb(4.0).gap(4.0).children(steps(ctx, s, a)));
    }
    frame
}

/// The turn's steps, with each run of subagent dispatches drawn as one `SubagentGroup`.
fn steps<S: 'static>(ctx: &Ctx<S>, s: &S, a: &Activity) -> Vec<El> {
    let tools = ctx.tools(s);
    let dispatch = |item: &Item| match item {
        Item::Tool(i) => tools.get(*i).filter(|t| t.get("subagent").is_some_and(|v| !v.is_null())),
        _ => None,
    };
    let mut out = Vec::new();
    let mut run = Vec::new();
    for item in &a.items {
        if let Some(tool) = dispatch(item) {
            run.push(tool);
            continue;
        }
        if !run.is_empty() {
            out.push(super::subagents::group(ctx.t, &ctx.session, &std::mem::take(&mut run), a.active, ctx.lenses));
        }
        out.push(step(ctx, s, item, a.active));
    }
    if !run.is_empty() {
        out.push(super::subagents::group(ctx.t, &ctx.session, &run, a.active, ctx.lenses));
    }
    out
}

fn step<S: 'static>(ctx: &Ctx<S>, s: &S, item: &Item, active: bool) -> El {
    let t = ctx.t;
    let (messages, tools) = (ctx.messages(s), ctx.tools(s));
    match item {
        Item::Thoughts(run) => {
            let words: String = run.iter().filter_map(|&i| messages.get(i)).map(message_text).collect();
            let first = words.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_owned();
            placeholder(t, "Thought", &first)
        }
        Item::Tool(i) => {
            let Some(tool) = tools.get(*i) else { return el() };
            tool_card(ctx, s, tool)
        }
        Item::Message(i) => {
            let Some(m) = messages.get(*i) else { return el() };
            markdown::prose(t, &format!("md-{}", m.id), &message_text(m), active && m.streaming, &ctx.hooks)
        }
    }
}

/// `ToolCallCard` for `tool`, with the reader's open result and full diffs.
fn tool_card<S: 'static>(ctx: &Ctx<S>, s: &S, tool: &model::acp::types::Object) -> El {
    let key = format!("tool-{}", tool_id(tool));
    let lenses = ctx.lenses;
    let reader = (lenses.reader)(s);
    let expanded = |n: usize| reader.is_diff_open(&format!("{key}-diff-{n}"));
    let (toggled, expand) = (key.clone(), key.clone());
    let actions = tool_call::Actions {
        toggle: Box::new(move |s: &mut S| (lenses.reader_mut)(s).toggle_tool(&toggled)),
        expand_diff: Rc::new(move |s: &mut S, n| (lenses.reader_mut)(s).open_diff(format!("{expand}-diff-{n}"))),
        open_file: Some(Rc::new(move |s: &mut S, path: String, line: Option<usize>| {
            (lenses.open_file)(s, line.map_or_else(|| path.clone(), |l| format!("{path}:{l}")))
        })),
    };
    tool_call::card(ctx.t, &key, tool, tool_call::Shown { open: reader.is_tool_open(&key), expanded: &expanded }, actions)
}
