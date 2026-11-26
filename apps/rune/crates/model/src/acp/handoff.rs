//! The handoff summary an agent switch sends the new agent
//! (`components/chat/handoff-summary.ts`: `buildHandoffSummary` and
//! `sanitizeHandoffWireBlocks`), with the tool one-liner it reads
//! (`components/chat/tool-call-summary.ts` `describeToolCall`) and
//! `stripTermulPlanFences` (`lib/plan-session.ts`).
//!
//! The composer's tokens (skills, commands, files, conversations, tools) and
//! its selection-context chips are rendered to text by `sanitizeDisplayText`
//! and `serializeSelectionContexts`, which belong to the composer unit. Here
//! only the sweep of their sentinels runs, so a draft or a transcript holding
//! tokens keeps their names' raw text.

use serde_json::{Value, json};

use super::js;
use super::types::{ChatMessage, Object, Role};

const MAX_TURNS: usize = 12;
const MAX_BODY_CHARS: usize = 4000;
const MAX_FRAGMENT_CHARS: usize = 200;
const GENERIC_AGENT_LABEL: &str = "the previous agent";
const ATTACHMENT_ONLY: &str = "[shared an attachment]";
const HANDOFF_HEADER: &str = "# Conversation handoff";
const SUMMARY_TRUNCATED_MARKER: &str = "[… summary truncated …]";

/// `HandoffSummaryResult`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Handoff {
    /// The framed summary alone; empty when the transcript gives none.
    pub summary_text: String,
    /// The first prompt's wire: the summary, then the draft.
    pub wire_blocks: Vec<Value>,
    /// The user bubble: the draft only.
    pub display_blocks: Vec<Value>,
}

/// JavaScript's `\s`, which `js_trim` also uses.
fn js_space(c: char) -> bool {
    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
}

fn trim(text: &str) -> &str {
    text.trim_matches(js_space)
}

/// The sentinels, separator and padding a composer token can leave behind.
fn token_noise(c: char) -> bool {
    matches!(c, '\u{e000}'..='\u{e007}' | '\u{1f}' | '\u{2007}')
}

/// `sanitizeWireText`, without the token rendering (see the module note).
fn sanitize_wire_text(value: &str) -> String {
    value.chars().filter(|c| !token_noise(*c)).collect()
}

