//! The chat transcript (`components/chat/ChatMessageList.tsx` with `ChatMessage` and
//! `TurnActivity`): one virtual row per timeline row in the centred `max-w-3xl` column, inside
//! a scroll viewport that follows the end while it is at the end and holds the reader's place
//! when scrolled up (`ScrollSpec::stick_bottom`; virtual rows keep their first visible row).
//!
//! The inline cards (tool calls, approvals, the plan, subagents) are later units; each draws a
//! placeholder row here so the transcript shows where it goes.

mod rows;
pub mod timeline;

#[cfg(test)]
mod timeline_tests;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::rc::Rc;

use model::acp;
use rune::{El, GrowthSpec, ScrollSpec, col, el, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use crate::markdown::Hooks;
use crate::type_::sans;
pub use timeline::{Activity, Item, Row, RowKind, message_text, rows};

/// `LIST_PADDING_Y_PX`: room above the first row and below the last.
pub const LIST_PADDING_Y: f32 = 24.0;
/// `max-w-3xl`.
pub const COLUMN_MAX_W: f32 = 768.0;
/// The scroll viewport's key; a chat's is `chat-scroll-{session}`.
pub fn scroll_key(session_id: &str) -> String {
    format!("chat-scroll-{session_id}")
}

/// The open state of each turn's activity row that the reader set by hand.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Folds(BTreeMap<String, bool>);

impl Folds {
    /// `useState(active || (!attentionRequired && !hasFinalResponse))`, then the reader's pick.
    pub fn is_open(&self, key: &str, activity: &Activity) -> bool {
        if activity.active {
            return true;
        }
        self.0
            .get(key)
            .copied()
            .unwrap_or(!activity.attention && !activity.has_final)
    }

    pub fn toggle(&mut self, key: &str, activity: &Activity) {
        let open = self.is_open(key, activity);
        self.0.insert(key.to_owned(), !open);
    }
}

/// How the transcript reaches the app's state.
pub struct Lenses<S> {
    pub acp: fn(&S) -> &acp::State,
    pub folds: fn(&S) -> &Folds,
    pub folds_mut: fn(&mut S) -> &mut Folds,
}

impl<S> Clone for Lenses<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for Lenses<S> {}

/// What a transcript draws from: a session, its rows, and how to reach the state.
pub(crate) struct Ctx<S> {
    pub t: &'static Tokens,
    pub session: String,
    pub rows: Vec<Row>,
    pub lenses: Lenses<S>,
    pub hooks: Hooks<S>,
}

impl<S> Ctx<S> {
    pub fn messages<'s>(&self, s: &'s S) -> &'s [acp::types::ChatMessage] {
        (self.lenses.acp)(s).messages.get(&self.session).map_or(&[], Vec::as_slice)
    }

    pub fn tools<'s>(&self, s: &'s S) -> &'s [acp::types::Object] {
        (self.lenses.acp)(s).tool_calls.get(&self.session).map_or(&[], Vec::as_slice)
    }
}

/// Whether the session has a turn open (`activeTurn`).
pub fn turn_active(state: &acp::State, session_id: &str) -> bool {
    state.sessions.get(session_id).is_some_and(|s| s.active_turn)
}

/// The transcript of `session_id` as `state` holds it.
pub fn transcript<S: 'static>(
    t: &'static Tokens,
    state: &S,
    session_id: &str,
    lenses: Lenses<S>,
    hooks: Hooks<S>,
) -> El {
    let acp_state = (lenses.acp)(state);
    let messages = acp_state.messages.get(session_id).map_or(&[][..], Vec::as_slice);
    let tools = acp_state.tool_calls.get(session_id).map_or(&[][..], Vec::as_slice);
    let active = turn_active(acp_state, session_id);
    let rows = rows(messages, tools, active);
    if rows.is_empty() {
        return empty(t, session_id);
    }
    let count = rows.len();
    let ctx = Rc::new(Ctx { t, session: session_id.to_owned(), rows, lenses, hooks });
    let (keys, build) = (ctx.clone(), ctx.clone());
    let list = col()
        .key(format!("chat-rows-{session_id}"))
        .w_full()
        .max_w(COLUMN_MAX_W)
        .self_center()
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            120.0,
            move |_s: &S, n| Cow::Owned(keys.rows.get(n).map_or_else(String::new, |r| r.key.clone())),
            move |s: &S, n| match build.rows.get(n) {
                Some(row) => rows::build(&build, s, row, n),
                None => el(),
            },
        );
    let fade = |top: bool| {
        let e = el()
            .w_full()
            .h(LIST_PADDING_Y)
            .linear_gradient(if top { 180.0 } else { 0.0 }, [(0.0, t.colors.terminal_bg), (1.0, t.colors.terminal_bg.alpha(0.0))])
            .a11y_hidden();
        if top { e.pin(Some(0.0), Some(0.0), Some(0.0), None) } else { e.pin(Some(0.0), None, Some(0.0), Some(0.0)) }
    };
    el().key(format!("chat-transcript-{session_id}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            col()
                .key(scroll_key(session_id))
                .role("log")
                .label("Messages")
                .w_full()
                .h_full()
                .min_h(0.0)
                .px(20.0)
                .scroll(ScrollSpec { stick_bottom: true })
                .child(col().w_full().pt(LIST_PADDING_Y).pb(LIST_PADDING_Y).child(list)),
        )
        .child(fade(true))
        .child(fade(false))
}

/// `ChatEmptyState`, reduced: a chat with no messages yet.
fn empty(t: &'static Tokens, session_id: &str) -> El {
    col()
        .key(format!("chat-empty-{session_id}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(10.0)
        .child(icon(&i::BOT, 24.0).color(t.colors.muted_foreground))
        .child(text("No messages yet").font(sans(13.0, 400.0, 20.0)).color(t.colors.muted_foreground))
}

/// A row standing in for an inline card a later unit draws: its kind and a one-line summary.
pub fn placeholder(t: &Tokens, kind: &str, summary: &str) -> El {
    let c = &t.colors;
    row()
        .w_full()
        .min_h(28.0)
        .gap(8.0)
        .px(8.0)
        .py(4.0)
        .items_center()
        .radius(6.0)
        .border(1.0, c.border.alpha(0.6))
        .child(text(kind.to_owned()).font(sans(11.0, 500.0, 16.0)).color(c.muted_foreground))
        .child(
            text(summary.to_owned())
                .font(sans(12.0, 400.0, 16.0))
                .color(c.secondary_foreground)
                .single_line()
                .truncate()
                .min_w(0.0)
                .grow(1.0),
        )
}
