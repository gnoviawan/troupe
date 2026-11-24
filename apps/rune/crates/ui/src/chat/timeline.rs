//! The transcript's rows (`components/chat/chat-timeline.ts`): messages and tool calls merged
//! in arrival order (`buildTimeline`), runs of thoughts joined (`consolidateThoughtGroups`),
//! then each user turn split into the user's message, one activity row for the agent's work,
//! and the replies that stay visible (`groupTurnActivity`).
//!
//! Rows point into the session's message and tool call lists by index, so a row list is only
//! good for the lists it was built from.

use model::acp::types::{ChatMessage, Object, Role};
use serde_json::Value;

use crate::markdown::strip_plan_fences;

/// One timeline entry inside a turn.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A message, by its index in the session's messages.
    Message(usize),
    /// A tool call, by its index in the session's tool calls.
    Tool(usize),
    /// Adjacent thoughts, by index, oldest first.
    Thoughts(Vec<usize>),
}

/// The disclosure that folds a turn's work: thoughts, tools and narration.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub items: Vec<Item>,
    /// The turn is still running.
    pub active: bool,
    pub duration_ms: Option<f64>,
    /// A tool call failed or never finished.
    pub attention: bool,
    /// A reply stays visible after the disclosure.
    pub has_final: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    User(usize),
    Activity(Activity),
    /// A visible agent reply; `tail` ends the turn and carries its copy action.
    Reply { index: usize, tail: bool },
}

/// One transcript row with the key that names it across commits.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: String,
    pub kind: RowKind,
}

/// The text of a message's text blocks, joined.
pub fn message_text(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect()
}

fn has_substantive_text(message: &ChatMessage) -> bool {
    message.blocks.iter().any(|b| {
        b.get("type").and_then(Value::as_str) == Some("text")
            && !strip_plan_fences(b.get("text").and_then(Value::as_str).unwrap_or("")).trim().is_empty()
    })
}

fn has_media(message: &ChatMessage) -> bool {
    message.blocks.iter().any(|b| b.get("type").and_then(Value::as_str) != Some("text"))
}

fn num(tool: &Object, field: &str) -> Option<f64> {
    tool.get(field).and_then(Value::as_f64)
}

fn str_field<'a>(tool: &'a Object, field: &str) -> Option<&'a str> {
    tool.get(field).and_then(Value::as_str)
}

/// A tool call's id.
pub fn tool_id(tool: &Object) -> &str {
    str_field(tool, "toolCallId").unwrap_or("")
}

fn is_subagent(tool: &Object) -> bool {
    tool.get("subagent").is_some_and(|v| !v.is_null())
}

enum Entry {
    Message(usize),
    Tool(usize),
}

/// `buildTimeline`: seqless history first by timestamp, then by seq; source order breaks ties.
fn build(messages: &[ChatMessage], tools: &[Object]) -> Vec<Entry> {
    struct Stamped {
        entry: Entry,
        seq: Option<f64>,
        ts: f64,
        order: usize,
    }
    let ids: std::collections::HashSet<&str> = tools.iter().map(tool_id).collect();
    let mut stamped: Vec<Stamped> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| Stamped { entry: Entry::Message(i), seq: m.seq, ts: m.timestamp, order: i })
        .collect();
    for (i, tool) in tools.iter().enumerate() {
        if tool.get("restoredSummary") == Some(&Value::Bool(true)) || model::acp::is_plan_tool(tool) {
            continue;
        }
        if str_field(tool, "parentToolCallId").is_some_and(|p| ids.contains(p)) {
            continue;
        }
        stamped.push(Stamped {
            entry: Entry::Tool(i),
            seq: num(tool, "seq"),
            ts: num(tool, "timestamp").unwrap_or(0.0),
            order: 1000 + i,
        });
    }
    stamped.sort_by(|a, b| match (a.seq, b.seq) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a.ts.total_cmp(&b.ts).then(a.order.cmp(&b.order)),
    });
    stamped.into_iter().map(|s| s.entry).collect()
}

/// The rows of a session: `active_turn` while the agent's turn runs.
pub fn rows(messages: &[ChatMessage], tools: &[Object], active_turn: bool) -> Vec<Row> {
    let mut items: Vec<Item> = Vec::new();
    for entry in build(messages, tools) {
        match entry {
            Entry::Message(i) if messages[i].role == Role::Thought => match items.last_mut() {
                Some(Item::Thoughts(run)) => run.push(i),
                _ => items.push(Item::Thoughts(vec![i])),
            },
            Entry::Message(i) => items.push(Item::Message(i)),
            Entry::Tool(i) => items.push(Item::Tool(i)),
        }
    }
    Grouper { messages, tools, out: Vec::new() }.group(items, active_turn)
}

struct Grouper<'a> {
    messages: &'a [ChatMessage],
    tools: &'a [Object],
    out: Vec<Row>,
}

