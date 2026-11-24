//! The chat transcript (`components/chat/ChatMessageList.tsx` with `ChatMessage` and
//! `TurnActivity`): one virtual row per timeline row in the centred `max-w-3xl` column, inside
//! a scroll viewport that follows the end while it is at the end and holds the reader's place
//! when scrolled up (`ScrollSpec::stick_bottom`; virtual rows keep their first visible row),
//! with `MessageScrollerButton`'s "Scroll to latest" over the bottom while it is scrolled up.
//!
//! Tool calls draw as `ToolCallCard`s and subagent dispatches as their group; thoughts still
//! draw a placeholder row.

pub mod approvals;
pub mod diff;
pub mod elicitation;
pub mod notice;
pub mod plan;
mod rows;
pub mod subagents;
pub mod timeline;
pub mod tool_call;

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod elicitation_tests;
#[cfg(test)]
mod notice_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod timeline_tests;
#[cfg(test)]
mod tool_call_tests;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use model::acp;
use rune::{El, GrowthSpec, ScrollSpec, col, el, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use crate::markdown::Hooks;
use crate::paint::{PILL, shadows};
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

/// What the reader set by hand: the open state of turns' activity rows, and the transcripts
/// scrolled up from their end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reader {
    folds: BTreeMap<String, bool>,
    away: BTreeSet<String>,
    /// The options picked in the open questions.
    pub picks: approvals::Picks,
    /// Per session, the subagent dispatch whose panel is open.
    subagent_panels: BTreeMap<String, String>,
    /// Subagents a stop was sent for, until their record leaves running.
    stopping: BTreeSet<String>,
    /// Tool call cards whose result the reader opened, by card key.
    tools_open: BTreeSet<String>,
    /// Edit diffs shown in full, as `{card key}-diff-{n}`.
    diffs_open: BTreeSet<String>,
    /// Per session, the plan bar the reader folded or opened, and whether the plan was
    /// finished then.
    plan_folds: BTreeMap<String, (bool, bool)>,
    /// Plan entries showing their detail.
    plan_entries: BTreeSet<(String, usize)>,
    /// Per session, the error the reader dismissed (`dismissedError`).
    pub dismissed_error: BTreeMap<String, String>,
    /// Sessions whose error notice shows its details.
    pub error_details: BTreeSet<String>,
    /// What the reader entered in each open elicitation form, by request.
    forms: BTreeMap<String, elicitation::Form>,
}

impl Reader {
    /// `useState(active || (!attentionRequired && !hasFinalResponse))`, then the reader's pick.
    pub fn is_open(&self, key: &str, activity: &Activity) -> bool {
        if activity.active {
            return true;
        }
        self.folds
            .get(key)
            .copied()
            .unwrap_or(!activity.attention && !activity.has_final)
    }

    pub fn toggle(&mut self, key: &str, activity: &Activity) {
        let open = self.is_open(key, activity);
        self.folds.insert(key.to_owned(), !open);
    }

    /// The session's transcript is scrolled up from its end.
    pub fn is_away(&self, session_id: &str) -> bool {
        self.away.contains(session_id)
    }

    /// Opens the panel of the dispatch `tool_call_id` in `session_id`.
    pub fn open_subagent(&mut self, session_id: &str, tool_call_id: &str) {
        self.subagent_panels.insert(session_id.to_owned(), tool_call_id.to_owned());
    }

    pub fn close_subagent(&mut self, session_id: &str) {
        self.subagent_panels.remove(session_id);
    }

    /// The dispatch whose panel is open in `session_id`.
    pub fn subagent_panel(&self, session_id: &str) -> Option<&str> {
        self.subagent_panels.get(session_id).map(String::as_str)
    }

    /// A stop was sent for `subagent_id` (`setStopping(true)`), or it settled.
    pub fn set_stopping(&mut self, subagent_id: &str, on: bool) {
        if on {
            self.stopping.insert(subagent_id.to_owned());
        } else {
            self.stopping.remove(subagent_id);
        }
    }

    pub fn is_stopping(&self, subagent_id: &str) -> bool {
        self.stopping.contains(subagent_id)
    }