/// `sanitizeFragment`: one line, every whitespace run one space.
fn sanitize_fragment(value: &str) -> String {
    let mut out = String::new();
    let mut in_space = false;
    for c in sanitize_wire_text(value).chars() {
        if js_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// `sanitizeHandoffWireBlocks`: text blocks swept; other blocks pass.
pub(crate) fn sanitize_wire_blocks(blocks: &[Value]) -> Vec<Value> {
    blocks
        .iter()
        .map(|block| match block.get("text").and_then(Value::as_str) {
            Some(text) if block.get("type").and_then(Value::as_str) == Some("text") => {
                json!({ "type": "text", "text": sanitize_wire_text(text) })
            }
            _ => block.clone(),
        })
        .collect()
}

fn truncate(value: &str, max: usize) -> String {
    if js::len(value) <= max {
        value.to_owned()
    } else {
        format!("{}…", js::head(value, max))
    }
}

/// `stripTermulPlanFences`: the persisted plan fences go; a fence that
/// started a line leaves that line break.
pub(crate) fn strip_plan_fences(text: &str) -> String {
    const OPEN: &str = "```termul-plan";
    if !text.contains("termul-plan") {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut rest = text;
    let mut at_start = true;
    loop {
        let found = if at_start && rest.starts_with(OPEN) {
            Some((0, false))
        } else {
            rest.find(&format!("\n{OPEN}")).map(|at| (at, true))
        };
        let Some((at, newline)) = found else {
            out.push_str(rest);
            return out;
        };
        let body = &rest[at + usize::from(newline) + OPEN.len()..];
        let Some(close) = fence_end(body) else {
            // No closing fence: this opening stays text.
            let keep = at + usize::from(newline) + OPEN.len();
            out.push_str(&rest[..keep]);
            rest = &rest[keep..];
            at_start = false;
            continue;
        };
        out.push_str(&rest[..at]);
        if newline {
            out.push('\n');
        }
        rest = &body[close..];
        at_start = false;
    }
}

/// Where a fence opened by ```` ```termul-plan ```` ends in `body`, as the
/// lazy `\r?\n[\s\S]*?\r?\n```\n?` matches it: just after the first
/// closing fence and one line break.
fn fence_end(body: &str) -> Option<usize> {
    let start = if body.starts_with("\r\n") {
        2
    } else if body.starts_with('\n') {
        1
    } else {
        return None;
    };
    let mut end = start + body[start..].find("\n```")? + 4;
    if body[end..].starts_with('\n') {
        end += 1;
    }
    Some(end)
}

fn first_string(input: Option<&Object>, keys: &[&str]) -> Option<String> {
    let input = input?;
    keys.iter().find_map(|key| {
        input
            .get(*key)
            .and_then(Value::as_str)
            .map(trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

fn first_number(input: Option<&Object>, keys: &[&str]) -> Option<f64> {
    let input = input?;
    keys.iter()
        .find_map(|key| input.get(*key).and_then(Value::as_f64))
        .filter(|number| number.is_finite())
}

/// `baseName`.
fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rfind(['/', '\\'])
        .map_or(trimmed, |at| &trimmed[at + 1..])
        .to_owned()
}

/// `lineRange`.
fn line_range(input: Option<&Object>) -> Option<String> {
    let start = first_number(
        input,
        &["startLine", "start_line", "start", "line", "lineStart"],
    );
    let end = first_number(input, &["endLine", "end_line", "end", "lineEnd"]);
    if let (Some(start), Some(end)) = (start, end) {
        return Some(format!(
            "L{}-{}",
            crate::js_number(start),
            crate::js_number(end)
        ));
    }
    let offset = first_number(input, &["offset"])?;
    let limit = first_number(input, &["limit", "count", "lines"]).filter(|limit| *limit > 0.0)?;
    Some(format!(
        "L{}-{}",
        crate::js_number(offset),
        crate::js_number(offset + limit)
    ))
}

const COMMAND_KEYS: &[&str] = &["command", "cmd", "script", "commandLine"];
const QUERY_KEYS: &[&str] = &["query", "pattern", "q", "search", "searchTerm", "regex"];
const URL_KEYS: &[&str] = &["url", "uri", "href", "link"];

/// `describeToolCall`'s verb, primary and detail.
fn describe_tool_call(call: &Object) -> [Option<String>; 3] {
    let input = call.get("rawInput").and_then(Value::as_object);
    let title = call
        .get("title")
        .and_then(Value::as_str)
        .map(|t| trim(t).to_owned());
    let kind = call.get("kind").and_then(Value::as_str);
    let running = matches!(
        call.get("status").and_then(Value::as_str),
        Some("pending" | "in_progress")
    );
    let verb = verb_for_kind(kind, running).to_owned();
    if let Some(subagent) = call.get("subagent") {
        let primary = subagent
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned);
        return [Some(String::new()), primary, None];
    }
    if call.contains_key("subagentControl") {
        let primary = title.unwrap_or_else(|| "Agent command".into());
        return [Some(String::new()), Some(primary), None];
    }
    let or = |found: Option<String>, fallback: &str| {
        found
            .or(title.clone())
            .unwrap_or_else(|| fallback.to_owned())
    };
    let path_primary = || {
        super::window::call_path(call)
            .map_or_else(|| title.clone().unwrap_or_else(|| "file".into()), base_name)
    };
    let (primary, detail) = match kind {
        Some("read") => (path_primary(), line_range(input)),
        Some("delete" | "move") => (path_primary(), None),
        Some("edit") => {
            let detail = super::window::diff_stat(call, "edit").map(|stat| {
                let count = |key: &str| stat.get(key).and_then(Value::as_u64).unwrap_or(0);
                match count("removed") {
                    0 => format!("+{}", count("added")),
                    removed => format!("+{} \u{2212}{removed}", count("added")),
                }
            });
            (path_primary(), detail)
        }
        Some("execute") => (or(first_string(input, COMMAND_KEYS), "command"), None),
        Some("search") => (or(first_string(input, QUERY_KEYS), ""), None),
        Some("fetch") => (or(first_string(input, URL_KEYS), ""), None),
        Some("think") => (
            or(
                first_string(input, &["thought", "text", "message"]),
                "Thinking",
            ),
            None,
        ),
        _ => (
            title
                .clone()
                .or_else(|| kind.map(str::to_owned))
                .unwrap_or_else(|| "Tool call".into()),
            None,
        ),
    };
    [Some(verb), Some(primary), detail]
}

/// `verbForKind` and `runningVerbForKind`.
fn verb_for_kind(kind: Option<&str>, running: bool) -> &'static str {
    let (done, now) = match kind {
        Some("read") => ("Read", "Reading"),
        Some("edit") => ("Edited", "Editing"),
        Some("delete") => ("Deleted", "Deleting"),
        Some("move") => ("Moved", "Moving"),
        Some("search") => ("Searched", "Searching"),
        Some("execute") => ("Ran", "Running"),
        Some("think") => ("Thinking", "Thinking"),
        Some("fetch") => ("Fetched", "Fetching"),
        Some("switch_mode") => ("Switched mode", "Switching mode"),
        _ => ("", ""),
    };
    if running { now } else { done }
}

/// `toolOneLiner`.
fn tool_one_liner(call: &Object) -> String {
    let parts: Vec<String> = describe_tool_call(call)
        .into_iter()
        .flatten()
        .filter(|part| !trim(part).is_empty())
        .collect();
    truncate(
        trim(&sanitize_fragment(&parts.join(" "))),
        MAX_FRAGMENT_CHARS,
    )
}

/// `messageText`.
fn message_text(message: &ChatMessage) -> String {
    let text: String = message
        .blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
        })
        .collect();
    let text = match message.role {
        Role::Agent => strip_plan_fences(&text),
        _ => text,
    };
    trim(&sanitize_fragment(&text)).to_owned()
}

enum Item<'a> {
    Message(&'a ChatMessage),
    Tool(&'a Object),
}

/// `mergeByArrival`: seq first, then timestamp, then input order; items
/// without a seq sort before every item with one.
fn merge_by_arrival<'a>(messages: &'a [ChatMessage], tools: &'a [Object]) -> Vec<Item<'a>> {
    let mut stamped: Vec<(Option<f64>, f64, usize, Item<'a>)> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        stamped.push((
            message.seq,
            message.timestamp,
            index,
            Item::Message(message),
        ));
    }
    for (index, tool) in tools.iter().enumerate() {
        if super::plan::is_plan_tool(tool) {
            continue;
        }
        let number = |key: &str| tool.get(key).and_then(Value::as_f64);
        let ts = number("timestamp").unwrap_or(0.0);
        stamped.push((number("seq"), ts, 1000 + index, Item::Tool(tool)));
    }
    stamped.sort_by(|a, b| {
        let by_seq = match (a.0, b.0) {
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            (None, None) => std::cmp::Ordering::Equal,
        };
        by_seq
            .then_with(|| match (a.0, b.0) {
                (Some(_), Some(_)) => std::cmp::Ordering::Equal,
                _ => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal),
            })
            .then(a.2.cmp(&b.2))
    });
    stamped.into_iter().map(|(.., item)| item).collect()
}