impl Grouper<'_> {
    fn item_key(&self, item: &Item) -> String {
        match item {
            Item::Message(i) => self.messages[*i].id.clone(),
            Item::Tool(i) => tool_id(&self.tools[*i]).to_owned(),
            Item::Thoughts(run) => self.messages[run[0]].id.clone(),
        }
    }

    fn agent(&self, item: &Item) -> Option<&ChatMessage> {
        match item {
            Item::Message(i) if self.messages[*i].role == Role::Agent => Some(&self.messages[*i]),
            _ => None,
        }
    }

    fn timestamp(&self, item: &Item) -> Option<f64> {
        let ts = match item {
            Item::Message(i) => Some(self.messages[*i].timestamp),
            Item::Tool(i) => num(&self.tools[*i], "timestamp"),
            Item::Thoughts(run) => run.iter().map(|&i| self.messages[i].timestamp).reduce(f64::max),
        };
        ts.filter(|&t| t > 0.0)
    }

    fn needs_attention(&self, items: &[Item]) -> bool {
        items.iter().any(|item| match item {
            Item::Tool(i) => {
                let tool = &self.tools[*i];
                !is_subagent(tool) && matches!(str_field(tool, "status"), Some("failed" | "pending" | "in_progress"))
            }
            _ => false,
        })
    }

    fn group(mut self, items: Vec<Item>, active_turn: bool) -> Vec<Row> {
        let mut user: Option<usize> = None;
        let mut turn: Vec<Item> = Vec::new();
        let mut turn_index = 0;
        let last = items.len().checked_sub(1);
        for (n, item) in items.into_iter().enumerate() {
            match item {
                Item::Message(i) if self.messages[i].role == Role::User => {
                    if user.is_some() || !turn.is_empty() {
                        self.flush(&mut user, &mut turn, &mut turn_index, false);
                    }
                    user = Some(i);
                }
                other => turn.push(other),
            }
            if Some(n) == last {
                self.flush(&mut user, &mut turn, &mut turn_index, active_turn);
            }
        }
        if last.is_none() && active_turn {
            self.flush(&mut user, &mut turn, &mut turn_index, true);
        }
        self.out
    }

    fn flush(&mut self, user: &mut Option<usize>, turn: &mut Vec<Item>, turn_index: &mut usize, active: bool) {
        if user.is_none() && turn.is_empty() && !active {
            return;
        }
        let mut final_text = None;
        if !active {
            for (i, item) in turn.iter().enumerate().rev() {
                let Some(message) = self.agent(item) else { break };
                if has_substantive_text(message) {
                    final_text = Some(i);
                    break;
                }
            }
        }
        let visible: Vec<usize> = (0..turn.len())
            .filter(|&i| Some(i) == final_text || self.agent(&turn[i]).is_some_and(has_media))
            .collect();
        let count = turn.len();
        let activity: Vec<Item> = turn
            .iter()
            .enumerate()
            .filter(|(i, item)| {
                if visible.contains(i) {
                    return false;
                }
                // Only the trailing empty live tail stays (the caret lives there).
                !self.agent(item).is_some_and(|m| {
                    !has_substantive_text(m) && !has_media(m) && !(*i == count - 1 && m.streaming)
                })
            })
            .map(|(_, item)| item.clone())
            .collect();
        let user_index = user.take();
        if let Some(i) = user_index {
            self.out.push(Row { key: self.messages[i].id.clone(), kind: RowKind::User(i) });
        }
        if !activity.is_empty() || active {
            let started = user_index.map(|i| self.messages[i].timestamp).filter(|&t| t > 0.0);
            let ended = turn.iter().filter_map(|item| self.timestamp(item)).reduce(f64::max);
            let duration_ms = match (active, started, ended) {
                (false, Some(s), Some(e)) if e > s => Some(e - s),
                _ => None,
            };
            let key = match (user_index, turn.first()) {
                (Some(i), _) => format!("activity:{}", self.messages[i].id),
                (None, Some(first)) => format!("activity:{}", self.item_key(first)),
                (None, None) => format!("activity:{turn_index}"),
            };
            let attention = self.needs_attention(&activity);
            self.out.push(Row {
                key,
                kind: RowKind::Activity(Activity { items: activity, active, duration_ms, attention, has_final: !visible.is_empty() }),
            });
        }
        let tail = visible
            .iter()
            .rposition(|&i| self.agent(&turn[i]).is_some_and(has_substantive_text))
            .or(visible.len().checked_sub(1));
        for (n, &i) in visible.iter().enumerate() {
            let Item::Message(index) = turn[i] else { continue };
            self.out.push(Row { key: self.messages[index].id.clone(), kind: RowKind::Reply { index, tail: Some(n) == tail } });
        }
        turn.clear();
        *turn_index += 1;
    }
}
