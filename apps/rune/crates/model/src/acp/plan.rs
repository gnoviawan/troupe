//! One plan per chat session, from a plan tool call (the host `plan` tool,
//! Codex `update_plan`, Claude `TodoWrite`, Cursor `updateTodos`). Port of
//! `src/renderer/lib/plan-session.ts`; that file's header states the rules.

use serde_json::Value;

use super::types::{Object, PlanEntry};
use crate::js_trim;

const PLAN_TOOL_NAMES: [&str; 6] = [
    "plan",
    "todowrite",
    "todo_write",
    "update_plan",
    "updatetodos",
    "update_todos",
];
const MERGE_TOOL_NAMES: [&str; 4] = ["todowrite", "todo_write", "updatetodos", "update_todos"];
const PLAN_LIST_KEYS: [&str; 4] = ["todos", "plan", "entries", "tasks"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    New,
    Update,
    Clear,
}

fn non_empty(value: Option<&Value>) -> Option<&str> {
    let text = js_trim(value?.as_str()?);
    (!text.is_empty()).then_some(text)
}

/// `planToolLeaf`: `mcp__server__plan` and `server:plan` both become `plan`.
fn leaf(name: &str) -> String {
    let slashed = name.split(['/', ':']).next_back().unwrap_or(name);
    let leaf = slashed.rsplit("__").next().unwrap_or(slashed);
    js_trim(leaf).to_lowercase().replace('-', "_")
}

fn leaves(tool: &Object) -> Vec<String> {
    let meta = tool.get("_meta");
    let input = tool.get("rawInput").filter(|input| input.is_object());
    [
        tool.get("title"),
        meta.and_then(|meta| meta.get("claudeCode"))
            .filter(|code| code.is_object())
            .and_then(|code| code.get("toolName")),
        input.and_then(|input| input.get("_toolName")),
        input.and_then(|input| input.get("toolName")),
    ]
    .into_iter()
    .filter_map(non_empty)
    .map(leaf)
    .collect()
}

/// `isPlanToolCall`.
fn is_plan_tool(tool: &Object) -> bool {
    tool.get("planTool") == Some(&Value::Bool(true))
        || leaves(tool)
            .iter()
            .any(|leaf| PLAN_TOOL_NAMES.contains(&leaf.as_str()))
}

/// `markPlanTool`: stamp `planTool` so a restored call stays hidden.
pub(crate) fn mark(tool: &mut Object) {
    if tool.get("planTool") != Some(&Value::Bool(true)) && is_plan_tool(tool) {
        tool.insert("planTool".into(), Value::Bool(true));
    }
}

/// `planEventHasList`.
pub(crate) fn has_list(tool: &Object) -> bool {
    tool.get("rawInput")
        .and_then(Value::as_object)
        .is_some_and(|input| {
            PLAN_LIST_KEYS
                .iter()
                .any(|key| input.get(*key).is_some_and(Value::is_array))
        })
}

fn normalize_status(value: &str) -> String {
    let status: String = value
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '-' {
                '_'
            } else {
                c
            }
        })
        .collect();
    match status.as_str() {
        "in_progress" | "inprogress" | "running" => "in_progress".into(),
        "completed" | "complete" | "done" => "completed".into(),
        "cancelled" | "canceled" => "cancelled".into(),
        "pending" | "todo" | "not_started" => "pending".into(),
        _ => value.to_owned(),
    }
}

fn entry_from_item(item: &Value) -> Option<PlanEntry> {
    let record = item.as_object()?;
    let content = ["content", "step", "text"]
        .into_iter()
        .find_map(|key| non_empty(record.get(key)))?;
    let priority = non_empty(record.get("priority"))
        .map(str::to_lowercase)
        .filter(|p| matches!(p.as_str(), "high" | "medium" | "low"));
    let detail = non_empty(record.get("detail"))
        .or_else(|| non_empty(record.get("description")))
        .filter(|detail| *detail != content);
    Some(PlanEntry {
        content: content.to_owned(),
        id: non_empty(record.get("id")).map(|id| Value::String(id.to_owned())),
        status: non_empty(record.get("status")).map(normalize_status),
        priority,
        detail: detail.map(str::to_owned),
        rest: Object::new(),
    })
}