    pub fn is_tool_open(&self, key: &str) -> bool {
        self.tools_open.contains(key)
    }

    pub fn toggle_tool(&mut self, key: &str) {
        if !self.tools_open.remove(key) {
            self.tools_open.insert(key.to_owned());
        }
    }

    pub fn is_diff_open(&self, key: &str) -> bool {
        self.diffs_open.contains(key)
    }

    pub fn open_diff(&mut self, key: String) {
        self.diffs_open.insert(key);
    }

    /// Whether `session`'s plan shows only its bar: the reader's pick, closed again once the plan
    /// finishes; else folded on a phone or when finished.
    pub fn plan_collapsed(&self, session: &str, settled: bool, phone: bool) -> bool {
        match self.plan_folds.get(session) {
            Some(&(collapsed, was_settled)) => collapsed || (settled && !was_settled),
            None => phone || settled,
        }
    }

    pub fn toggle_plan(&mut self, session: &str, collapsed: bool, settled: bool) {
        self.plan_folds.insert(session.to_owned(), (!collapsed, settled));
    }

    pub fn is_plan_entry_open(&self, session: &str, n: usize) -> bool {
        self.plan_entries.contains(&(session.to_owned(), n))
    }

    pub fn toggle_plan_entry(&mut self, session: &str, n: usize) {
        let key = (session.to_owned(), n);
        if !self.plan_entries.remove(&key) {
            self.plan_entries.insert(key);
        }
    }

    /// The form of the elicitation `request_id`.
    pub fn form(&mut self, request_id: &str) -> &mut elicitation::Form {
        self.forms.entry(request_id.to_owned()).or_default()
    }

    pub fn form_of(&self, request_id: &str) -> Option<&elicitation::Form> {
        self.forms.get(request_id)
    }

    /// Records whether the transcript shows its end; true when that changed.
    pub fn set_at_end(&mut self, session_id: &str, at_end: bool) -> bool {
        if at_end { self.away.remove(session_id) } else { self.away.insert(session_id.to_owned()) }
    }
}

/// How the transcript reaches the app's state.
pub struct Lenses<S> {
    pub acp: fn(&S) -> &acp::State,
    pub reader: fn(&S) -> &Reader,
    pub reader_mut: fn(&mut S) -> &mut Reader,
    /// Applies one model action and runs its effects; the cards answer through it.
    pub step: fn(&mut S, acp::Event),
    /// Asks the host to stop a subagent (`stop_subagent`): the session and the record's id.
    pub stop_subagent: fn(&mut S, String, String),
    /// Opens a file a card names, as `path` or `path:line`.
    pub open_file: fn(&mut S, String),
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
    let away = (lenses.reader)(state).is_away(session_id);
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
    let session = session_id.to_owned();
    let frame = el()
        .key(format!("chat-transcript-{session_id}"))
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
                .on_at_bottom_if(move |s: &mut S, at_end| (lenses.reader_mut)(s).set_at_end(&session, at_end))
                .child(col().w_full().pt(LIST_PADDING_Y).pb(LIST_PADDING_Y).child(list)),
        )
        .child(fade(true))
        .child(fade(false));
    if away { frame.child(scroll_to_latest::<S>(t, session_id)) } else { frame }
}

/// `MessageScrollerButton`: a round arrow centred over the transcript's bottom that scrolls
/// to the end.
fn scroll_to_latest<S: 'static>(t: &'static Tokens, session_id: &str) -> El {
    let c = &t.colors;
    let target = scroll_key(session_id);
    let button = row()
        .key(format!("chat-latest-{session_id}"))
        .role("button")
        .label("Scroll to latest")
        .size(40.0, 40.0)
        .center()
        .radius(PILL)
        .border(1.0, c.border)
        .fill(c.background)
        .hover(c.muted)
        .focusable(true)
        .child(icon(&i::ARROW_DOWN, 16.0).color(c.foreground))
        .on_press_tx(move |_s: &mut S, tx: &mut rune::Tx| tx.scroll_to_bottom(target.clone()));
    row()
        .w_full()
        .justify_center()
        .pin(Some(0.0), None, Some(0.0), Some(16.0))
        .child(shadows(button, &t.shadows.shadow_control))
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