struct Turn {
    user_text: Option<String>,
    lines: Vec<String>,
}

/// `partitionTurns` with the handoff budget: each user message opens a
/// turn; agent text and tool one-liners join the open one.
fn partition_turns(items: &[Item<'_>]) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut open = false;
    for item in items {
        let line = match item {
            Item::Tool(tool) => {
                if tool.get("kind").and_then(Value::as_str) == Some("think") {
                    continue;
                }
                let one_liner = tool_one_liner(tool);
                if one_liner.is_empty() {
                    continue;
                }
                format!("- {one_liner}")
            }
            Item::Message(message) => {
                if message.role == Role::Thought || message.streaming {
                    continue;
                }
                let text = truncate(&message_text(message), MAX_FRAGMENT_CHARS);
                if message.role == Role::User {
                    let non_text = message
                        .blocks
                        .iter()
                        .any(|block| block.get("type").and_then(Value::as_str) != Some("text"));
                    let user_text = if !text.is_empty() {
                        Some(text)
                    } else if non_text {
                        Some(ATTACHMENT_ONLY.to_owned())
                    } else {
                        None
                    };
                    turns.push(Turn {
                        user_text,
                        lines: Vec::new(),
                    });
                    open = true;
                    continue;
                }
                if text.is_empty() {
                    continue;
                }
                format!("Agent: {text}")
            }
        };
        if !open {
            turns.push(Turn {
                user_text: None,
                lines: Vec::new(),
            });
            open = true;
        }
        turns.last_mut().expect("opened above").lines.push(line);
    }
    turns.retain(|turn| turn.user_text.is_some() || !turn.lines.is_empty());
    turns
}

fn turn_section(turn: &Turn) -> String {
    turn.user_text
        .iter()
        .map(|text| format!("User: {text}"))
        .chain(turn.lines.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `fitBodyBudget`: over the budget, keep the front matter and the first
/// user line, then as many of the newest lines as fit.
fn fit_body_budget(front: &[String], sections: &[String]) -> String {
    let all: Vec<&str> = front.iter().chain(sections).map(String::as_str).collect();
    let whole = all.join("\n\n");
    if js::len(&whole) <= MAX_BODY_CHARS {
        return whole;
    }
    let lines: Vec<&str> = sections.iter().flat_map(|s| s.split('\n')).collect();
    let first_user = lines.iter().position(|line| line.starts_with("User: "));
    let mut pinned: Vec<&str> = front.iter().map(String::as_str).collect();
    if let Some(at) = first_user {
        pinned.push(lines[at]);
    }
    let rest: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != first_user)
        .map(|(_, line)| *line)
        .collect();
    let mut kept: Vec<&str> = Vec::new();
    for line in rest.iter().rev() {
        let candidate: Vec<&str> = pinned
            .iter()
            .copied()
            .chain([SUMMARY_TRUNCATED_MARKER, line])
            .chain(kept.iter().copied())
            .collect();
        if js::len(&candidate.join("\n")) > MAX_BODY_CHARS {
            break;
        }
        kept.insert(0, line);
    }
    let mut out = vec![pinned.join("\n")];
    if kept.len() < rest.len() {
        out.push(SUMMARY_TRUNCATED_MARKER.to_owned());
    }
    out.extend(kept.iter().map(|line| (*line).to_owned()));
    out.join("\n")
}

/// `buildTranscriptBody`: the last turns, the first user line kept as the
/// intent when it fell out, and a note of how many turns were left out.
fn transcript_body(messages: &[ChatMessage], tools: &[Object]) -> String {
    let turns = partition_turns(&merge_by_arrival(messages, tools));
    let from = turns.len().saturating_sub(MAX_TURNS);
    let capped = &turns[from..];
    let mut front = Vec::new();
    if turns.len() > MAX_TURNS {
        let first_user = turns.iter().position(|turn| turn.user_text.is_some());
        let anchor_dropped = first_user.is_some_and(|at| at < from);
        if let (true, Some(at)) = (anchor_dropped, first_user) {
            let text = turns[at].user_text.as_deref().unwrap_or_default();
            front.push(format!("User: {text}"));
        }
        let dropped = turns.len() - capped.len() - usize::from(anchor_dropped);
        if dropped > 0 {
            let plural = if dropped == 1 { "" } else { "s" };
            front.push(format!("[… {dropped} earlier turn{plural} omitted …]"));
        }
    }
    let sections: Vec<String> = capped.iter().map(turn_section).collect();
    fit_body_budget(&front, &sections)
}

/// `buildHandoffSummary`.
pub(crate) fn build_handoff_summary(
    messages: &[ChatMessage],
    tools: &[Object],
    agent_name: Option<&str>,
    pending_text: &str,
) -> Handoff {
    let body = transcript_body(messages, tools);
    let label = sanitize_fragment(agent_name.unwrap_or_default());
    let label = match trim(&label) {
        "" => GENERIC_AGENT_LABEL,
        label => label,
    };
    let preamble = format!(
        "You are taking over a conversation previously handled by {label}. Summary of the prior conversation:"
    );
    let summary_text = if body.is_empty() {
        String::new()
    } else {
        format!("{HANDOFF_HEADER}\n\n{preamble}\n\n{body}")
    };
    let pending_display = trim(pending_text);
    let pending_wire = sanitize_wire_text(pending_text);
    let pending_wire = trim(&pending_wire);
    let wire_text = match (summary_text.is_empty(), pending_wire.is_empty()) {
        (false, false) => format!("{summary_text}\n\n---\n\n{pending_wire}"),
        (false, true) => summary_text.clone(),
        _ => pending_wire.to_owned(),
    };
    let text_block = |text: &str| vec![json!({ "type": "text", "text": text })];
    Handoff {
        wire_blocks: if wire_text.is_empty() {
            Vec::new()
        } else {
            text_block(&wire_text)
        },
        display_blocks: if pending_display.is_empty() {
            Vec::new()
        } else {
            text_block(pending_display)
        },
        summary_text,
    }
}