fn list_from_input(input: &Object) -> Option<Vec<PlanEntry>> {
    PLAN_LIST_KEYS.iter().find_map(|key| {
        let items = input.get(*key)?.as_array()?;
        Some(items.iter().filter_map(entry_from_item).collect())
    })
}

fn merge(current: Option<&Vec<PlanEntry>>, incoming: Vec<PlanEntry>) -> Vec<PlanEntry> {
    let mut next = current.cloned().unwrap_or_default();
    for item in incoming {
        let id = item.id_str().unwrap_or_default().to_owned();
        let index = if id.is_empty() {
            next.iter().position(|entry| entry.content == item.content)
        } else {
            next.iter()
                .position(|entry| entry.id_str() == Some(id.as_str()))
        };
        match index {
            Some(index) => {
                let entry = &mut next[index];
                entry.content = item.content;
                entry.id = item.id.or(entry.id.take());
                entry.status = item.status.or(entry.status.take());
                entry.priority = item.priority.or(entry.priority.take());
                entry.detail = item.detail.or(entry.detail.take());
            }
            None => next.push(item),
        }
    }
    next
}

fn keys(entry: &PlanEntry) -> Vec<String> {
    let mut keys = vec![format!("content:{}", js_trim(&entry.content))];
    if let Some(id) = entry.id_str().map(js_trim).filter(|id| !id.is_empty()) {
        keys.push(format!("id:{id}"));
    }
    keys
}

/// `classifyFullPlan`.
fn classify(current: Option<&Vec<PlanEntry>>, incoming: &[PlanEntry]) -> Change {
    if incoming.is_empty() {
        return Change::Clear;
    }
    let Some(current) = current.filter(|current| !current.is_empty()) else {
        return Change::New;
    };
    let previous: Vec<String> = current.iter().flat_map(keys).collect();
    if incoming
        .iter()
        .flat_map(keys)
        .any(|key| previous.contains(&key))
    {
        Change::Update
    } else {
        Change::New
    }
}

fn same(current: Option<&Vec<PlanEntry>>, next: &[PlanEntry]) -> bool {
    current.is_some_and(|current| {
        current.len() == next.len()
            && current.iter().zip(next).all(|(a, b)| {
                a.content == b.content
                    && a.status == b.status
                    && a.priority == b.priority
                    && a.id == b.id
                    && a.detail == b.detail
            })
    })
}

/// `resolvePlanToolCall`: the plan a plan tool call sets, or `None` when it
/// is not a plan tool or its arguments are not ready.
fn resolve(current: Option<&Vec<PlanEntry>>, tool: &Object) -> Option<(Vec<PlanEntry>, Change)> {
    if !is_plan_tool(tool) {
        return None;
    }
    let input = tool.get("rawInput")?.as_object()?;
    let incoming = list_from_input(input)?;
    let merge_flag = input.get("merge") == Some(&Value::Bool(true))
        && (leaves(tool)
            .iter()
            .any(|leaf| MERGE_TOOL_NAMES.contains(&leaf.as_str()))
            || tool.get("planMerge") == Some(&Value::Bool(true)));
    if incoming.is_empty() {
        let status = tool.get("status").and_then(Value::as_str);
        if merge_flag || !matches!(status, Some("completed" | "failed")) {
            return None;
        }
        return Some((Vec::new(), Change::Clear));
    }
    let entries = if merge_flag {
        merge(current, incoming)
    } else {
        incoming
    };
    let change = classify(current, &entries);
    Some((entries, change))
}

/// `plansAfterToolCall`: the session's plan after this tool call, or `None`
/// when it does not change. `Some(None)` clears it.
pub(crate) fn after_tool_call(
    current: Option<&Vec<PlanEntry>>,
    tool: &Object,
) -> Option<(Option<Vec<PlanEntry>>, Change)> {
    let (entries, change) = resolve(current, tool)?;
    if change == Change::Clear {
        return current.is_some().then_some((None, Change::Clear));
    }
    if same(current, &entries) {
        return None;
    }
    Some((Some(entries), change))
}
